mod conv;
mod state;

#[cfg(feature = "tokio-client")]
pub mod tokio;

#[cfg(feature = "tokio-client")]
pub(crate) mod command;

#[cfg(feature = "tokio-client")]
mod holdoff;

use crate::{misc::Will, TopicBuf};
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
	#[inline]
	fn from((topic, retain, payload): (T, bool, Bytes)) -> Self {
		Self {
			topic: topic.into(),
			retain,
			payload,
		}
	}
}

#[derive(Debug)]
pub struct ClientConfiguration<'a, T = ()> {
	pub transport: T,
	//
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,

	pub username: Option<String>,
	pub password: Option<String>,
	pub will: Option<Will<'a>>,
}

impl<'a> Default for ClientConfiguration<'a, ()> {
	#[inline]
	fn default() -> Self {
		Self::default_with(())
	}
}

impl<'a> ClientConfiguration<'a, ()> {
	pub fn with_transport<T>(self, transport: T) -> ClientConfiguration<'a, T> {
		ClientConfiguration {
			transport,
			keep_alive: self.keep_alive,
			clean_session: self.clean_session,
			client_id: self.client_id,
			username: self.username,
			password: self.password,
			will: self.will,
		}
	}
}

impl<'a, T> ClientConfiguration<'a, T> {
	pub fn default_with(transport: T) -> Self {
		Self {
			transport,
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
			username: Default::default(),
			password: Default::default(),
			will: Default::default(),
		}
	}
}

/// Configuration for connecting to a Server over TCP.
pub struct TcpConfiguration {
	/// Hostname of IP address of the MQTT Server.
	pub host: String,
	pub port: u16,
	#[cfg(feature = "tls")]
	pub tls: bool,
	pub linger: bool,
}

#[derive(thiserror::Error, Debug)]
#[error("the specified url scheme is not supported")]
pub struct UnsupportedScheme;

#[cfg(feature = "url")]
impl TryFrom<url::Url> for TcpConfiguration {
	type Error = UnsupportedScheme;
	fn try_from(value: url::Url) -> Result<Self, Self::Error> {
		let host = value.host_str().unwrap_or("localhost").into();
		let tls = match value.scheme() {
			"mqtt" | "tcp" => false,
			"mqtts" | "ssl" => true,
			_ => return Err(UnsupportedScheme),
		};

		let port = value.port().unwrap_or(if tls { 8883 } else { 1883 });

		Ok(Self {
			host,
			port,
			tls,
			linger: true,
		})
	}
}

#[cfg(all(feature = "url", feature = "tokio-client"))]
pub fn client(
	url: url::Url,
) -> (
	tokio::client::Client,
	::tokio::task::JoinHandle<crate::Result<()>>,
) {
	match url.scheme() {
		"tcp" | "mqtt" => {
			let options = ClientConfiguration::default_with(url.try_into().unwrap());
			tokio::tcp_client(options)
		}
		#[cfg(feature = "tls")]
		"ssl" | "mqtts" => {
			let options = ClientConfiguration::default_with(url.try_into().unwrap());
			tokio::tcp_client(options)
		}
		_ => unimplemented!(),
	}
}
