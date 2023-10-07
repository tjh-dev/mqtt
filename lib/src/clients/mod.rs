mod conv;
mod state;

#[cfg(feature = "tokio-client")]
pub mod tokio;

#[cfg(feature = "tokio-client")]
pub(crate) mod command;

#[cfg(feature = "tokio-client")]
mod holdoff;

use crate::TopicBuf;
use bytes::Bytes;

pub use self::{
	conv::{Filters, FiltersWithQoS},
	state::{ClientState, StateError},
};

/// A published message received from the Server.
#[derive(Debug)]
pub struct Message {
	/// Message topic.
	pub topic: TopicBuf,
	/// Indicates whether the sender of the message set the retain flag.
	pub retain: bool,
	/// Message payload.
	pub payload: Bytes,
}

impl<T: Into<TopicBuf>> From<(T, bool, Bytes)> for Message {
	fn from((topic, retain, payload): (T, bool, Bytes)) -> Self {
		Self {
			topic: topic.into(),
			retain,
			payload,
		}
	}
}

/// Options for connecting to the Server.
pub struct TcpConnectOptions {
	pub host: String,
	pub port: u16,
	#[cfg(feature = "tls")]
	pub tls: bool,
	pub user: Option<String>,
	pub password: Option<String>,
	pub linger: bool,
}
