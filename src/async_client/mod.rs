mod client;
mod command;
mod packetstream;
mod state;
mod task;

use crate::misc::{Credentials, Will};
pub use client::{Client, Message, MessageGuard, Subscription};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug)]
pub struct Options {
	pub host: String,
	pub port: u16,
	pub tls: bool,
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,
	pub credentials: Option<Credentials>,
	pub will: Option<Will>,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			host: Default::default(),
			port: 1883,
			tls: false,
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
			credentials: Default::default(),
			will: Default::default(),
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
pub fn client(options: impl Into<Options>) -> (client::Client, JoinHandle<crate::Result<()>>) {
	let (tx, rx) = mpsc::unbounded_channel();
	let handle = tokio::spawn(task::client_task(options.into(), rx));

	(client::Client::new(tx), handle)
}
