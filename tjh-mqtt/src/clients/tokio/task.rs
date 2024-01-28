use super::{mqtt_stream::MqttStream, Command, CommandRx, HoldOff, StateError};
use crate::{
	clients::{
		command::{PublishCommand, SubscribeCommand, UnsubscribeCommand},
		Message,
	},
	packets::{self, DeserializePacket},
	FilterBuf, Packet, PacketType, QoS,
};
use std::{
	ops::{ControlFlow, ControlFlow::Continue},
	time::Instant,
};
use tokio::{
	sync::{mpsc, oneshot},
	time,
};

type ClientState = super::ClientState<
	mpsc::Sender<Message>,
	oneshot::Sender<()>,
	oneshot::Sender<Vec<(FilterBuf, QoS)>>,
	oneshot::Sender<()>,
>;

pub async fn preconnect_task(
	state: &mut ClientState,
	command_channel: &mut CommandRx,
	connection: &mut MqttStream,
	reconnect_delay: &mut HoldOff,
) -> crate::Result<ControlFlow<(), ()>> {
	use packets::ConnAck;

	// Send a Connect packet to the Server. `connect` is a `Bytes`, so this clone
	// should be cheap.
	state.reconnect();
	connection.write(state.buffer().unwrap()).await?;

	let sleep = time::sleep(state.keep_alive);
	tokio::pin!(sleep);

	// Wait for ConnAck
	let frame = tokio::select! {
		Ok(Some(frame)) = connection.read_frame() => frame,
		_ = &mut sleep => return Ok(Continue(())),
	};

	let connack = ConnAck::from_frame(&frame)?;
	let session_present = connack.session_present;

	// TODO: Check return code.

	reconnect_delay.reset();
	connected_task(state, command_channel, connection, session_present).await
}

async fn connected_task(
	state: &mut ClientState,
	command_channel: &mut CommandRx,
	connection: &mut MqttStream,
	session_present: bool,
) -> crate::Result<ControlFlow<(), ()>> {
	//
	// We've just connected to the Server and received a ConnAck packet.
	//
	// Check if we should attempt to re-subscribe to all the active topic filters
	// in the Client's state.
	//
	if !session_present && state.has_active_subscriptions() {
		let (tx, rx) = oneshot::channel();
		if state.generate_resubscribe(tx) {
			let buffer = state.outgoing.split().freeze();
			connection.write(buffer).await?;
		}

		tokio::spawn(async move { tracing::debug!(?rx.await) });
	}

	let mut should_shutdown = false;
	let mut keep_alive =
		time::interval_at((Instant::now() + state.keep_alive).into(), state.keep_alive);

	while !should_shutdown {
		#[rustfmt::skip]
		tokio::select! {
			Some(command) = command_channel.recv() => {
				match process_command(state, *command).await {
					Ok(shutdown) => {
						should_shutdown = shutdown;
					}
					Err(error) => {
						tracing::error!(error = ?error, "failed to process command");
						return Ok(Continue(()))
					}
				}
			}
			Ok(frame) = connection.read_frame() => {
				let Some(frame) = frame else {
					tracing::warn!("connection reset by peer");
					return Ok(Continue(()))
				};

				tracing::debug!(packet = ?frame, "read from stream");
				let packet: Packet = Packet::parse(&frame)?;
				if process_packet(state, packet).await.is_err() {
					return Ok(Continue(()));
				}
			}
			_ = keep_alive.tick() => {
				if state.expired() {
					tracing::error!("pending requests have exceeded keep_alive");
					return Ok(Continue(()));
				}

				// If we are about to send a packet to the Server, we don't need to send a PingReq.
				if state.outgoing.is_empty() {
					state.pingreq_state = Some(Instant::now());
					state.enqueue_packet(&packets::PingReq);
				}
			}
		}

		let update_keep_alive = if !state.outgoing.is_empty() {
			let buffer = state.outgoing.split().freeze();
			connection.write(buffer).await?;
			true
		} else {
			false
		};

		if update_keep_alive {
			// We've just sent a packet, update the keep alive.
			keep_alive.reset_at((Instant::now() + state.keep_alive).into());
		}
	}

	Ok(ControlFlow::Break(()))
}

async fn process_packet<'a>(
	state: &'a mut ClientState,
	packet: Packet<'a>,
) -> Result<(), StateError<'a>> {
	use packets::Publish;

	match packet {
		Packet::Publish(publish) => match *publish {
			Publish::AtMostOnce {
				retain,
				topic,
				payload,
			} => {
				let Some(channel) = state.find_publish_channel(topic) else {
					panic!();
				};

				channel
					.send(Message {
						topic: topic.to_topic_buf(),
						retain,
						payload,
					})
					.await
					.unwrap();
				// .map_err(|p| StateError::DeliveryFailure())?;

				Ok(())
			}
			Publish::AtLeastOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				if duplicate {
					unimplemented!("duplicate Publish packets are not yet handled");
				}

				let Some(channel) = state.find_publish_channel(topic) else {
					panic!();
				};

				channel
					.send(Message {
						topic: topic.to_topic_buf(),
						retain,
						payload,
					})
					.await
					.unwrap();
				// .map_err(|p| StateError::DeliveryFailure(p.0))?;

				state.enqueue_packet(&packets::PubAck { id });

				Ok(())
			}
			Publish::ExactlyOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				if duplicate {
					unimplemented!("duplicate Publish packets are not yet handled");
				}

				state.incoming.insert(
					id,
					Message {
						topic: topic.to_topic_buf(),
						retain,
						payload,
					},
				);

				state.enqueue_packet(&packets::PubRec { id });

				Ok(())
			}
		},
		Packet::PubAck(packets::PubAck { id }) => {
			let response = state.puback(id)?;
			let _ = response.send(());
			Ok(())
		}
		Packet::PubRec(packets::PubRec { id }) => {
			state.pubrec(id)?;
			Ok(())
		}
		Packet::PubRel(packets::PubRel { id }) => {
			let Ok(publish) = state.pubrel(id) else {
				return Err(StateError::ProtocolError(
					"received PubRel for unknown Publish id",
				));
			};

			let Some(channel) = state.find_publish_channel(&publish.topic) else {
				panic!();
				// return Err(StateError::DeliveryFailure(publish));
			};

			if let Err(publish) = channel.send(publish).await {
				state.incoming.insert(id, publish.0);
				return Err(StateError::HardDeliveryFailure);
			};

			// We've successfully passed on the Publish message. Queue up a PubComp
			// packet
			state.enqueue_packet(&packets::PubComp { id });

			Ok(())
		}
		Packet::PubComp(packets::PubComp { id }) => {
			let response = state.pubcomp(id)?;
			let _ = response.send(());
			Ok(())
		}
		Packet::SubAck(ack) => {
			let (sender, payload) = state.suback(*ack)?;
			let _ = sender.send(payload);
			Ok(())
		}
		Packet::UnsubAck(ack) => {
			let response = state.unsuback(ack)?;
			let _ = response.send(());
			Ok(())
		}
		Packet::PingResp => {
			let Some(req) = state.pingreq_state.take() else {
				tracing::error!("unsolicited PingResp");
				return Err(StateError::Unsolicited(PacketType::PingResp));
			};
			tracing::info!(elapsed = ?req.elapsed(), "PingResp recevied");
			Ok(())
		}
		Packet::Connect(_)
		| Packet::ConnAck { .. }
		| Packet::Subscribe { .. }
		| Packet::Unsubscribe { .. }
		| Packet::PingReq
		| Packet::Disconnect => Err(StateError::InvalidPacket),
	}
}

async fn process_command(state: &mut ClientState, command: Command) -> Result<bool, StateError> {
	match command {
		Command::Shutdown => {
			// TODO: This shutdown process could be better.
			state.enqueue_packet(&packets::Disconnect);
			return Ok(true);
		}
		Command::Publish(PublishCommand {
			topic,
			payload,
			qos,
			retain,
			response: response_tx,
		}) => {
			if let Some(response) = state.publish(&topic, payload, qos, retain, response_tx) {
				let _ = response.send(());
			};
		}
		Command::Subscribe(SubscribeCommand {
			filters,
			channel: publish_tx,
			response: response_tx,
		}) => {
			state.subscribe(filters, publish_tx, response_tx);
		}
		Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response: response_tx,
		}) => {
			state.unsubscribe(filters, response_tx);
		}
	}
	Ok(false)
}
