use std::path;

pub const DEFAULT_MQTT_HOST: &str = "localhost";
pub const DEFAULT_MQTT_PORT: u16 = 1883;
pub const DEFAULT_MQTTS_PORT: u16 = 8883;

/// Transport options.
#[derive(Debug)]
pub enum Transport {
	/// Raw TCP transport.
	Tcp(TcpConfiguration),
	#[cfg(feature = "tls")]
	Tls(TcpConfiguration),
	Socket(path::PathBuf),
}

/// Configuration for connecting to a Server over TCP.
#[derive(Debug)]
pub struct TcpConfiguration {
	/// Hostname or IP address of the MQTT Server.
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
			#[cfg(feature = "tls")]
			"mqtts" | "ssl" | "tls" => {
				let transport = TcpConfiguration {
					host: value.host_str().unwrap_or(DEFAULT_MQTT_HOST).into(),
					port: value.port().unwrap_or(DEFAULT_MQTTS_PORT),
					linger: true,
				};

				Ok(Self::Tls(transport))
			}
			"sock" | "socket" => {
				let path = value.path();
				Ok(Self::Socket(path.into()))
			}
			_ => Err(UnsupportedScheme),
		}
	}
}
