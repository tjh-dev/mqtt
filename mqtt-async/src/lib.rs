mod client;
mod command;
mod connection;
mod task;

pub use mqtt_core::{Error, Filter, FilterBuf, FilterError, Packet, QoS, Result};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug)]
pub struct Options {
	pub host: String,
	pub port: u16,
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			host: Default::default(),
			port: 1883,
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
		}
	}
}

impl<H: AsRef<str>> From<(H, u16)> for Options {
	fn from(value: (H, u16)) -> Self {
		let (host, port) = value;
		Self {
			host: host.as_ref().into(),
			port,
			..Default::default()
		}
	}
}

/// Construct a new asynchronous MQTT client.
///
pub fn client(options: impl Into<Options>) -> (client::Client, JoinHandle<mqtt_core::Result<()>>) {
	let (tx, rx) = mpsc::unbounded_channel();
	let handle = tokio::spawn(task::client_task(options.into(), tx.clone(), rx));

	(client::Client::new(tx), handle)
}
