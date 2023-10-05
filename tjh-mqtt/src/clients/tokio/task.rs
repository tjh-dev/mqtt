use super::{mqtt_stream::MqttStream, Command, CommandRx, HoldOff, StateError};
use crate::{
	clients::command::{PublishCommand, SubscribeCommand, UnsubscribeCommand},
	packets, FilterBuf, Packet, PacketType, QoS,
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
	mpsc::Sender<packets::Publish>,
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
	let connect_packet: Packet = state.connect.clone().into();
	connection.write_packet(&connect_packet).await?;

	let sleep = time::sleep(state.keep_alive);
	tokio::pin!(sleep);

	// Wait for ConnAck
	#[rustfmt::skip]
	let packets::ConnAck { session_present, .. } = tokio::select! {
		result = connection.read_packet() => {
			match result {
				Ok(Some(Packet::ConnAck(connack))) => {
					tracing::trace!(packet = ?connack, "read from stream");
					if connack.code == 0x80 {
						return Ok(Continue(()))
					}
					reconnect_delay.reset();
					connack
				}
				_ => return Ok(Continue(())),
			}
		}
		_ = &mut sleep => {
			return Ok(Continue(()))
		}
	};

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
		if let Some(packet) = state.generate_resubscribe(tx) {
			connection.write_packet(&packet).await?;
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
			Ok(packet) = connection.read_packet() => {
				let Some(packet) = packet else {
					tracing::warn!("connection reset by peer");
					return Ok(Continue(()))
				};

				tracing::debug!(packet = ?packet, "read from stream");
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
					state.outgoing.push_back(Packet::PingReq);
				}
			}
		}

		let update_keep_alive = !state.outgoing.is_empty();
		for packet in state.outgoing.drain(..) {
			tracing::trace!(packet = ?packet, "writing to stream");
			connection.write_packet(&packet).await?;
		}

		if update_keep_alive {
			// We've just sent a packet, update the keep alive.
			keep_alive.reset_at((Instant::now() + state.keep_alive).into());
		}
	}

	Ok(ControlFlow::Break(()))
}

async fn process_packet(state: &mut ClientState, packet: Packet) -> Result<(), StateError> {
	use packets::Publish;

	match packet {
		Packet::Publish(publish) => match *publish {
			Publish::AtMostOnce {
				retain,
				topic,
				payload,
			} => {
				let Some(channel) = state.find_publish_channel(&topic) else {
					panic!();
				};

				channel
					.send(Publish::AtMostOnce {
						retain,
						topic,
						payload,
					})
					.await
					.map_err(|p| StateError::DeliveryFailure(p.0))?;

				Ok(())
			}
			Publish::AtLeastOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				let Some(channel) = state.find_publish_channel(&topic) else {
					panic!();
				};

				channel
					.send(Publish::AtLeastOnce {
						id,
						retain,
						duplicate,
						topic,
						payload,
					})
					.await
					.map_err(|p| StateError::DeliveryFailure(p.0))?;
				state.outgoing.push_back(packets::PubAck { id }.into());
				Ok(())
			}
			Publish::ExactlyOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				state.incoming.insert(
					id,
					Publish::ExactlyOnce {
						id,
						retain,
						duplicate,
						topic,
						payload,
					},
				);
				state.outgoing.push_back(packets::PubRec { id }.into());
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

			let Some(channel) = state.find_publish_channel(publish.topic()) else {
				return Err(StateError::DeliveryFailure(publish));
			};

			if let Err(publish) = channel.send(publish).await {
				state.incoming.insert(id, publish.0);
				return Err(StateError::HardDeliveryFailure);
			};

			// We've successfully passed on the Publish message. Queue up a PubComp
			// packet
			state.outgoing.push_back(packets::PubComp { id }.into());

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
			state.outgoing.push_back(Packet::Disconnect);
			return Ok(true);
		}
		Command::Publish(PublishCommand {
			topic,
			payload,
			qos,
			retain,
			response: response_tx,
		}) => {
			if let Some(response) = state.publish(topic, payload, qos, retain, response_tx) {
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
