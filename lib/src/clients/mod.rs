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

/// Transport options.
#[derive(Debug)]
pub enum Transport {
	/// Raw TCP transport.
	Tcp(TcpConfiguration),
}

pub struct ClientOptions<'a> {
	transport: Transport,
	configuration: ClientConfiguration<'a>,
}

impl<'a> ClientOptions<'a> {
	/// Creates a new `ClientOptions` instance with specified transport and
	/// configuration.
	pub fn new(transport: Transport, configuration: ClientConfiguration<'a>) -> Self {
		Self {
			transport,
			configuration,
		}
	}

	#[cfg(feature = "url")]
	pub fn try_from_url(url: &url::Url) -> Result<Self, UnsupportedScheme> {
		let transport = url.try_into()?;
		let configuration = ClientConfiguration::from_url(url);
		Ok(Self {
			transport,
			configuration,
		})
	}
}

/// Client configuration which is independent from the transport protocol.
#[derive(Debug)]
pub struct ClientConfiguration<'a> {
	/// Keep alive timeout in seconds.
	///
	/// Defaults to 60 seconds.
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,

	/// Username for authentication.
	pub username: Option<String>,

	/// Password for authentication.
	///
	/// This will be ignored if no username is set.
	pub password: Option<String>,
	pub will: Option<Will<'a>>,

	/// Controls whether the client task should attempt to automatically
	/// reconnect.
	pub reconnect: bool,
}

impl<'a> Default for ClientConfiguration<'a> {
	fn default() -> Self {
		Self {
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
			username: Default::default(),
			password: Default::default(),
			will: Default::default(),
			reconnect: true,
		}
	}
}

impl<'a> ClientConfiguration<'a> {
	#[cfg(feature = "url")]
	pub fn from_url(url: &url::Url) -> Self {
		let mut config: ClientConfiguration<'a> = Default::default();

		if !url.username().is_empty() {
			config.username = Some(url.username().into());
		}
		config.password = url.password().map(|x| x.into());

		for (key, value) in url.query_pairs() {
			match key.as_ref() {
				"clean_session" => {
					let Ok(clean_session) = value.parse() else {
						continue;
					};
					config.clean_session = clean_session;
				}
				"client_id" => {
					let Ok(client_id) = value.parse() else {
						continue;
					};
					config.client_id = client_id;
				}
				"keep_alive" => {
					let Ok(keep_alive) = value.parse() else {
						continue;
					};
					config.keep_alive = keep_alive;
				}
				_ => {}
			}
		}

		config
	}

	pub fn into_options<T, E>(self, transport: T) -> Result<ClientOptions<'a>, E>
	where
		T: TryInto<Transport>,
		E: From<T::Error>,
	{
		let transport = transport.try_into()?;
		Ok(ClientOptions {
			transport,
			configuration: self,
		})
	}
}

#[cfg(feature = "url")]
impl<'a> TryFrom<url::Url> for ClientConfiguration<'a> {
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

/// Configuration for connecting to a Server over TCP.
#[derive(Debug)]
pub struct TcpConfiguration {
	/// Hostname of IP address of the MQTT Server.
	pub host: String,
	pub port: u16,
	pub linger: bool,
}

#[derive(thiserror::Error, Debug)]
#[error("the specified url scheme is not supported")]
pub struct UnsupportedScheme;

#[cfg(feature = "url")]
impl TryFrom<&url::Url> for Transport {
	type Error = UnsupportedScheme;
	fn try_from(value: &url::Url) -> Result<Self, Self::Error> {
		match value.scheme() {
			"mqtt" | "tcp" => {
				let transport = TcpConfiguration {
					host: value.host_str().unwrap_or(DEFAULT_MQTT_HOST).into(),
					port: value.port().unwrap_or(DEFAULT_MQTT_PORT),
					linger: true,
				};

				Ok(Self::Tcp(transport))
			}
			_ => Err(UnsupportedScheme),
		}
	}
}

#[cfg(feature = "url")]
impl TryFrom<&url::Url> for ClientOptions<'_> {
	type Error = UnsupportedScheme;
	#[inline]
	fn try_from(value: &url::Url) -> Result<Self, Self::Error> {
		Self::try_from_url(value)
	}
}

#[cfg(feature = "url")]
impl TryFrom<url::Url> for ClientOptions<'_> {
	type Error = UnsupportedScheme;
	#[inline]
	fn try_from(value: url::Url) -> Result<Self, Self::Error> {
		Self::try_from(&value)
	}
}

#[cfg(feature = "url")]
impl TryFrom<&str> for ClientOptions<'_> {
	type Error = UnsupportedScheme;
	#[inline]
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		let url: url::Url = value.try_into().map_err(|_| UnsupportedScheme)?;
		Self::try_from(url)
	}
}

#[cfg(feature = "tokio-client")]
pub fn create_client(
	options: ClientOptions,
) -> (
	tokio::client::Client,
	::tokio::task::JoinHandle<crate::Result<()>>,
) {
	let ClientOptions {
		transport,
		configuration,
	} = options;
	match transport {
		Transport::Tcp(transport) => tokio::tcp_client(transport, configuration),
	}
}
