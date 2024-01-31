use super::{client_configuration::ClientConfiguration, transport::Transport};

#[cfg(feature = "url")]
use super::transport::UnsupportedScheme;

pub struct ClientOptions<'a> {
	pub transport: Transport,
	pub configuration: ClientConfiguration<'a>,
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
