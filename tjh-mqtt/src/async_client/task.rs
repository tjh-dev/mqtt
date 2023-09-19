use super::{
	command::{Command, PublishCommand, UnsubscribeCommand},
	holdoff::HoldOff,
	mqtt_stream::MqttStream,
	state::StateError,
};
use crate::{async_client::command::SubscribeCommand, packets, FilterBuf, Packet, PacketType, QoS};
use std::ops::{ControlFlow, ControlFlow::Continue};
use tokio::{
	sync::{mpsc, oneshot},
	time::{self, Instant},
};

type CommandRx = mpsc::UnboundedReceiver<Command>;
type ClientState = super::state::ClientState<
	mpsc::Sender<packets::Publish>,
	oneshot::Sender<()>,
	oneshot::Sender<Vec<(FilterBuf, QoS)>>,
>;

pub async fn client_task(
	state: &mut ClientState,
	command_channel: &mut CommandRx,
	connection: &mut MqttStream,
	reconnect_delay: &mut HoldOff,
) -> crate::Result<ControlFlow<(), ()>> {
	let connect_packet: Packet = state.connect.clone().into();
	connection.write_packet(&connect_packet).await?;

	// Wait for ConnAck
	#[rustfmt::skip]
	let packets::ConnAck { session_present, .. } = tokio::select! {
	  result = connection.read_packet() => {
      match result {
        Ok(Some(Packet::ConnAck(connack))) => {
					tracing::info!(packet = ?connack, "read from stream");
          if connack.code == 0x80 {
            return Ok(Continue(()))
          }
          // state.connect.clean_session = false;
					reconnect_delay.reset();
          connack
        }
        _ => return Ok(Continue(())),
      }
	  }
    _ = time::sleep(state.keep_alive) => {
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
	if !session_present && state.has_active_subscriptions() {
		// Run-reconnect logic.
		let (tx, rx) = oneshot::channel();
		connection
			.write_packet(&state.generate_resubscribe(tx).unwrap())
			.await?;
		let Ok(Some(Packet::SubAck(suback))) = connection.read_packet().await else {
			tracing::error!("failed to read SubAck");
			return Err("failed to read suback".into());
		};
		if state.suback(suback).is_err() {
			return Ok(Continue(()));
		};
		rx.await?;
	}

	let mut disconnecting = false;
	let mut keep_alive = time::interval(state.keep_alive);
	keep_alive.tick().await;

	while !disconnecting {
		#[rustfmt::skip]
    tokio::select! {
      Some(command) = command_channel.recv() => {
        match process_command(state, command).await {
					Ok(should_disconnect) => {
						disconnecting = should_disconnect;
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

				tracing::info!(packet = ?packet, "read from stream");
				if process_packet(state, packet).await.is_err() {
					return Ok(Continue(()));
				}
      }
      _ = keep_alive.tick() => {
				if state.outgoing.is_empty() {
					if state.expired() {
						tracing::error!("pending requests have exceeded keep_alive");
						return Ok(Continue(()));
					}
					state.pingreq_state = Some(Instant::now());
					state.outgoing.push_front(Packet::PingReq);
				}
      }
    }

		for packet in state.outgoing.drain(..) {
			tracing::info!(packet = ?packet, "writing to stream");
			connection.write_packet(&packet).await?;
		}
	}

	Ok(ControlFlow::Break(()))
}

async fn process_packet(state: &mut ClientState, packet: Packet) -> Result<(), StateError> {
	use packets::Publish;

	match packet {
		Packet::Publish(publish) => match publish {
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
				// TODO: Fix
				state.outgoing.push_back(packets::PubComp { id }.into());
				return Ok(());
			};
			let Some(channel) = state.find_publish_channel(publish.topic()) else {
				return Err(StateError::DeliveryFailure(publish));
			};

			if let Err(publish) = channel.send(publish).await {
				state.incoming.insert(id, publish.0);
				return Err(StateError::HardDeliveryFailure);
			};

			Ok(())
		}
		Packet::PubComp(packets::PubComp { id }) => {
			let response = state.pubcomp(id)?;
			let _ = response.send(());
			Ok(())
		}
		Packet::SubAck(ack) => {
			let (sender, payload) = state.suback(ack)?;
			let _ = sender.send(payload);
			Ok(())
		}
		Packet::UnsubAck(ack) => state.unsuback(ack),
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
			response_tx,
		}) => {
			if let Some(response) = state.publish(topic, payload, qos, retain, response_tx) {
				let _ = response.send(());
			};
		}
		Command::Subscribe(SubscribeCommand {
			filters,
			publish_tx,
			response_tx,
		}) => {
			state.subscribe(filters, publish_tx, response_tx);
		}
		Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response_tx,
		}) => {
			state.unsubscribe(filters, response_tx);
		}
		Command::PublishComplete { id } => {
			state.outgoing.push_back(packets::PubComp { id }.into());
		}
	}
	Ok(false)
}
