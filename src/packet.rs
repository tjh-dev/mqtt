use crate::{
	packets::{
		ConnAck, Connect, Disconnect, ParseError, PingReq, PingResp, PubAck, PubComp, PubRec,
		PubRel, Publish, SubAck, Subscribe, UnsubAck, Unsubscribe,
	},
	serde,
};
use bytes::BufMut;
use std::io;

#[derive(Debug)]
pub enum Packet {
	Connect(Connect),
	ConnAck(ConnAck),
	Publish(Publish),
	PubAck(PubAck),
	PubRec(PubRec),
	PubRel(PubRel),
	PubComp(PubComp),
	Subscribe(Subscribe),
	SubAck(SubAck),
	Unsubscribe(Unsubscribe),
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

impl Packet {
	/// Checks if a complete [`Packet`] can be decoded from `src`. If so, returns the length of the
	/// packet.
	pub fn check(src: &mut io::Cursor<&[u8]>) -> Result<u64, ParseError> {
		let header = serde::get_u8(src)?;
		if header == 0 || header == 0xf0 {
			return Err(ParseError::InvalidHeader);
		}

		let length = serde::get_var(src)?;
		let _ = serde::get_slice(src, length)?;
		Ok(src.position())
	}

	/// Parses a [`Packet`] from src.
	pub fn parse(src: &mut io::Cursor<&[u8]>) -> Result<Self, ParseError> {
		let header = serde::get_u8(src)?;
		let length = serde::get_var(src)?;
		let payload = serde::get_slice(src, length)?;

		match (header & 0xf0, header & 0x0f) {
			(CONNECT, 0x00) => Ok(Connect::parse(payload)?.into()),
			(CONNACK, 0x00) => Ok(ConnAck::parse(payload)?.into()),
			(PUBLISH, flags) => Ok(Publish::parse(payload, flags)?.into()),
			(PUBACK, 0x00) => Ok(PubAck::parse(payload)?.into()),
			(PUBREC, 0x00) => Ok(PubRec::parse(payload)?.into()),
			(PUBREL, 0x02) => Ok(PubRel::parse(payload)?.into()),
			(PUBCOMP, 0x00) => Ok(PubComp::parse(payload)?.into()),
			(SUBSCRIBE, 0x02) => Ok(Subscribe::parse(payload)?.into()),
			(SUBACK, 0x00) => Ok(SubAck::parse(payload)?.into()),
			(UNSUBSCRIBE, 0x02) => Ok(Unsubscribe::parse(payload)?.into()),
			(UNSUBACK, 0x00) => Ok(UnsubAck::parse(payload)?.into()),
			(PINGREQ, 0x00) => Ok(PingReq::parse(payload)?.into()),
			(PINGRESP, 0x00) => Ok(PingResp::parse(payload)?.into()),
			(DISCONNECT, 0x00) => Ok(Disconnect::parse(payload)?.into()),
			_ => Err(ParseError::InvalidHeader),
		}
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), serde::WriteError> {
		match self {
			Self::Connect(connect) => connect.serialize_to_bytes(dst),
			Self::ConnAck(connack) => connack.serialize_to_bytes(dst),
			Self::Publish(publish) => publish.serialize_to_bytes(dst),
			Self::PubAck(puback) => puback.serialize_to_bytes(dst),
			Self::PubRec(pubrec) => pubrec.serialize_to_bytes(dst),
			Self::PubRel(pubrel) => pubrel.serialize_to_bytes(dst),
			Self::PubComp(pubcomp) => pubcomp.serialize_to_bytes(dst),
			Self::Subscribe(subscribe) => subscribe.serialize_to_bytes(dst),
			Self::SubAck(suback) => suback.serialize_to_bytes(dst),
			Self::Unsubscribe(unsubscribe) => unsubscribe.serialize_to_bytes(dst),
			Self::UnsubAck(unsuback) => unsuback.serialize_to_bytes(dst),
			Self::PingReq => PingReq.serialize_to_bytes(dst),
			Self::PingResp => PingResp.serialize_to_bytes(dst),
			Self::Disconnect => Disconnect.serialize_to_bytes(dst),
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

impl From<Connect> for Packet {
	#[inline]
	fn from(value: Connect) -> Self {
		Self::Connect(value)
	}
}

impl From<ConnAck> for Packet {
	#[inline]
	fn from(value: ConnAck) -> Self {
		Self::ConnAck(value)
	}
}

impl From<Publish> for Packet {
	#[inline]
	fn from(value: Publish) -> Self {
		Self::Publish(value)
	}
}

impl From<Subscribe> for Packet {
	#[inline]
	fn from(value: Subscribe) -> Self {
		Self::Subscribe(value)
	}
}

impl From<SubAck> for Packet {
	#[inline]
	fn from(value: SubAck) -> Self {
		Self::SubAck(value)
	}
}

impl From<Unsubscribe> for Packet {
	#[inline]
	fn from(value: Unsubscribe) -> Self {
		Self::Unsubscribe(value)
	}
}
