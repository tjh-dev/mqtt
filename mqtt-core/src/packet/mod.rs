mod connect;
mod publish;
mod subscribe;

use crate::{qos::InvalidQoS, FilterError};
use bytes::{Buf, BufMut};
use std::{
	error, fmt, io, mem,
	str::{from_utf8, Utf8Error},
};

pub use self::{
	connect::{ConnAck, Connect},
	publish::{PubAck, PubComp, PubRec, PubRel, Publish},
	subscribe::{SubAck, Subscribe, UnsubAck, Unsubscribe},
};

mod control {
	pub const CONNECT: u8 = 0x10;
	pub const CONNACK: u8 = 0x20;
	pub const PUBLISH: u8 = 0x30;
	pub const PUBACK: u8 = 0x40;
	pub const PUBREC: u8 = 0x50;
	pub const PUBREL: u8 = 0x60;
	pub const PUBCOMP: u8 = 0x70;
	pub const SUBSCRIBE: u8 = 0x80;
	pub const SUBACK: u8 = 0x90;
	pub const UNSUBSCRIBE: u8 = 0xa0;
	pub const UNSUBACK: u8 = 0xb0;
	pub const PINGREQ: u8 = 0xc0;
	pub const PINGRESP: u8 = 0xd0;
	pub const DISCONNECT: u8 = 0xe0;
}

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
	PingReq(PingReq),
	PingResp(PingResp),
	Disconnect(Disconnect),
}

#[derive(Debug)]
pub enum Error {
	Incomplete,
	InvalidQoS,
	InvalidFilter(FilterError),
	InvalidHeader,
	ZeroPacketId,
	MalformedLength,
	MalformedPacket(&'static str),
	Utf8Error(Utf8Error),
}

#[derive(Debug)]
pub enum WriteError {
	Overflow,
}

impl From<Utf8Error> for Error {
	fn from(value: Utf8Error) -> Self {
		Self::Utf8Error(value)
	}
}

impl From<InvalidQoS> for Error {
	fn from(_: InvalidQoS) -> Self {
		Self::InvalidQoS
	}
}

impl From<FilterError> for Error {
	fn from(value: FilterError) -> Self {
		Self::InvalidFilter(value)
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl error::Error for Error {}

impl Packet {
	/// Checks if a complete [`Packet`] can be decoded from `src`.
	pub fn check(src: &mut io::Cursor<&[u8]>) -> Result<(), Error> {
		let header = get_u8(src)?;
		if header == 0 {
			return Err(Error::InvalidHeader);
		}

		let length = get_var(src)?;
		let _ = get_slice(src, length)?;
		Ok(())
	}

	pub fn parse(src: &mut io::Cursor<&[u8]>) -> Result<Self, Error> {
		let header = get_u8(src)?;
		let length = get_var(src)?;
		let payload = get_slice(src, length)?;

		match (header & 0xf0, header & 0x0f) {
			(control::CONNECT, 0x00) => Ok(Connect::parse(payload)?.into()),
			(control::CONNACK, 0x00) => Ok(ConnAck::parse(payload)?.into()),
			(control::PUBLISH, flags) => Ok(Publish::parse(payload, flags)?.into()),
			(control::PUBACK, 0x00) => Ok(PubAck::parse(payload)?.into()),
			(control::PUBREC, 0x00) => Ok(PubRec::parse(payload)?.into()),
			(control::PUBREL, 0x02) => Ok(PubRel::parse(payload)?.into()),
			(control::PUBCOMP, 0x00) => Ok(PubComp::parse(payload)?.into()),
			(control::SUBSCRIBE, 0x02) => Ok(Subscribe::parse(payload)?.into()),
			(control::SUBACK, 0x00) => Ok(SubAck::parse(payload)?.into()),
			(control::UNSUBSCRIBE, 0x02) => Ok(Unsubscribe::parse(payload)?.into()),
			(control::UNSUBACK, 0x00) => Ok(UnsubAck::parse(payload)?.into()),
			(control::PINGREQ, 0x00) => Ok(PingReq::parse(payload)?.into()),
			(control::PINGRESP, 0x00) => Ok(PingResp::parse(payload)?.into()),
			(control::DISCONNECT, 0x00) => Ok(Disconnect::parse(payload)?.into()),
			_ => Err(Error::InvalidHeader),
		}
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
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
			Self::PingReq(pingreq) => pingreq.serialize_to_bytes(dst),
			Self::PingResp(pingresp) => pingresp.serialize_to_bytes(dst),
			Self::Disconnect(disconnect) => disconnect.serialize_to_bytes(dst),
		}
	}
}

#[inline(always)]
fn require(src: &mut io::Cursor<&[u8]>, len: usize) -> Result<(), Error> {
	if src.remaining() < len {
		Err(Error::Incomplete)
	} else {
		Ok(())
	}
}

#[inline(always)]
fn require_mut(dst: &mut impl BufMut, len: usize) -> Result<(), WriteError> {
	if dst.remaining_mut() < len {
		Err(WriteError::Overflow)
	} else {
		Ok(())
	}
}

#[inline(always)]
fn get_u8(src: &mut io::Cursor<&[u8]>) -> Result<u8, Error> {
	require(src, mem::size_of::<u8>())?;
	Ok(src.get_u8())
}

fn put_u8(dst: &mut impl BufMut, val: u8) -> Result<(), WriteError> {
	require_mut(dst, mem::size_of::<u8>())?;
	dst.put_u8(val);
	Ok(())
}

#[inline(always)]
fn get_u16(src: &mut io::Cursor<&[u8]>) -> Result<u16, Error> {
	require(src, mem::size_of::<u16>())?;
	Ok(src.get_u16())
}

#[inline(always)]
fn put_u16(dst: &mut impl BufMut, val: u16) -> Result<(), WriteError> {
	require_mut(dst, mem::size_of::<u16>())?;
	dst.put_u16(val);
	Ok(())
}

#[inline(always)]
fn get_id(src: &mut io::Cursor<&[u8]>) -> Result<u16, Error> {
	let id = get_u16(src)?;
	if id == 0 {
		return Err(Error::ZeroPacketId);
	}
	Ok(id)
}

#[inline(always)]
fn get_slice<'s>(src: &mut io::Cursor<&'s [u8]>, len: usize) -> Result<&'s [u8], Error> {
	require(src, len)?;
	let position = src.position() as usize;
	src.advance(len);
	Ok(&src.get_ref()[position..position + len])
}

#[inline(always)]
fn put_slice(dst: &mut impl BufMut, slice: &[u8]) -> Result<(), WriteError> {
	require_mut(dst, slice.len())?;
	dst.put_slice(slice);
	Ok(())
}

#[inline(always)]
fn get_str<'s>(src: &mut io::Cursor<&'s [u8]>) -> Result<&'s str, Error> {
	let len = get_u16(src)? as usize;
	let slice = get_slice(src, len)?;
	let s = from_utf8(slice)?;
	Ok(s)
}

#[inline(always)]
fn put_str(dst: &mut impl BufMut, s: &str) -> Result<(), WriteError> {
	if s.len() > u16::MAX as usize {
		return Err(WriteError::Overflow);
	}
	put_u16(dst, s.len() as u16)?;
	put_slice(dst, s.as_bytes())
}

#[inline(always)]
fn get_var(src: &mut io::Cursor<&[u8]>) -> Result<usize, Error> {
	let mut value = 0;
	for multiplier in [0x01, 0x80, 0x4000, 0x200000, usize::MAX] {
		// Detect if we've read too many bytes.
		if multiplier == usize::MAX {
			return Err(Error::MalformedLength);
		}

		let encoded = get_u8(src)? as usize;
		value += (encoded & 0x7f) * multiplier;

		// exit early if we've reached the last byte.
		if encoded & 0x80 == 0 {
			break;
		}
	}

	Ok(value)
}

#[inline(always)]
fn put_var(dst: &mut impl BufMut, mut value: usize) -> Result<(), WriteError> {
	if value > 268_435_455 {
		return Err(WriteError::Overflow);
	}

	loop {
		let mut encoded = value % 0x80;
		value /= 0x80;
		if value > 0 {
			encoded |= 0x80;
		}
		put_u8(dst, encoded as u8)?;
		if value == 0 {
			break Ok(());
		}
	}
}

macro_rules! id_packet {
	($name:tt,$variant:expr,$header:literal) => {
		#[derive(Debug)]
		pub struct $name {
			pub id: PacketId,
		}

		impl $name {
			pub fn parse(payload: &[u8]) -> Result<Self, Error> {
				if payload.len() != 2 {
					return Err(Error::MalformedPacket("packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = super::get_id(&mut buf)?;
				Ok(Self { id })
			}

			pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
				let Self { id } = self;
				super::put_u8(dst, $header)?;
				super::put_var(dst, 2)?;
				super::put_u16(dst, *id)?;
				Ok(())
			}
		}

		impl From<$name> for Packet {
			fn from(value: $name) -> Packet {
				$variant(value)
			}
		}
	};
}

macro_rules! nul_packet {
	($name:tt,$variant:expr,$header:literal) => {
		#[derive(Debug)]
		pub struct $name;

		impl $name {
			pub fn parse(payload: &[u8]) -> Result<Self, Error> {
				if payload.len() != 0 {
					return Err(Error::MalformedPacket("packet must have length 0"));
				}
				Ok(Self)
			}

			pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
				put_u8(dst, $header)?;
				put_var(dst, 0)?;
				Ok(())
			}
		}

		impl From<$name> for Packet {
			fn from(value: $name) -> Packet {
				$variant(value)
			}
		}
	};
}

use id_packet;
use nul_packet;

nul_packet!(PingReq, Packet::PingReq, 0xc0);
nul_packet!(PingResp, Packet::PingResp, 0xd0);
nul_packet!(Disconnect, Packet::Disconnect, 0xe0);
