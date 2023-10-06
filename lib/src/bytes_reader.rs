use crate::packets::{DeserializeError, Frame};
use bytes::{Buf, Bytes};
use std::num::NonZeroU16;
use std::str::from_utf8;
use DeserializeError as Error;
use DeserializeError::*;

type Result<T> = std::result::Result<T, Error>;

pub struct BytesReader {
	buffer: Bytes,
}

pub struct Cursor<'b> {
	position: usize,
	buffer: &'b [u8],
}

impl BytesReader {
	pub fn new(bytes: Bytes) -> Self {
		Self { buffer: bytes }
	}

	pub fn remaining(&self) -> usize {
		self.buffer.remaining()
	}

	pub fn require(&self, len: usize) -> Result<()> {
		if self.buffer.remaining() >= len {
			Ok(())
		} else {
			Err(Incomplete)
		}
	}

	pub fn take_inner(self) -> Bytes {
		self.buffer
	}

	#[allow(unused)]
	pub fn take_u8(&mut self) -> Result<u8> {
		self.require(1)?;
		Ok(self.buffer.get_u8())
	}

	pub fn take_u16(&mut self) -> Result<u16> {
		self.require(2)?;
		Ok(self.buffer.get_u16())
	}

	pub fn take_id(&mut self) -> Result<NonZeroU16> {
		let id = self.take_u16()?;
		let id = NonZeroU16::new(id).ok_or(Error::ZeroPacketId)?;
		Ok(id)
	}

	pub fn take_bytes(&mut self, len: usize) -> Result<Bytes> {
		self.require(len)?;
		Ok(self.buffer.split_to(len))
	}

	pub fn take_str_bytes(&mut self) -> Result<Bytes> {
		let len = self.take_u16()?;
		let bytes = self.take_bytes(len.into())?;
		Ok(bytes)
	}

	pub fn take_str(&mut self) -> Result<String> {
		let bytes = self.take_str_bytes()?;
		let s = String::from_utf8(bytes.into()).map_err(|e| Error::Utf8Error(e.utf8_error()))?;
		Ok(s)
	}

	pub fn take_var(&mut self) -> Result<usize> {
		let mut value = 0;
		for multiplier in [0x01, 0x80, 0x4000, 0x200000, usize::MAX] {
			// Detect if we've read too many bytes.
			if multiplier == usize::MAX {
				return Err(DeserializeError::MalformedLength);
			}

			let encoded = self.take_u8()? as usize;
			value += (encoded & 0x7f) * multiplier;

			// exit early if we've reached the last byte.
			if encoded & 0x80 == 0 {
				break;
			}
		}

		Ok(value)
	}
}

impl<'b> Cursor<'b> {
	pub fn new(buffer: &'b [u8]) -> Self {
		Self {
			position: 0,
			buffer,
		}
	}

	pub fn from_frame(frame: &'b Frame) -> Self {
		Self::new(frame.payload.as_ref())
	}

	pub fn position(&self) -> usize {
		self.position
	}

	pub fn remaining(&self) -> usize {
		self.buffer.len() - self.position
	}

	pub fn has_remaining(&self) -> bool {
		self.remaining() > 0
	}

	pub fn require(&self, len: usize) -> Result<()> {
		if self.buffer.remaining() >= len {
			Ok(())
		} else {
			Err(Incomplete)
		}
	}

	pub fn take_u8(&mut self) -> Result<u8> {
		self.require(1)?;
		let value = self.buffer[self.position];
		self.position += 1;
		Ok(value)
	}

	pub fn take_u16(&mut self) -> Result<u16> {
		self.require(2)?;
		let slice = self.take_slice(2)?;
		let value = u16::from_be_bytes([slice[0], slice[1]]);
		Ok(value)
	}

	pub fn take_id(&mut self) -> Result<NonZeroU16> {
		let id = self.take_u16()?;
		let id = NonZeroU16::new(id).ok_or(Error::ZeroPacketId)?;
		Ok(id)
	}

	pub fn take_slice(&mut self, len: usize) -> Result<&'b [u8]> {
		self.require(len)?;
		let slice = &self.buffer[self.position..self.position + len];
		self.position += len;
		Ok(slice)
	}

	pub fn take_str(&mut self) -> Result<&'b str> {
		let len = self.take_u16()?;
		let bytes = self.take_slice(len.into())?;
		let string = from_utf8(bytes)?;
		Ok(string)
	}

	pub fn take_var(&mut self) -> Result<usize> {
		let mut value = 0;
		for multiplier in [0x01, 0x80, 0x4000, 0x200000, usize::MAX] {
			// Detect if we've read too many bytes.
			if multiplier == usize::MAX {
				return Err(DeserializeError::MalformedLength);
			}

			let encoded = self.take_u8()? as usize;
			value += (encoded & 0x7f) * multiplier;

			// exit early if we've reached the last byte.
			if encoded & 0x80 == 0 {
				break;
			}
		}

		Ok(value)
	}
}
