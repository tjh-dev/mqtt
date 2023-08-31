use crate::{
	client::Subscription,
	command::{Command, CommandRx, CommandTx},
	connection::Connection,
	Options,
};
use mqtt_core::{Connect, Packet, Publish, QoS};
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
	time::Duration,
};
use tokio::{
	net::TcpStream,
	sync::{mpsc, oneshot},
	time::{self, Instant},
};

use self::holdoff::HoldOff;

mod holdoff;

#[tracing::instrument(skip(options, command_tx, rx), ret, err)]
pub async fn client_task(
	options: Options,
	command_tx: CommandTx,
	mut rx: CommandRx,
) -> mqtt_core::Result<()> {
	//
	// Build the Connect packet.
	let connect = Packet::Connect(Connect {
		client_id: options.client_id.clone(),
		keep_alive: options.keep_alive,
		clean_session: options.clean_session,
		..Default::default()
	});

	let mut awaiting_suback: HashMap<u16, (Arc<Vec<String>>, oneshot::Sender<Subscription>)> =
		Default::default();
	let mut awaiting_puback: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut awaiting_pubrec: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut awaiting_pubcomp: HashMap<u16, oneshot::Sender<()>> = Default::default();
	let mut id = 0;

	let mut awaiting_pubrel: HashSet<u16> = Default::default();
	let mut awaiting_outgoing_pubcomp: HashSet<u16> = Default::default();
	let mut queued_pubcomp: HashSet<u16> = Default::default();

	let mut subscriptions: HashMap<String, mpsc::Sender<Publish>> = Default::default();

	let mut holdoff =
		HoldOff::new(Duration::from_millis(50)..Duration::from_secs(options.keep_alive as u64));
	loop {
		// Use a hold-off when reconnecting. On the first connection attempt, this
		// won't wait at all.
		holdoff.wait().await;
		holdoff.increase_with(|delay| delay * 2);
		tracing::debug!("{holdoff:?}");

		// Open the the connection to the broker.
		let Ok(stream) = TcpStream::connect((options.host.as_str(), options.port)).await else {
			continue;
		};
		let mut connection = Connection::new(stream);

		// Send the Connect packet.
		connection.write_packet(&connect).await?;

		match wait_for_connack(
			&mut connection,
			time::Duration::from_secs(options.keep_alive as u64),
		)
		.await?
		{
			ConnAckResult::Continue { session_present } => {
				tracing::debug!("connected! session_present = {session_present}");
				holdoff.reset();
			}
			ConnAckResult::Timeout => {
				tracing::error!("timeout waiting for ConnAck");
				continue;
			}
		}

		let mut pingreq_sent: Option<Instant> = None;

		// Discard the first tick from the keep-alive interval.
		let mut keep_alive = time::interval(time::Duration::from_secs(options.keep_alive as u64));
		let _ = keep_alive.tick().await;

		// let mut command_queue = VecDeque::new();

		loop {
			// while let Some(command) = command_queue.pop_front() {
			// 	//
			// }

			tokio::select! {
				Some(command) = rx.recv() => {
					tracing::debug!(?command);
					// command_queue.push_back(command);
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
							let packet = Packet::Subscribe { id, filters };
							connection.write_packet(&packet).await?;

							let Packet::Subscribe { id, filters } = packet else {
								panic!();
							};
							let just_filters = filters.into_iter().map(|(f, _)| f).collect();
							awaiting_suback.insert(id, (Arc::new(just_filters), tx));
						}
						Command::Unsubscribe { filters, .. } => {
							//
							tracing::error!("should unsubscribe {filters:?}");
						}
						Command::PublishComplete { id } => {
							if awaiting_outgoing_pubcomp.remove(&id) {
								connection.write_packet(&Packet::PubComp { id }).await?;
								tracing::info!("PubComp sent");
							} else {
								queued_pubcomp.insert(id);
								tracing::info!("PubComp queued");
							}
						}
						Command::Shutdown => {
							connection.write_packet(&Packet::Disconnect).await?;

							tracing::debug!("closing client_task with {} inflight subscriptions", awaiting_suback.len());
							drop(awaiting_suback);
							return Ok(())
						}
					}
				}
				Ok(packet) = connection.read_packet() => {
					tracing::trace!(?packet);
					match packet {
						Some(Packet::Publish(publish)) => {
							if let Some(channel) = subscriptions.get(publish.topic()) {

								tracing::info!("found channel for {}", publish.topic());

								let qos = publish.qos();
								let id = publish.id();
								channel.send(publish).await?;
								match (qos, id) {
									(QoS::AtMostOnce, None) => {
										//
									}
									(QoS::AtLeastOnce, Some(id)) => {
										// Send a PubAck
										connection.write_packet(&Packet::PubAck { id }).await?;
									}
									(QoS::ExactlyOnce, Some(id)) => {
										// Send a PubRec
										awaiting_pubrel.insert(id);
										connection.write_packet(&Packet::PubRec { id }).await?;
									},
									_ => panic!()
								}
							}
						}
						Some(Packet::PubAck { id }) => {
							let Some(sent) = awaiting_puback.remove(&id) else {
								tracing::error!("unsolicited PubAck with id {id}");
								break
							};

							sent.send(()).unwrap();
						}
						Some(Packet::PubRec { id }) => {
							let Some(sent) = awaiting_pubrec.remove(&id) else {
								tracing::error!("unsolicited PubRec with id {id}");
								break
							};

							awaiting_pubcomp.insert(id, sent);
							connection.write_packet(&Packet::PubRel { id }).await?;
						}
						Some(Packet::PubRel { id }) => {
							// Check to see if we're expecting a PubRel.
							if !awaiting_pubrel.remove(&id) {
								break
							}

							if queued_pubcomp.remove(&id) {
								connection.write_packet(&Packet::PubComp { id }).await?;
							} else {
								awaiting_outgoing_pubcomp.insert(id);
							}
						}
						Some(Packet::PubComp { id }) => {
							let Some(sent) = awaiting_pubcomp.remove(&id) else {
								tracing::error!("unsolicited PubComp with id {id}");
								break
							};

							sent.send(()).unwrap();
						}
						Some(Packet::SubAck { id, .. }) => {
							//
							let Some((filters, response)) = awaiting_suback.remove(&id) else {
								tracing::error!("unsolicited SubAck with id {id}");
								break
							};

							let (publish_tx, publish_rx) = mpsc::channel(32);
							for filter in filters.iter() {
								subscriptions.insert(filter.clone(), publish_tx.clone());
							}

							response.send(Subscription::new(Arc::clone(&filters), publish_rx, command_tx.clone())).unwrap();
						}
						Some(Packet::PingResp) => {
							if let Some(sent) = pingreq_sent.take() {
								tracing::info!("PingResp received in {:?}", sent.elapsed());
							}
						}
						_ => {
							tracing::warn!("could not read packet");
							break
						}

					}
				}
				_ = keep_alive.tick() => {
					pingreq_sent.replace(Instant::now());
					connection.write_packet(&Packet::PingReq).await?;
				}
				else => {
					tracing::warn!("ending client task");
					return Ok(())
				}
			}
		}
	}
}

enum ConnAckResult {
	Continue { session_present: bool },
	Timeout,
}

async fn wait_for_connack(
	connection: &mut Connection,
	timeout: time::Duration,
) -> crate::Result<ConnAckResult> {
	let mut timeout = time::interval_at(Instant::now() + timeout, timeout);
	loop {
		tokio::select! {
			Ok(packet) = connection.read_packet() => {
				match packet {
					Some(Packet::ConnAck { session_present, code }) => {
						if code == 0 {
							break Ok(ConnAckResult::Continue { session_present })
						} else {
							break Err("connect error, rejected by peer".into())
						}
					}
					Some(_) => break Err("protocol error".into()),
					None => {
						break Ok(ConnAckResult::Timeout)//Err("connection reset by peer".into())
					}
				}
			}
			_ = timeout.tick() => {
				break Ok(ConnAckResult::Timeout)
			}
		}
	}
}
