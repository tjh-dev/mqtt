use super::{client_options::ClientOptions, transport::Transport};
use mqtt_protocol::misc::{Credentials, Will};

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

	pub fn credentials(&self) -> Option<Credentials> {
		self.username.as_ref().map(|username| Credentials {
			username,
			password: self.password.as_deref(),
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
