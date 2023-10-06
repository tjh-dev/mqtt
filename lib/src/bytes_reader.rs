use crate::packets::ParseError;
use bytes::{Buf, Bytes};
use std::num::NonZeroU16;
use ParseError as Error;
use ParseError::*;

type Result<T> = std::result::Result<T, Error>;

pub struct BytesReader {
	bytes: Bytes,
}

impl BytesReader {
	pub fn new(bytes: Bytes) -> Self {
		Self { bytes }
	}

	pub fn require(&self, len: usize) -> Result<()> {
		if self.bytes.remaining() >= len {
			Ok(())
		} else {
			Err(Incomplete)
		}
	}

	pub fn take_inner(self) -> Bytes {
		self.bytes
	}

	pub fn take_u8(&mut self) -> Result<u8> {
		self.require(1)?;
		Ok(self.bytes.get_u8())
	}

	pub fn take_u16(&mut self) -> Result<u16> {
		self.require(2)?;
		Ok(self.bytes.get_u16())
	}

	pub fn take_id(&mut self) -> Result<NonZeroU16> {
		let id = self.take_u16()?;
		let id = NonZeroU16::new(id).ok_or(Error::ZeroPacketId)?;
		Ok(id)
	}

	pub fn take_bytes(&mut self, len: usize) -> Result<Bytes> {
		self.require(len)?;
		Ok(self.bytes.split_to(len))
	}

	pub fn take_str_bytes(&mut self) -> Result<Bytes> {
		let len = self.take_u16()?;
		let bytes = self.take_bytes(len.into())?;
		Ok(bytes)
	}

	pub fn take_str(&mut self) -> Result<String> {
		let bytes = self.take_str_bytes()?;
		let s = String::from_utf8(bytes.into()).map_err(Error::FromUtf8Error)?;
		Ok(s)
	}
}
