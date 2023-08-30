use self::connection::Connection;
use mqtt_core::Packet;

pub use mqtt_core::{Error, QoS, Result};

use std::{collections::HashMap, time::Duration};
use tokio::{
	net::{TcpStream, ToSocketAddrs},
	sync::{
		mpsc::{self, UnboundedReceiver, UnboundedSender},
		oneshot,
	},
	task::JoinHandle,
	time,
};
mod connection;

#[derive(Debug)]
enum Command {
	Subscribe {
		filters: Vec<(String, QoS)>,
		tx: oneshot::Sender<Vec<Option<QoS>>>,
	},
}

pub struct Client {
	tx: UnboundedSender<Command>,
}

impl Client {
	pub async fn subscribe(&self, filters: Vec<(String, QoS)>) -> Vec<Option<QoS>> {
		let (tx, rx) = oneshot::channel();
		self.tx.send(Command::Subscribe { filters, tx }).unwrap();

		let result = rx.await.unwrap();
		result
	}
	//
}

pub fn client<A: ToSocketAddrs + Send + 'static>(
	addr: A,
) -> (Client, JoinHandle<mqtt_core::Result<()>>) {
	let (tx, rx) = mpsc::unbounded_channel();
	let handle = tokio::spawn(client_task(addr, rx));
	(Client { tx }, handle)
}

async fn client_task<A: ToSocketAddrs + Send>(
	addr: A,
	mut rx: UnboundedReceiver<Command>,
) -> mqtt_core::Result<()> {
	//

	let stream = TcpStream::connect(addr).await?;
	let mut connection = Connection::new(stream);
	let mut keep_alive = time::interval(Duration::from_secs(5));

	let mut inflight_subs: HashMap<u16, oneshot::Sender<Vec<Option<QoS>>>> = Default::default();
	let mut id = 0;

	connection
		.write_packet(&Packet::Connect {
			client_id: String::from("mqtt-async"),
			keep_alive: 5,
		})
		.await?;

	// connection
	// 	.write_packet(&Packet::Subscribe {
	// 		id: 0x01,
	// 		filters: vec![(String::from("#"), QoS::AtMostOnce)],
	// 	})
	// 	.await?;

	loop {
		tokio::select! {
			Some(command) = rx.recv() => {
				tracing::debug!(?command);
				match command {
					Command::Subscribe { filters, tx } => {
						id += 1;
						inflight_subs.insert(id, tx);
						connection.write_packet(&Packet::Subscribe { id, filters }).await?;
					}
				}
			}
			Ok(packet) = connection.read_packet() => {
				match packet {
					Some(Packet::SubAck { id, result }) => {
						//
						let Some(tx) = inflight_subs.remove(&id) else {
							tracing::error!("unsolicited SubAck with id {id}");
							break Err("unsolicited SubAck".into())
						};
						tx.send(result).unwrap();
					}
					_ => {}
				}
			}
			_ = keep_alive.tick() => {
				connection.write_packet(&Packet::PingReq).await?;
			}
			else => {
				tracing::warn!("ending client task");
				break Ok(())
			}
		}
	}
}
