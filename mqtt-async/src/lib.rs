mod client;
mod command;
mod connection;

use crate::command::Command;

use self::connection::Connection;
use mqtt_core::Connect;
pub use mqtt_core::{Error, Packet, QoS, Result};
use std::{
	collections::HashMap,
	process,
	time::{Duration, Instant},
};
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
		.write_packet(&Packet::Connect(Connect {
			client_id: format!(
				"{}/{}:{}",
				env!("CARGO_PKG_NAME"),
				env!("CARGO_PKG_VERSION"),
				process::id()
			),
			keep_alive: 30,
			credentials: Some(("tjh", "sausages").into()),
			..Default::default()
		}))
		.await?;

	let mut pingreq_sent: Option<Instant> = None;

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
