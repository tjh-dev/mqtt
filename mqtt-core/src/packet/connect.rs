use super::{get_slice, get_str, get_u16, get_u8};
use crate::{QoS, WriteError};
use bytes::{BufMut, Bytes};
use std::{borrow::Cow, io};

const PROTOCOL_NAME: &str = "MQTT";

#[derive(Debug)]
pub struct Connect {
	pub protocol_name: Cow<'static, str>,
	pub protocol_level: u8,
	pub client_id: String,
	pub keep_alive: u16,
	pub clean_session: bool,
	pub will: Option<Will>,
	pub credentials: Option<Credentials>,
}

#[derive(Debug)]
pub struct Credentials {
	pub username: String,
	pub password: Option<String>,
}

#[derive(Debug)]
pub struct Will {
	pub topic: String,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
}

impl Default for Connect {
	fn default() -> Self {
		Self {
			protocol_name: Cow::Borrowed(PROTOCOL_NAME),
			protocol_level: 4,
			client_id: String::from(""),
			keep_alive: 0,
			clean_session: true,
			will: None,
			credentials: None,
		}
	}
}

impl Connect {
	pub fn parse(payload: &mut io::Cursor<&[u8]>) -> Result<Self, super::Error> {
		let protocol_name = match get_str(payload)? {
			PROTOCOL_NAME => Cow::Borrowed(PROTOCOL_NAME),
			_ => {
				return Err(super::Error::MalformedPacket("invalid protocol name"));
			}
		};

		let protocol_level = get_u8(payload)?;
		let flags = get_u8(payload)?;
		let keep_alive = get_u16(payload)?;
		let client_id = get_str(payload)?;

		let clean_session = flags & 0x02 == 0x02;
		let will = if flags & 0x04 == 0x04 {
			let topic = get_str(payload)?;
			let len = get_u16(payload)?;
			let payload = get_slice(payload, len as usize)?.to_vec();
			let qos = ((flags & 0x18) >> 3).try_into()?;
			let retain = flags & 0x20 == 0x20;
			Some(Will {
				topic: String::from(topic),
				payload: Bytes::from(payload),
				qos,
				retain,
			})
		} else {
			None
		};

		let credentials = if flags & 0x40 == 0x40 {
			let username = get_str(payload)?;
			let password = if flags & 0x80 == 0x80 {
				Some(get_str(payload)?.to_string())
			} else {
				None
			};
			Some(Credentials {
				username: String::from(username),
				password,
			})
		} else {
			None
		};

		Ok(Self {
			protocol_name,
			protocol_level,
			client_id: String::from(client_id),
			keep_alive,
			clean_session,
			will,
			credentials,
		})
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		// Write the packet type and length.
		super::put_u8(dst, 0x10)?;
		super::put_var(dst, self.payload_len())?;

		// Write the protocol name and level.
		super::put_str(dst, &self.protocol_name)?;
		super::put_u8(dst, self.protocol_level)?;

		// Write the flags and keep alive.
		super::put_u8(dst, self.flags())?;
		super::put_u16(dst, self.keep_alive)?;

		// Write the client ID.
		super::put_str(dst, &self.client_id)?;

		// Write the will.
		if let Some(will) = &self.will {
			super::put_str(dst, &will.topic)?;
			super::put_slice(dst, &will.payload)?;
		}

		// Write the credentials.
		if let Some(credentials) = &self.credentials {
			super::put_str(dst, &credentials.username)?;
			if let Some(password) = &credentials.password {
				super::put_str(dst, password)?;
			}
		}

		Ok(())
	}

	#[inline(always)]
	fn payload_len(&self) -> usize {
		let mut len = 2 + self.protocol_name.len()
      + 4 // protocol level, flags, an keep alive
      + (2 + self.client_id.len());

		if let Some(will) = &self.will {
			len += 2 + will.topic.len() + 2 + will.payload.len();
		}

		if let Some(credentials) = &self.credentials {
			len += 2 + credentials.username.len();
			if let Some(password) = &credentials.password {
				len += 2 + password.len();
			}
		}

		len
	}

	#[inline(always)]
	fn flags(&self) -> u8 {
		let mut flags = 0;

		if self.clean_session {
			flags |= 0x02;
		}

		if let Some(will) = &self.will {
			flags |= 0x04;
			flags |= (will.qos as u8) << 3;
			if will.retain {
				flags |= 0x20;
			}
		}

		if let Some(credentials) = &self.credentials {
			flags |= 0x80;
			if credentials.password.is_some() {
				flags |= 0x40;
			}
		}

		flags
	}
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
