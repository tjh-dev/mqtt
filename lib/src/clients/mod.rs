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

pub const DEFAULT_MQTT_HOST: &str = "localhost";
pub const DEFAULT_MQTT_PORT: u16 = 1883;
pub const DEFAULT_MQTTS_PORT: u16 = 8883;

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

#[cfg(feature = "url")]
impl<'a> TryFrom<url::Url> for ClientConfiguration<'a, ()> {
	type Error = Box<dyn std::error::Error>;
	fn try_from(value: url::Url) -> Result<Self, Self::Error> {
		let username: Option<String> = match value.username() {
			"" => None,
			username => Some(username.into()),
		};

		let mut clean_session = true;
		let mut client_id = Default::default();
		let mut keep_alive = 60;

		for (key, value) in value.query_pairs() {
			match key.as_ref() {
				"clean_session" => {
					clean_session = value.parse()?;
				}
				"client_id" => {
					client_id = value.into_owned();
				}
				"keep_alive" => {
					keep_alive = value.parse()?;
				}
				_ => {}
			}
		}

		Ok(Self {
			username,
			password: value.password().map(|x| x.into()),
			clean_session,
			client_id,
			keep_alive,
			..Default::default()
		})
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
		let tls = match value.scheme() {
			"mqtt" | "tcp" => false,
			"mqtts" | "ssl" => true,
			_ => return Err(UnsupportedScheme),
		};

		let port = value.port().unwrap_or(match tls {
			true => DEFAULT_MQTTS_PORT,
			false => DEFAULT_MQTT_PORT,
		});

		Ok(Self {
			host: value.host_str().unwrap_or(DEFAULT_MQTT_HOST).into(),
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
		"mqtt" | "tcp" => {
			let options = ClientConfiguration::default_with(url.try_into().unwrap());
			tokio::tcp_client(options)
		}
		#[cfg(feature = "tls")]
		"mqtts" | "ssl" => {
			let options = ClientConfiguration::default_with(url.try_into().unwrap());
			tokio::tcp_client(options)
		}
		_ => unimplemented!(),
	}
}
