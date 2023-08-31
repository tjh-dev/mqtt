mod connect;
mod publish;

use crate::{qos::InvalidQoS, FilterBuf, FilterError, QoS};
use bytes::{Buf, BufMut};
use std::{
	error, fmt, io, mem,
	str::{from_utf8, Utf8Error},
};

pub use self::{connect::Connect, publish::Publish};

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
	ConnAck {
		session_present: bool,
		code: u8,
	},
	Publish(Publish),
	PubAck {
		id: u16,
	},
	PubRec {
		id: u16,
	},
	PubRel {
		id: u16,
	},
	PubComp {
		id: u16,
	},
	Subscribe {
		id: u16,
		filters: Vec<(FilterBuf, QoS)>,
	},
	SubAck {
		id: u16,
		result: Vec<Option<QoS>>,
	},
	Unsubscribe {
		id: u16,
		filters: Vec<String>,
	},
	UnsubAck {
		id: u16,
	},
	PingReq,
	PingResp,
	Disconnect,
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
			(control::CONNECT, 0x00) => {
				let mut buf = io::Cursor::new(payload);
				let connect = Connect::parse(&mut buf)?;
				Ok(Self::Connect(connect))
			}
			(control::CONNACK, 0x00) => {
				if length != 2 {
					return Err(Error::MalformedPacket("ConnAck packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let flags = get_u8(&mut buf)?;
				let code = get_u8(&mut buf)?;

				if flags & 0xe0 != 0 {
					return Err(Error::MalformedPacket(
						"upper 7 bits in ConnAck flags must be zero",
					));
				}

				let session_present = flags & 0x01 == 0x01;

				Ok(Self::ConnAck {
					session_present,
					code,
				})
			}
			(control::PUBLISH, flags) => {
				let mut buf = io::Cursor::new(payload);
				let publish = Publish::parse(flags, &mut buf)?;
				Ok(Self::Publish(publish))
			}
			(control::PUBACK, 0x00) => {
				if length != 2 {
					return Err(Error::MalformedPacket("PubAck packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;
				Ok(Self::PubAck { id })
			}
			(control::PUBREC, 0x00) => {
				if length != 2 {
					return Err(Error::MalformedPacket("PubRec packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;
				Ok(Self::PubRec { id })
			}
			(control::PUBREL, 0x02) => {
				if length != 2 {
					return Err(Error::MalformedPacket("PubRel packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;
				Ok(Self::PubRel { id })
			}
			(control::PUBCOMP, 0x00) => {
				if length != 2 {
					return Err(Error::MalformedPacket("PubComp packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;
				Ok(Self::PubComp { id })
			}
			(control::SUBSCRIBE, 0x02) => {
				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;

				let mut filters = Vec::new();
				while buf.has_remaining() {
					let filter = get_str(&mut buf)?;
					let qos: QoS = get_u8(&mut buf)?.try_into()?;
					filters.push((FilterBuf::new(filter)?, qos));
				}

				Ok(Self::Subscribe { id, filters })
			}
			(control::SUBACK, 0x00) => {
				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;

				let mut result = Vec::new();
				while buf.has_remaining() {
					let return_code = get_u8(&mut buf)?;
					let qos: Option<QoS> = match return_code.try_into() {
						Ok(qos) => Some(qos),
						Err(InvalidQoS) => {
							if return_code == 0x80 {
								None
							} else {
								return Err(Error::MalformedPacket(
									"invalid return code in SubAck",
								));
							}
						}
					};

					result.push(qos);
				}

				Ok(Self::SubAck { id, result })
			}
			(control::UNSUBSCRIBE, 0x02) => {
				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;

				let mut filters = Vec::new();
				while buf.has_remaining() {
					let filter = get_str(&mut buf)?;
					filters.push(String::from(filter));
				}

				Ok(Self::Unsubscribe { id, filters })
			}
			(control::UNSUBACK, 0x00) => {
				if length != 2 {
					return Err(Error::MalformedPacket("UnsubAck packet must have length 2"));
				}

				let mut buf = io::Cursor::new(payload);
				let id = get_id(&mut buf)?;
				Ok(Self::UnsubAck { id })
			}
			(control::PINGREQ, 0x00) => {
				if length != 0 {
					return Err(Error::MalformedPacket("PingReq packet must have length 0"));
				}
				Ok(Self::PingReq)
			}
			(control::PINGRESP, 0x00) => {
				if length != 0 {
					return Err(Error::MalformedPacket("PingResp packet must have length 0"));
				}
				Ok(Self::PingResp)
			}
			(control::DISCONNECT, 0x00) => {
				if length != 0 {
					return Err(Error::MalformedPacket(
						"Disconnect packet must have length 0",
					));
				}
				Ok(Self::Disconnect)
			}
			_ => Err(Error::InvalidHeader),
		}
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		match self {
			Self::Connect(connect) => connect.serialize_to_bytes(dst),
			Self::ConnAck {
				session_present,
				code,
			} => {
				put_u8(dst, 0x20)?;
				put_var(dst, 2)?;
				put_u8(dst, if *session_present { 0x01 } else { 0x00 })?;
				put_u8(dst, *code)?;
				Ok(())
			}
			Self::Publish(publish) => publish.serialize_to_bytes(dst),
			Self::PubAck { id } => {
				put_u8(dst, 0x40)?;
				put_var(dst, 2)?;
				put_u16(dst, *id)?;
				Ok(())
			}
			Self::PubRec { id } => {
				put_u8(dst, 0x50)?;
				put_var(dst, 2)?;
				put_u16(dst, *id)?;
				Ok(())
			}
			Self::PubRel { id } => {
				put_u8(dst, 0x62)?;
				put_var(dst, 2)?;
				put_u16(dst, *id)?;
				Ok(())
			}
			Self::PubComp { id } => {
				put_u8(dst, 0x70)?;
				put_var(dst, 2)?;
				put_u16(dst, *id)?;
				Ok(())
			}
			Self::Subscribe { id, filters } => {
				put_u8(dst, 0x82)?;

				let len = 2 + filters
					.iter()
					.fold(0usize, |acc, (filter, _)| acc + 3 + filter.len());

				put_var(dst, len)?;
				put_u16(dst, *id)?;
				for (filter, qos) in filters {
					put_str(dst, filter.as_str())?;
					put_u8(dst, *qos as u8)?;
				}

				Ok(())
			}
			Self::SubAck { id, result } => {
				put_u8(dst, 0x90)?;

				let len = 2 + result.len();

				put_var(dst, len)?;
				put_u16(dst, *id)?;
				for qos in result {
					put_u8(dst, qos.map(|qos| qos as u8).unwrap_or(0x80))?;
				}

				Ok(())
			}
			Self::Unsubscribe { id, filters } => {
				put_u8(dst, 0xa2)?;

				let len = 2 + filters
					.iter()
					.fold(0usize, |acc, filter| acc + 2 + filter.len());

				put_var(dst, len)?;
				put_u16(dst, *id)?;
				for filter in filters {
					put_str(dst, filter)?;
				}

				Ok(())
			}
			Self::UnsubAck { id } => {
				put_u8(dst, 0xb0)?;
				put_var(dst, 2)?;
				put_u16(dst, *id)?;
				Ok(())
			}
			Self::PingReq => {
				put_u16(dst, 0xc000)?;
				Ok(())
			}
			Self::PingResp => {
				put_u16(dst, 0xd000)?;
				Ok(())
			}
			Self::Disconnect => {
				put_u16(dst, 0xe000)?;
				Ok(())
			}
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
