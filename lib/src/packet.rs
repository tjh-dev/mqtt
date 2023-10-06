use crate::{
	packets::{
		ConnAck, Connect, DeserializeError, Disconnect, Frame, PingReq, PingResp, PubAck, PubComp,
		PubRec, PubRel, Publish, SubAck, Subscribe, UnsubAck, Unsubscribe,
	},
	serde,
};
use bytes::BufMut;
use std::io;

#[derive(Debug)]
pub enum Packet<'a> {
	Connect(Box<Connect<'a>>),
	ConnAck(ConnAck),
	Publish(Box<Publish>),
	PubAck(PubAck),
	PubRec(PubRec),
	PubRel(PubRel),
	PubComp(PubComp),
	Subscribe(Box<Subscribe<'a>>),
	SubAck(Box<SubAck>),
	Unsubscribe(Box<Unsubscribe<'a>>),
	UnsubAck(UnsubAck),
	PingReq,
	PingResp,
	Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketType {
	Connect,
	ConnAck,
	Publish,
	PubAck,
	PubRec,
	PubRel,
	PubComp,
	Subscribe,
	SubAck,
	Unsubscribe,
	UnsubAck,
	PingReq,
	PingResp,
	Disconnect,
}

const CONNECT: u8 = 0x10;
const CONNACK: u8 = 0x20;
const PUBLISH: u8 = 0x30;
const PUBACK: u8 = 0x40;
const PUBREC: u8 = 0x50;
const PUBREL: u8 = 0x60;
const PUBCOMP: u8 = 0x70;
const SUBSCRIBE: u8 = 0x80;
const SUBACK: u8 = 0x90;
const UNSUBSCRIBE: u8 = 0xa0;
const UNSUBACK: u8 = 0xb0;
const PINGREQ: u8 = 0xc0;
const PINGRESP: u8 = 0xd0;
const DISCONNECT: u8 = 0xe0;

impl<'a> Packet<'a> {
	/// Checks if a complete [`Packet`] can be decoded from `src`. If so,
	/// returns the length of the packet.
	pub fn check(src: &mut io::Cursor<&[u8]>) -> Result<u64, DeserializeError> {
		let header = serde::get_u8(src)?;
		if header == 0 || header == 0xf0 {
			return Err(DeserializeError::InvalidHeader);
		}

		let length = serde::get_var(src)?;
		let _ = serde::get_slice(src, length)?;
		Ok(src.position())
	}

	/// Parses a [`Packet`] from src.
	pub fn parse(frame: &'a Frame) -> Result<Self, DeserializeError> {
		let header = frame.header;

		let packet = match (header & 0xf0, header & 0x0f) {
			(CONNECT, 0x00) => Self::Connect(Box::new(frame.deserialize_packet()?)),
			(CONNACK, 0x00) => Self::ConnAck(frame.deserialize_packet()?),
			(PUBLISH, _) => Self::Publish(Box::new(frame.deserialize_packet()?)),
			(PUBACK, 0x00) => Self::PubAck(frame.deserialize_packet()?),
			(PUBREC, 0x00) => Self::PubRec(frame.deserialize_packet()?),
			(PUBREL, 0x02) => Self::PubRel(frame.deserialize_packet()?),
			(PUBCOMP, 0x00) => Self::PubComp(frame.deserialize_packet()?),
			(SUBSCRIBE, 0x02) => Self::Subscribe(Box::new(frame.deserialize_packet()?)),
			(SUBACK, 0x00) => SubAck::deserialize_from(frame)?.into(),
			(UNSUBSCRIBE, 0x02) => Unsubscribe::deserialize_from(frame)?.into(),
			(UNSUBACK, 0x00) => UnsubAck::deserialize_from(frame)?.into(),
			(PINGREQ, 0x00) => PingReq::deserialize_from(frame)?.into(),
			(PINGRESP, 0x00) => PingResp::deserialize_from(frame)?.into(),
			(DISCONNECT, 0x00) => Disconnect::deserialize_from(frame)?.into(),
			_ => return Err(DeserializeError::InvalidHeader),
		};

		Ok(packet)
	}

	pub fn serialize_into(&self, dst: &mut impl BufMut) -> Result<(), serde::SerializeError> {
		match self {
			Self::Connect(connect) => connect.serialize_into(dst),
			Self::ConnAck(connack) => connack.serialize_into(dst),
			Self::Publish(publish) => publish.serialize_into(dst),
			Self::PubAck(puback) => puback.serialize_into(dst),
			Self::PubRec(pubrec) => pubrec.serialize_into(dst),
			Self::PubRel(pubrel) => pubrel.serialize_into(dst),
			Self::PubComp(pubcomp) => pubcomp.serialize_into(dst),
			Self::Subscribe(subscribe) => subscribe.serialize_into(dst),
			Self::SubAck(suback) => suback.serialize_into(dst),
			Self::Unsubscribe(unsubscribe) => unsubscribe.serialize_into(dst),
			Self::UnsubAck(unsuback) => unsuback.serialize_into(dst),
			Self::PingReq => PingReq.serialize_into(dst),
			Self::PingResp => PingResp.serialize_into(dst),
			Self::Disconnect => Disconnect.serialize_into(dst),
		}
	}

	#[inline]
	pub fn packet_type(&self) -> PacketType {
		match self {
			Self::Connect(_) => PacketType::Connect,
			Self::ConnAck(_) => PacketType::ConnAck,
			Self::Publish(_) => PacketType::Publish,
			Self::PubAck(_) => PacketType::PubAck,
			Self::PubRec(_) => PacketType::PubRec,
			Self::PubRel(_) => PacketType::PubRel,
			Self::PubComp(_) => PacketType::PubComp,
			Self::Subscribe(_) => PacketType::Subscribe,
			Self::SubAck(_) => PacketType::SubAck,
			Self::Unsubscribe(_) => PacketType::Unsubscribe,
			Self::UnsubAck(_) => PacketType::UnsubAck,
			Self::PingReq => PacketType::PingReq,
			Self::PingResp => PacketType::PingResp,
			Self::Disconnect => PacketType::Disconnect,
		}
	}
}

impl<'a> From<Connect<'a>> for Packet<'a> {
	#[inline]
	fn from(value: Connect<'a>) -> Self {
		Self::Connect(value.into())
	}
}

impl<'a> From<ConnAck> for Packet<'a> {
	#[inline]
	fn from(value: ConnAck) -> Self {
		Self::ConnAck(value)
	}
}

impl<'a> From<Publish> for Packet<'a> {
	#[inline]
	fn from(value: Publish) -> Self {
		Self::Publish(value.into())
	}
}

impl<'a> From<Subscribe<'a>> for Packet<'a> {
	#[inline]
	fn from(value: Subscribe<'a>) -> Self {
		Self::Subscribe(value.into())
	}
}

impl<'a> From<SubAck> for Packet<'a> {
	#[inline]
	fn from(value: SubAck) -> Self {
		Self::SubAck(value.into())
	}
}

impl<'a> From<Unsubscribe<'a>> for Packet<'a> {
	#[inline]
	fn from(value: Unsubscribe<'a>) -> Self {
		Self::Unsubscribe(value.into())
	}
}
