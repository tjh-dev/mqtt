use bytes::Bytes;

use crate::QoS;

/// Client credentials
///
/// Note that is not possible to set a password without also setting a username.
#[derive(Debug)]
pub struct Credentials {
	pub username: String,
	pub password: Option<String>,
}

impl From<String> for Credentials {
	#[inline]
	fn from(username: String) -> Self {
		Self {
			username,
			password: None,
		}
	}
}

impl From<&str> for Credentials {
	#[inline]
	fn from(username: &str) -> Self {
		Self {
			username: String::from(username),
			password: None,
		}
	}
}

impl From<(String, String)> for Credentials {
	#[inline]
	fn from((username, password): (String, String)) -> Self {
		Self {
			username,
			password: Some(password),
		}
	}
}

impl From<(&str, &str)> for Credentials {
	#[inline]
	fn from((username, password): (&str, &str)) -> Self {
		Self {
			username: String::from(username),
			password: Some(String::from(password)),
		}
	}
}

/// Will Message
///
/// The will message is set by the Client when it connects to the Server. If the
/// Client disconnects abnormally, the Server publishes the will message to the
/// topic on behalf of the Client. The will message MUST be published with the
/// Will QoS and Retain flags as specified.
#[derive(Debug)]
pub struct Will {
	/// The topic to publish the will message to.
	pub topic: String,

	/// The message to publish as the will.
	pub payload: Bytes,

	/// The quality of service to publish the will message at.
	pub qos: QoS,

	/// Whether or not the will message should be retained.
	pub retain: bool,
}
