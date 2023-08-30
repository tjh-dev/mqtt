mod client;
mod command;
mod connection;

use crate::command::Command;

use self::connection::Connection;
pub use mqtt_core::{Error, Packet, QoS, Result};
use std::{collections::HashMap, time::Duration};
use tokio::{
	io::AsyncWriteExt,
	net::{TcpStream, ToSocketAddrs},
	sync::{
		mpsc::{self, UnboundedReceiver},
		oneshot,
	},
	task::JoinHandle,
	time,
};

pub fn client<A: ToSocketAddrs + Send + 'static>(
	addr: A,
) -> (client::Client, JoinHandle<mqtt_core::Result<()>>) {
	let (tx, rx) = mpsc::unbounded_channel();
	let handle = tokio::spawn(client_task(addr, rx));

	(client::Client::new(tx), handle)
}

async fn client_task<A: ToSocketAddrs + Send>(
	addr: A,
	mut rx: UnboundedReceiver<command::Command>,
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
					Command::Shutdown => {
						connection.write_packet(&Packet::Disconnect).await?;
						drop(inflight_subs);
						break Ok(())
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
