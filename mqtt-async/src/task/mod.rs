use crate::{
	command::{Command, CommandRx},
	connection::Connection,
	Options,
};
use mqtt_core::{Connect, Packet, Publish, QoS};
use std::{collections::HashMap, str::from_utf8};
use tokio::{
	net::TcpStream,
	sync::oneshot,
	time::{self, Instant},
};

#[tracing::instrument(skip(options, rx), ret, err)]
pub async fn client_task(options: Options, mut rx: CommandRx) -> mqtt_core::Result<()> {
	//

	let stream = TcpStream::connect((options.host, options.port)).await?;
	let mut connection = Connection::new(stream);
	let mut keep_alive = time::interval(time::Duration::from_secs(options.keep_alive as u64));

	let mut awaiting_suback: HashMap<u16, oneshot::Sender<Vec<Option<QoS>>>> = Default::default();
	let mut awaiting_puback: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut awaiting_pubrec: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut awaiting_pubcomp: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut id = 0;

	connection
		.write_packet(&Packet::Connect(Connect {
			client_id: options.client_id,
			keep_alive: options.keep_alive,
			clean_session: options.clean_session,
			..Default::default()
		}))
		.await?;

	let Some(Packet::ConnAck {
		session_present,
		code,
	}) = connection.read_packet().await?
	else {
		return Ok(());
	};

	tracing::info!("connected! session_present = {session_present}, code = {code}");

	let mut pingreq_sent: Option<Instant> = None;

	// Discard the first tick from the keep-alive interval.
	let _ = keep_alive.tick().await;

	loop {
		tokio::select! {
			Some(command) = rx.recv() => {
				tracing::debug!(?command);
				match command {
					Command::Publish { topic, payload, qos, retain, tx } => {
						let packet = match qos {
							QoS::AtMostOnce => Packet::Publish(Publish::AtMostOnce {
								topic,
								payload,
								retain,
							}),
							QoS::AtLeastOnce => {
								id += 1;
								Packet::Publish(Publish::AtLeastOnce {
									id,
									topic,
									payload,
									retain,
									duplicate: false
								})
							}
							QoS::ExactlyOnce => {
								id += 1;
								Packet::Publish(Publish::ExactlyOnce {
									id,
									topic,
									payload,
									retain,
									duplicate: false
								})
							}
						};

						let tx = match qos {
							QoS::AtMostOnce => Some(tx),
							QoS::AtLeastOnce => {
								awaiting_puback.insert(id, tx);
								None
							}
							QoS::ExactlyOnce => {
								awaiting_pubrec.insert(id, tx);
								None
							}
						};

						connection.write_packet(&packet).await?;
						if let Some(tx) = tx {
							tx.send(()).unwrap();
						}
					}
					Command::Subscribe { filters, tx } => {
						id += 1;
						awaiting_suback.insert(id, tx);
						connection.write_packet(&Packet::Subscribe { id, filters }).await?;
					}
					Command::Shutdown => {
						connection.write_packet(&Packet::Disconnect).await?;

						tracing::debug!("closing client_task with {} inflight subscriptions", awaiting_suback.len());
						drop(awaiting_suback);
						break Ok(())
					}
				}
			}
			Ok(packet) = connection.read_packet() => {
				match packet {
					Some(Packet::Publish(publish)) => {
						match publish {
							Publish::AtMostOnce { .. } => {}
							Publish::AtLeastOnce { id, .. } => {
								connection.write_packet(&Packet::PubAck { id }).await?;
							}
							Publish::ExactlyOnce { id, .. } => {
								connection.write_packet(&Packet::PubRec { id }).await?;
							}
						}

						match from_utf8(publish.payload()) {
							Ok(_) => {
								// println!("{payload}");
							}
							Err(_) => {
								//
							}
						}
					}
					Some(Packet::PubAck { id }) => {
						let Some(sent) = awaiting_puback.remove(&id) else {
							tracing::error!("unsolicited PubAck with id {id}");
							break Err("unsolicited PubAck".into())
						};

						sent.send(()).unwrap();
					}
					Some(Packet::PubRec { id }) => {
						let Some(sent) = awaiting_pubrec.remove(&id) else {
							tracing::error!("unsolicited PubRec with id {id}");
							break Err("unsolicited PubRec".into())
						};

						awaiting_pubcomp.insert(id, sent);
						connection.write_packet(&Packet::PubRel { id }).await?;
					}
					Some(Packet::PubRel { id }) => {
						connection.write_packet(&Packet::PubComp { id }).await?;
					}
					Some(Packet::PubComp { id }) => {
						let Some(sent) = awaiting_pubcomp.remove(&id) else {
							tracing::error!("unsolicited PubComp with id {id}");
							break Err("unsolicited PubComp".into())
						};

						sent.send(()).unwrap();
					}
					Some(Packet::SubAck { id, result }) => {
						//
						let Some(tx) = awaiting_suback.remove(&id) else {
							tracing::error!("unsolicited SubAck with id {id}");
							break Err("unsolicited SubAck".into())
						};
						tx.send(result).unwrap();
					}
					Some(Packet::PingResp) => {
						if let Some(sent) = pingreq_sent.take() {
							tracing::info!("PingResp received in {:?}", sent.elapsed());
						}
					}
					_ => {}
				}
			}
			_ = keep_alive.tick() => {
				pingreq_sent.replace(Instant::now());
				connection.write_packet(&Packet::PingReq).await?;
			}
			else => {
				tracing::warn!("ending client task");
				break Ok(())
			}
		}
	}
}
