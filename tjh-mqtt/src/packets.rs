use crate::{filter, misc, serde, FilterBuf, InvalidQoS, Packet, PacketId, QoS, Topic, TopicBuf};
use bytes::{Buf, BufMut, Bytes};
use std::{borrow::Cow, error, fmt, io, str::Utf8Error};

const DEFAULT_PROTOCOL_NAME: &str = "MQTT";

#[derive(Debug)]
pub struct SubscribeFailed;

//
// Packet Types
//

#[derive(Debug)]
pub struct Connect {
	pub protocol_name: Cow<'static, str>,
	pub protocol_level: u8,
	pub client_id: String,
	pub keep_alive: u16,
	pub clean_session: bool,
	pub will: Option<misc::Will>,
	pub credentials: Option<misc::Credentials>,
}

#[derive(Debug)]
pub struct ConnAck {
	pub session_present: bool,
	pub code: u8,
}

pub enum Publish {
	AtMostOnce {
		retain: bool,
		topic: TopicBuf,
		payload: Bytes,
	},
	AtLeastOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: TopicBuf,
		payload: Bytes,
	},
	ExactlyOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: TopicBuf,
		payload: Bytes,
	},
}

id_packet!(PubAck, Packet::PubAck, 0x40);
id_packet!(PubRec, Packet::PubRec, 0x50);
id_packet!(PubRel, Packet::PubRel, 0x62);
id_packet!(PubComp, Packet::PubComp, 0x70);

#[derive(Debug)]
pub struct Subscribe {
	pub id: PacketId,
	pub filters: Vec<(FilterBuf, QoS)>,
}

#[derive(Debug)]
pub struct SubAck {
	pub id: PacketId,
	pub result: Vec<Result<QoS, SubscribeFailed>>,
}

#[derive(Debug)]
pub struct Unsubscribe {
	pub id: PacketId,
	pub filters: Vec<FilterBuf>,
}

id_packet!(UnsubAck, Packet::UnsubAck, 0xb0);
nul_packet!(PingReq, crate::packet::Packet::PingReq, 0xc0);
nul_packet!(PingResp, crate::packet::Packet::PingResp, 0xd0);
nul_packet!(Disconnect, crate::packet::Packet::Disconnect, 0xe0);

mod connect {
	use super::*;

	impl Default for Connect {
		fn default() -> Self {
			Self {
				protocol_name: Cow::Borrowed(DEFAULT_PROTOCOL_NAME),
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
		pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
			let mut cursor = io::Cursor::new(payload);
			let protocol_name = match serde::get_str(&mut cursor)? {
				DEFAULT_PROTOCOL_NAME => Cow::Borrowed(DEFAULT_PROTOCOL_NAME),
				_ => {
					return Err(ParseError::MalformedPacket("invalid protocol name"));
				}
			};

			let protocol_level = serde::get_u8(&mut cursor)?;
			let flags = serde::get_u8(&mut cursor)?;
			let keep_alive = serde::get_u16(&mut cursor)?;
			let client_id = serde::get_str(&mut cursor)?;

			let clean_session = flags & 0x02 == 0x02;
			let will = if flags & 0x04 == 0x04 {
				let topic = serde::get_str(&mut cursor)?;
				let len = serde::get_u16(&mut cursor)?;

				// TODO: Can this be borrowed?
				let payload = serde::get_slice(&mut cursor, len as usize)?.to_vec();
				let qos = ((flags & 0x18) >> 3).try_into()?;
				let retain = flags & 0x20 == 0x20;

				Some(misc::Will {
					topic: TopicBuf::new(topic)?,
					payload: Bytes::from(payload),
					qos,
					retain,
				})
			} else {
				None
			};

			let credentials = if flags & 0x40 == 0x40 {
				let username = serde::get_str(&mut cursor)?;
				let password = if flags & 0x80 == 0x80 {
					Some(serde::get_str(&mut cursor)?.to_string())
				} else {
					None
				};
				Some(misc::Credentials {
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

		pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
			// Write the packet type and length.
			serde::put_u8(dst, 0x10)?;
			serde::put_var(dst, self.payload_len())?;

			// Write the protocol name and level.
			serde::put_str(dst, &self.protocol_name)?;
			serde::put_u8(dst, self.protocol_level)?;

			// Write the flags and keep alive.
			serde::put_u8(dst, self.flags())?;
			serde::put_u16(dst, self.keep_alive)?;

			// Write the client ID.
			serde::put_str(dst, &self.client_id)?;

			// Write the will.
			if let Some(will) = &self.will {
				serde::put_str(dst, will.topic.as_str())?;
				serde::put_slice(dst, &will.payload)?;
			}

			// Write the credentials.
			if let Some(credentials) = &self.credentials {
				serde::put_str(dst, &credentials.username)?;
				if let Some(password) = &credentials.password {
					serde::put_str(dst, password)?;
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
}

impl ConnAck {
	/// Parses the payload of a ConnAck packet.
	pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
		if payload.len() != 2 {
			return Err(ParseError::MalformedPacket(
				"ConnAck packet must have length 2",
			));
		}

		let mut cursor = io::Cursor::new(payload);
		let flags = serde::get_u8(&mut cursor)?;
		let code = serde::get_u8(&mut cursor)?;

		if flags & 0xe0 != 0 {
			return Err(ParseError::MalformedPacket(
				"upper 7 bits in ConnAck flags must be zero",
			));
		}

		let session_present = flags & 0x01 == 0x01;

		Ok(Self {
			session_present,
			code,
		})
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		let Self {
			session_present,
			code,
		} = self;
		serde::put_u8(dst, 0x20)?;
		serde::put_var(dst, 2)?;
		serde::put_u8(dst, if *session_present { 0x01 } else { 0x00 })?;
		serde::put_u8(dst, *code)?;
		Ok(())
	}
}

const PUBLISH_HEADER_CONTROL: u8 = 0x30;
const PUBLISH_HEADER_RETAIN_FLAG: u8 = 0x01;
const PUBLISH_HEADER_DUPLICATE_FLAG: u8 = 0x08;
const PUBLISH_HEADER_QOS_MASK: u8 = 0x06;

impl Publish {
	pub fn parse(payload: &[u8], flags: u8) -> Result<Self, ParseError> {
		let mut cursor = io::Cursor::new(payload);
		// Extract properties from the header flags.
		let retain = flags & PUBLISH_HEADER_RETAIN_FLAG == PUBLISH_HEADER_RETAIN_FLAG;
		let duplicate = flags & PUBLISH_HEADER_DUPLICATE_FLAG == PUBLISH_HEADER_DUPLICATE_FLAG;
		let qos: QoS = ((flags & PUBLISH_HEADER_QOS_MASK) >> 1).try_into()?;

		let topic = TopicBuf::new(serde::get_str(&mut cursor)?)?;

		// The interpretation of the remaining bytes depends on the QoS.
		match qos {
			QoS::AtMostOnce => {
				if duplicate {
					return Err(ParseError::MalformedPacket(
						"duplicate flag must be 0 for Publish packets with QoS of AtMostOnce",
					));
				}
				let remaining = cursor.remaining();
				let payload = serde::get_slice(&mut cursor, remaining)?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::AtMostOnce {
					retain,
					topic,
					payload,
				})
			}
			QoS::AtLeastOnce => {
				let id = serde::get_id(&mut cursor)?;
				let remaining = cursor.remaining();
				let payload = serde::get_slice(&mut cursor, remaining)?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::AtLeastOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
			QoS::ExactlyOnce => {
				let id = serde::get_id(&mut cursor)?;
				let remaining = cursor.remaining();
				let payload = serde::get_slice(&mut cursor, remaining)?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::ExactlyOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
		}
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		match self {
			Self::AtMostOnce {
				retain,
				topic,
				payload,
			} => {
				let flags = retain.then_some(PUBLISH_HEADER_RETAIN_FLAG).unwrap_or(0)
					| (QoS::AtMostOnce as u8) << 1;
				serde::put_u8(dst, PUBLISH_HEADER_CONTROL | flags)?;
				serde::put_var(dst, 2 + topic.len() + payload.len())?;
				serde::put_str(dst, topic.as_str())?;
				serde::put_slice(dst, payload)?;
			}
			Self::AtLeastOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				let flags = retain.then_some(PUBLISH_HEADER_RETAIN_FLAG).unwrap_or(0)
					| duplicate
						.then_some(PUBLISH_HEADER_DUPLICATE_FLAG)
						.unwrap_or(0) | (QoS::AtLeastOnce as u8) << 1;
				serde::put_u8(dst, PUBLISH_HEADER_CONTROL | flags)?;
				serde::put_var(dst, 4 + topic.len() + payload.len())?;
				serde::put_str(dst, topic.as_str())?;
				serde::put_u16(dst, id.get())?;
				serde::put_slice(dst, payload)?;
			}
			Self::ExactlyOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				let flags = retain.then_some(PUBLISH_HEADER_RETAIN_FLAG).unwrap_or(0)
					| duplicate
						.then_some(PUBLISH_HEADER_DUPLICATE_FLAG)
						.unwrap_or(0) | (QoS::ExactlyOnce as u8) << 1;
				serde::put_u8(dst, PUBLISH_HEADER_CONTROL | flags)?;
				serde::put_var(dst, 4 + topic.len() + payload.len())?;
				serde::put_str(dst, topic.as_str())?;
				serde::put_u16(dst, id.get())?;
				serde::put_slice(dst, payload)?;
			}
		}

		Ok(())
	}

	/// Returns the topic of the Publish packet.
	#[inline]
	pub fn topic(&self) -> &Topic {
		match self {
			Self::AtMostOnce { topic, .. } => topic,
			Self::AtLeastOnce { topic, .. } => topic,
			Self::ExactlyOnce { topic, .. } => topic,
		}
	}

	/// Returns the payload of the Publish packet.
	#[inline]
	pub fn payload(&self) -> &Bytes {
		match self {
			Self::AtMostOnce { payload, .. } => payload,
			Self::AtLeastOnce { payload, .. } => payload,
			Self::ExactlyOnce { payload, .. } => payload,
		}
	}

	/// Returns the QoS of the Publish packet.
	#[inline]
	pub fn qos(&self) -> QoS {
		match self {
			Self::AtMostOnce { .. } => QoS::AtMostOnce,
			Self::AtLeastOnce { .. } => QoS::AtLeastOnce,
			Self::ExactlyOnce { .. } => QoS::ExactlyOnce,
		}
	}

	/// Returns the retain flag of the Publish packet.
	#[inline]
	pub fn retain(&self) -> bool {
		match self {
			Self::AtMostOnce { retain, .. } => *retain,
			Self::AtLeastOnce { retain, .. } => *retain,
			Self::ExactlyOnce { retain, .. } => *retain,
		}
	}

	/// Returns the Packet ID of the Publish packet.
	///
	/// This will always return `None` for Publish packets with [`QoS`] of
	/// [`AtMostOnce`].
	///
	/// [`AtMostOnce`]: QoS#variant.AtMostOnce
	#[inline]
	pub fn id(&self) -> Option<PacketId> {
		match self {
			Self::AtMostOnce { .. } => None,
			Self::AtLeastOnce { id, .. } => Some(*id),
			Self::ExactlyOnce { id, .. } => Some(*id),
		}
	}

	/// Returns the duplicate flag of the Publish packet.
	///
	/// This will always return `false` for Publish packets with [`QoS`] of
	/// [`AtMostOnce`].
	///
	/// [`AtMostOnce`]: QoS#variant.AtMostOnce
	#[inline]
	pub fn duplicate(&self) -> bool {
		match self {
			Self::AtMostOnce { .. } => false,
			Self::AtLeastOnce { duplicate, .. } => *duplicate,
			Self::ExactlyOnce { duplicate, .. } => *duplicate,
		}
	}
}

impl fmt::Debug for Publish {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Publish")
			.field("id", &self.id())
			.field("qos", &self.qos())
			.field("retain", &self.retain())
			.field("duplicate", &self.duplicate())
			.field("topic", &self.topic())
			.field("payload length", &self.payload().len())
			.finish()
	}
}

impl Subscribe {
	/// Parses the payload of a [`Subscribe`] packet.
	pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
		let mut cursor = io::Cursor::new(payload);
		let id = serde::get_id(&mut cursor)?;

		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = serde::get_str(&mut cursor)?;
			let qos: QoS = serde::get_u8(&mut cursor)?.try_into()?;
			filters.push((FilterBuf::new(filter)?, qos));
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		let Self { id, filters } = self;
		serde::put_u8(dst, 0x82)?;

		let len = 2 + filters
			.iter()
			.fold(0usize, |acc, (filter, _)| acc + 3 + filter.len());

		serde::put_var(dst, len)?;
		serde::put_u16(dst, id.get())?;
		for (filter, qos) in filters {
			serde::put_str(dst, filter.as_str())?;
			serde::put_u8(dst, *qos as u8)?;
		}

		Ok(())
	}
}

impl SubAck {
	pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
		let mut cursor = io::Cursor::new(payload);
		let id = serde::get_id(&mut cursor)?;

		let mut result = Vec::new();
		while cursor.has_remaining() {
			let return_code = serde::get_u8(&mut cursor)?;
			let qos: Result<QoS, SubscribeFailed> = match return_code.try_into() {
				Ok(qos) => Ok(qos),
				Err(_) => {
					if return_code == 0x80 {
						Err(SubscribeFailed)
					} else {
						return Err(ParseError::MalformedPacket("invalid return code in SubAck"));
					}
				}
			};

			result.push(qos);
		}

		Ok(Self { id, result })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		let Self { id, result } = self;
		serde::put_u8(dst, 0x90)?;

		let len = 2 + result.len();

		serde::put_var(dst, len)?;
		serde::put_u16(dst, id.get())?;
		for qos in result {
			serde::put_u8(dst, qos.as_ref().map(|qos| *qos as u8).unwrap_or(0x80))?;
		}

		Ok(())
	}
}

impl Unsubscribe {
	/// Parses the payload of a [`Subscribe`] packet.
	pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
		let mut cursor = io::Cursor::new(payload);
		let id = serde::get_id(&mut cursor)?;

		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = serde::get_str(&mut cursor)?;
			filters.push(FilterBuf::new(filter)?);
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		let Self { id, filters } = self;
		serde::put_u8(dst, 0xa2)?;

		let len = 2 + filters
			.iter()
			.fold(0usize, |acc, filter| acc + 2 + filter.len());

		serde::put_var(dst, len)?;
		serde::put_u16(dst, id.get())?;
		for filter in filters {
			serde::put_str(dst, filter.as_str())?;
		}

		Ok(())
	}
}

#[derive(Debug)]
pub enum ParseError {
	Incomplete,
	InvalidQoS,
	InvalidFilter(filter::FilterError),
	InvalidTopic(crate::InvalidTopic),
	InvalidHeader,
	ZeroPacketId,
	MalformedLength,
	MalformedPacket(&'static str),
	Utf8Error(Utf8Error),
}

impl From<Utf8Error> for ParseError {
	#[inline]
	fn from(value: Utf8Error) -> Self {
		Self::Utf8Error(value)
	}
}

impl From<InvalidQoS> for ParseError {
	#[inline]
	fn from(_: InvalidQoS) -> Self {
		Self::InvalidQoS
	}
}

impl From<crate::InvalidTopic> for ParseError {
	fn from(value: crate::InvalidTopic) -> Self {
		Self::InvalidTopic(value)
	}
}

impl From<filter::FilterError> for ParseError {
	#[inline]
	fn from(value: filter::FilterError) -> Self {
		Self::InvalidFilter(value)
	}
}

impl fmt::Display for ParseError {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl error::Error for ParseError {}

macro_rules! id_packet {
	($name:tt,$variant:expr,$header:literal) => {
		#[derive(Debug)]
		pub struct $name {
			pub id: PacketId,
		}

		impl $name {
			pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
				if payload.len() != 2 {
					return Err(ParseError::MalformedPacket("packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = crate::serde::get_id(&mut buf)?;
				Ok(Self { id })
			}

			pub fn serialize_to_bytes(
				&self,
				dst: &mut impl BufMut,
			) -> Result<(), crate::serde::WriteError> {
				let Self { id } = self;
				crate::serde::put_u8(dst, $header)?;
				crate::serde::put_var(dst, 2)?;
				crate::serde::put_u16(dst, id.get())?;
				Ok(())
			}
		}

		impl From<$name> for Packet {
			#[inline]
			fn from(value: $name) -> Packet {
				$variant(value)
			}
		}
	};
}
use id_packet;

macro_rules! nul_packet {
	($name:tt,$variant:expr,$header:literal) => {
		#[derive(Debug)]
		pub struct $name;

		impl $name {
			pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
				if payload.len() != 0 {
					return Err(ParseError::MalformedPacket("packet must have length 0"));
				}
				Ok(Self)
			}

			pub fn serialize_to_bytes(
				&self,
				dst: &mut impl BufMut,
			) -> Result<(), crate::serde::WriteError> {
				crate::serde::put_u8(dst, $header)?;
				crate::serde::put_var(dst, 0)?;
				Ok(())
			}
		}

		impl From<$name> for crate::packet::Packet {
			#[inline]
			fn from(_: $name) -> crate::packet::Packet {
				$variant
			}
		}
	};
}
use nul_packet;
