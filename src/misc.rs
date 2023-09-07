use bytes::Bytes;

/// Quality of Service
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum QoS {
	AtMostOnce = 0,
	AtLeastOnce,
	ExactlyOnce,
}

#[derive(Debug)]
pub struct InvalidQoS;

impl TryFrom<u8> for QoS {
	type Error = InvalidQoS;
	fn try_from(value: u8) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Self::AtMostOnce),
			1 => Ok(Self::AtLeastOnce),
			2 => Ok(Self::ExactlyOnce),
			_ => Err(InvalidQoS),
		}
	}
}

#[derive(Debug)]
pub struct Credentials {
	pub username: String,
	pub password: Option<String>,
}

impl From<String> for Credentials {
	fn from(username: String) -> Self {
		Self {
			username,
			password: None,
		}
	}
}

impl From<&str> for Credentials {
	fn from(username: &str) -> Self {
		Self {
			username: String::from(username),
			password: None,
		}
	}
}

impl From<(String, String)> for Credentials {
	fn from((username, password): (String, String)) -> Self {
		Self {
			username,
			password: Some(password),
		}
	}
}

impl From<(&str, &str)> for Credentials {
	fn from((username, password): (&str, &str)) -> Self {
		Self {
			username: String::from(username),
			password: Some(String::from(password)),
		}
	}
}

#[derive(Debug)]
pub struct Will {
	pub topic: String,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
}
