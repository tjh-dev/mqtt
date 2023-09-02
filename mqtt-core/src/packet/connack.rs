use super::{Error, Packet};
use bytes::BufMut;
use std::io;

#[derive(Debug)]
pub struct ConnAck {
	pub session_present: bool,
	pub code: u8,
}

impl ConnAck {
	/// Parses the payload of a ConnAck packet.
	pub fn parse(payload: &[u8]) -> Result<Self, Error> {
		if payload.len() != 2 {
			return Err(Error::MalformedPacket("ConnAck packet must have length 2"));
		}

		let mut buf = io::Cursor::new(payload);
		let flags = super::get_u8(&mut buf)?;
		let code = super::get_u8(&mut buf)?;

		if flags & 0xe0 != 0 {
			return Err(Error::MalformedPacket(
				"upper 7 bits in ConnAck flags must be zero",
			));
		}

		let session_present = flags & 0x01 == 0x01;

		Ok(Self {
			session_present,
			code,
		})
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), super::WriteError> {
		let Self {
			session_present,
			code,
		} = self;
		super::put_u8(dst, 0x20)?;
		super::put_var(dst, 2)?;
		super::put_u8(dst, if *session_present { 0x01 } else { 0x00 })?;
		super::put_u8(dst, *code)?;
		Ok(())
	}
}

impl From<ConnAck> for Packet {
	#[inline]
	fn from(value: ConnAck) -> Self {
		Self::ConnAck(value)
	}
}
