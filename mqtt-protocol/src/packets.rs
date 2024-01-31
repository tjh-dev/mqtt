use crate::{
	bytes_reader::{BytesReader, Cursor},
	frame::Frame,
	misc::{self, Credentials, Will},
	serde, Filter, InvalidQoS, Packet, PacketId, QoS, Topic,
};
use bytes::{BufMut, Bytes};
use std::{error, fmt, str::Utf8Error};

/// The only valid value for [`protocol_name`] in [`Connect`] packets.
///
/// [`protocol_name`]: Connect::protocol_name
pub const PROTOCOL_NAME: &str = "MQTT";

pub trait SerializePacket {
	/// Serializes the packet into the provided [`BufMut`] implementation.
	fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError>;
}

pub trait DeserializePacket<'a>: Sized {
	fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError>;
}

#[derive(Debug)]
pub struct SubscribeFailed;

//
// Packet Types
//

/// A `Connect` packet is sent by the Client to the Server to initialise a
/// session.
#[derive(Clone, Debug)]
pub struct Connect<'a> {
	/// Protocol name. Should always be `"MQTT"`.
	pub protocol_name: &'a str,

	/// Protocol version.
	pub protocol_level: u8,

	/// Client ID.
	///
	/// The Server _may_ accept an empty client ID.
	pub client_id: &'a str,

	/// Keep-alive timeout in seconds.
	pub keep_alive: u16,

	/// Request a clean session.
	pub clean_session: bool,

	/// Last will and testament for the Client.
	pub will: Option<Will<'a>>,

	/// Login credentials.
	pub credentials: Option<Credentials<'a>>,
}

/// A ConnAck packet is sent by the Server to the Client to acknowledge a
/// new session.
///
/// The Client may send packets to the Server before receiving ConnAck, however
/// the Server shouldn't send any packets to the Client before ConnAck.
#[derive(Debug)]
pub struct ConnAck {
	/// Indicates that the Server has existing state from a previous session for
	/// the client.
	pub session_present: bool,

	/// Status code.
	pub code: u8,
}

/// A Publish packet can be sent by either the Client or the Server.
pub enum Publish<'a> {
	AtMostOnce {
		retain: bool,
		topic: &'a Topic,
		payload: Bytes,
	},
	AtLeastOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: &'a Topic,
		payload: Bytes,
	},
	ExactlyOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: &'a Topic,
		payload: Bytes,
	},
}

id_packet!(PubAck, Packet::PubAck, 0x40, "PubAck");
id_packet!(PubRec, Packet::PubRec, 0x50, "PubRec");
id_packet!(PubRel, Packet::PubRel, 0x62, "PubRel");
id_packet!(PubComp, Packet::PubComp, 0x70, "PubComp");

#[derive(Debug)]
pub struct Subscribe<'a> {
	pub id: PacketId,
	pub filters: Vec<(&'a Filter, QoS)>,
}

#[derive(Debug)]
pub struct SubAck {
	pub id: PacketId,
	pub result: Vec<Result<QoS, SubscribeFailed>>,
}

#[derive(Debug)]
pub struct Unsubscribe<'a> {
	pub id: PacketId,
	pub filters: Vec<&'a Filter>,
}

id_packet!(UnsubAck, Packet::UnsubAck, 0xb0, "UnsubAck");
nul_packet!(PingReq, crate::packet::Packet::PingReq, 0xc0);
nul_packet!(PingResp, crate::packet::Packet::PingResp, 0xd0);
nul_packet!(Disconnect, crate::packet::Packet::Disconnect, 0xe0);

mod connect {
	use super::*;

	impl<'a> Default for Connect<'a> {
		fn default() -> Self {
			Self {
				protocol_name: PROTOCOL_NAME,
				protocol_level: 4,
				client_id: "",
				keep_alive: 0,
				clean_session: true,
				will: None,
				credentials: None,
			}
		}
	}

	impl<'a> Connect<'a> {
		pub fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError> {
			let mut cursor = Cursor::from_frame(frame);
			let protocol_name = match cursor.take_str()? {
				PROTOCOL_NAME => PROTOCOL_NAME,
				_ => {
					return Err(DeserializeError::MalformedPacket("invalid protocol name"));
				}
			};

			let protocol_level = cursor.take_u8()?;
			let flags = cursor.take_u8()?;
			let keep_alive = cursor.take_u16()?;
			let client_id = cursor.take_str()?;

			let clean_session = flags & 0x02 == 0x02;
			let will = if flags & 0x04 == 0x04 {
				// Deserialize the last-will topic.
				let topic = Topic::new(cursor.take_str()?)?;

				// Read the last-will payload length and slice.
				let len = cursor.take_u16()?;
				let payload = cursor.take_slice(len.into())?;

				let qos = ((flags & 0x18) >> 3).try_into()?;
				let retain = flags & 0x20 == 0x20;

				Some(Will::new(topic, payload, qos, retain))
			} else {
				None
			};

			let credentials = if flags & 0x40 == 0x40 {
				let username = cursor.take_str()?;
				let password = if flags & 0x80 == 0x80 {
					Some(cursor.take_str()?)
				} else {
					None
				};
				Some(misc::Credentials { username, password })
			} else {
				None
			};

			Ok(Self {
				protocol_name,
				protocol_level,
				client_id,
				keep_alive,
				clean_session,
				will,
				credentials,
			})
		}

		pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
			// Write the packet type and length.
			serde::put_u8(dst, 0x10)?;
			serde::put_var(dst, self.payload_len())?;

			// Write the protocol name and level.
			serde::put_str(dst, self.protocol_name)?;
			serde::put_u8(dst, self.protocol_level)?;

			// Write the flags and keep alive.
			serde::put_u8(dst, self.flags())?;
			serde::put_u16(dst, self.keep_alive)?;

			// Write the client ID.
			serde::put_str(dst, self.client_id)?;

			// Write the will.
			if let Some(will) = &self.will {
				serde::put_str(dst, will.topic.as_str())?;
				serde::put_slice(dst, will.payload)?;
			}

			// Write the credentials.
			if let Some(credentials) = &self.credentials {
				serde::put_str(dst, credentials.username)?;
				if let Some(password) = &credentials.password {
					serde::put_str(dst, password)?;
				}
			}

			Ok(())
		}

		#[inline(always)]
		fn payload_len(&self) -> usize {
			let mut len = 2 + self.protocol_name.len() + 4 + (2 + self.client_id.len());

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
	pub fn deserialize_from(frame: &Frame) -> Result<Self, DeserializeError> {
		let mut cursor = Cursor::from_frame(frame);

		if cursor.remaining() != 2 {
			return Err(DeserializeError::MalformedPacket(
				"ConnAck packet must have length 2",
			));
		}

		let flags = cursor.take_u8()?;
		let code = cursor.take_u8()?;

		if flags & 0xe0 != 0 {
			return Err(DeserializeError::MalformedPacket(
				"upper 7 bits in ConnAck flags must be zero",
			));
		}

		let session_present = flags & 0x01 == 0x01;

		Ok(Self {
			session_present,
			code,
		})
	}

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
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

impl<'a> Publish<'a> {
	pub fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError> {
		// payload is a Bytes, so this clone should be cheap.
		let mut reader = BytesReader::new(frame.payload.clone());

		// Extract properties from the header flags.
		let flags = frame.header & 0x0f;
		let retain = flags & PUBLISH_HEADER_RETAIN_FLAG == PUBLISH_HEADER_RETAIN_FLAG;
		let duplicate = flags & PUBLISH_HEADER_DUPLICATE_FLAG == PUBLISH_HEADER_DUPLICATE_FLAG;
		let qos: QoS = ((flags & PUBLISH_HEADER_QOS_MASK) >> 1).try_into()?;

		// We can't borrow strings from BytesReader, so construct a Cursor so we can
		// borrow the topic from the payload.
		let mut cursor = Cursor::from_frame(frame);
		let topic = Topic::new(cursor.take_str()?)?;

		// Sync the position in the reader with the position of the cursor.
		reader.advance(cursor.position());

		// The interpretation of the remaining bytes depends on the QoS.
		match qos {
			QoS::AtMostOnce => {
				if duplicate {
					return Err(DeserializeError::MalformedPacket(
						"duplicate flag must be 0 for Publish packets with QoS of AtMostOnce",
					));
				}

				let payload = reader.take_inner();

				Ok(Self::AtMostOnce {
					retain,
					topic,
					payload,
				})
			}
			QoS::AtLeastOnce => {
				let id = reader.take_id()?;
				let payload = reader.take_inner();

				Ok(Self::AtLeastOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
			QoS::ExactlyOnce => {
				let id = reader.take_id()?;
				let payload = reader.take_inner();

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

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
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
			Self::AtMostOnce { topic, .. }
			| Self::AtLeastOnce { topic, .. }
			| Self::ExactlyOnce { topic, .. } => topic,
		}
	}

	/// Returns the payload of the Publish packet.
	#[inline]
	pub fn payload(&self) -> &[u8] {
		match self {
			Self::AtMostOnce { payload, .. }
			| Self::AtLeastOnce { payload, .. }
			| Self::ExactlyOnce { payload, .. } => payload,
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
			Self::AtMostOnce { retain, .. }
			| Self::AtLeastOnce { retain, .. }
			| Self::ExactlyOnce { retain, .. } => *retain,
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
			Self::AtLeastOnce { id, .. } | Self::ExactlyOnce { id, .. } => Some(*id),
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
			Self::AtLeastOnce { duplicate, .. } | Self::ExactlyOnce { duplicate, .. } => *duplicate,
		}
	}
}

impl fmt::Debug for Publish<'_> {
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

impl<'a> Subscribe<'a> {
	/// Parses the payload of a [`Subscribe`] packet.
	pub fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError> {
		let mut cursor = Cursor::from_frame(frame);

		let id = cursor.take_id()?;
		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = cursor.take_str()?;
			let qos = cursor.take_u8()?.try_into()?;
			filters.push((Filter::new(filter)?, qos));
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
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
	pub fn deserialize_from(frame: &Frame) -> Result<Self, DeserializeError> {
		let mut cursor = Cursor::from_frame(frame);
		let id = cursor.take_id()?;

		let mut result = Vec::new();
		while cursor.has_remaining() {
			let return_code = cursor.take_u8()?;
			let qos: Result<QoS, SubscribeFailed> = match return_code.try_into() {
				Ok(qos) => Ok(qos),
				Err(_) => {
					if return_code == 0x80 {
						Err(SubscribeFailed)
					} else {
						return Err(DeserializeError::MalformedPacket(
							"invalid return code in SubAck",
						));
					}
				}
			};

			result.push(qos);
		}

		Ok(Self { id, result })
	}

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
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

impl<'a> Unsubscribe<'a> {
	/// Deserializes an [`Unsubscribe`] packet.
	pub fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError> {
		let mut cursor = Cursor::from_frame(frame);

		let id = cursor.take_id()?;

		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = cursor.take_str()?;
			filters.push(Filter::new(filter)?);
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
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
pub enum DeserializeError {
	Incomplete,
	InvalidQoS,
	InvalidFilter(crate::InvalidFilter),
	InvalidTopic(crate::InvalidTopic),
	InvalidHeader,
	InvalidPacketId,
	MalformedLength,
	MalformedPacket(&'static str),
	Utf8Error(Utf8Error),
}

impl From<Utf8Error> for DeserializeError {
	#[inline]
	fn from(value: Utf8Error) -> Self {
		Self::Utf8Error(value)
	}
}

impl From<InvalidQoS> for DeserializeError {
	#[inline]
	fn from(_: InvalidQoS) -> Self {
		Self::InvalidQoS
	}
}

impl From<crate::InvalidTopic> for DeserializeError {
	fn from(value: crate::InvalidTopic) -> Self {
		Self::InvalidTopic(value)
	}
}

impl From<crate::InvalidFilter> for DeserializeError {
	#[inline]
	fn from(value: crate::InvalidFilter) -> Self {
		Self::InvalidFilter(value)
	}
}

impl fmt::Display for DeserializeError {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl error::Error for DeserializeError {}

macro_rules! impl_serde {
	($name:tt) => {
		impl SerializePacket for $name {
			fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
				Self::serialize_into(&self, dst)
			}
		}

		impl<'a> DeserializePacket<'a> for $name {
			fn deserialize_from(frame: &'a Frame) -> Result<Self, DeserializeError> {
				Self::deserialize_from(frame)
			}
		}
	};
	($name:tt,$a:tt) => {
		impl<$a> SerializePacket for $name<$a> {
			fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
				Self::serialize_into(&self, dst)
			}
		}

		impl<$a> DeserializePacket<$a> for $name<$a> {
			fn deserialize_from(frame: &$a Frame) -> Result<Self, DeserializeError> {
				Self::deserialize_from(frame)
			}
		}
	};
}

impl_serde!(Connect, 'a);
impl_serde!(ConnAck);
impl_serde!(Publish, 'a);
impl_serde!(PubAck);
impl_serde!(PubRec);
impl_serde!(PubRel);
impl_serde!(PubComp);
impl_serde!(Subscribe, 'a);
impl_serde!(SubAck);
impl_serde!(Unsubscribe, 'a);
impl_serde!(UnsubAck);
impl_serde!(PingReq);
impl_serde!(PingResp);
impl_serde!(Disconnect);

macro_rules! id_packet {
	($name:tt,$variant:expr,$header:literal,$label:literal) => {
		#[derive(Debug)]
		pub struct $name {
			pub id: PacketId,
		}

		impl $name {
			pub fn deserialize_from(frame: &Frame) -> Result<Self, DeserializeError> {
				let mut cursor = Cursor::from_frame(frame);

				if cursor.remaining() != 2 {
					return Err(DeserializeError::MalformedPacket(concat!(
						$label,
						" packet must have length 2"
					)));
				}

				let id = cursor.take_id()?;
				Ok(Self { id })
			}

			pub fn serialize_into(
				&self,
				dst: &mut impl BufMut,
			) -> Result<(), serde::SerializeError> {
				let Self { id } = self;
				crate::serde::put_u8(dst, $header)?;
				crate::serde::put_var(dst, 2)?;
				crate::serde::put_u16(dst, id.get())?;
				Ok(())
			}
		}

		impl<'a> From<$name> for Packet<'a> {
			#[inline]
			fn from(value: $name) -> Packet<'a> {
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
			pub fn deserialize_from(frame: &Frame) -> Result<Self, DeserializeError> {
				if frame.payload.len() != 0 {
					return Err(DeserializeError::MalformedPacket(
						"packet must have length 0",
					));
				}
				Ok(Self)
			}

			pub fn serialize_into(
				&self,
				dst: &mut impl BufMut,
			) -> Result<(), serde::SerializeError> {
				serde::put_u8(dst, $header)?;
				serde::put_var(dst, 0)?;
				Ok(())
			}
		}

		impl<'a> From<$name> for crate::packet::Packet<'a> {
			#[inline]
			fn from(_: $name) -> crate::packet::Packet<'a> {
				$variant
			}
		}
	};
}
use nul_packet;
