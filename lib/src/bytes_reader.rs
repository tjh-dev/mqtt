use crate::{packets::DeserializeError, Frame};
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

	pub fn advance(&mut self, amount: usize) {
		self.buffer.advance(amount)
	}

	pub fn require(&self, len: usize) -> Result<()> {
		if self.remaining() < len {
			return Err(Incomplete);
		}

		Ok(())
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
		let id = NonZeroU16::new(id).ok_or(Error::InvalidPacketId)?;
		Ok(id)
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
		if self.remaining() < len {
			return Err(Incomplete);
		}

		Ok(())
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
		let id = NonZeroU16::new(id).ok_or(Error::InvalidPacketId)?;
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

#[cfg(test)]
mod tests {
	use super::Cursor;

	#[test]
	fn remaining() {
		let buffer = [0x01, 0x02, 0x03, 0x04];

		let mut cursor = Cursor::new(&buffer);
		assert!(cursor.require(buffer.len()).is_ok());
		assert!(cursor.require(buffer.len() + 1).is_err());
		let s1 = cursor.take_slice(buffer.len()).unwrap();

		assert_eq!(s1, buffer);
		assert_eq!(cursor.remaining(), 0);
		assert_eq!(cursor.position, 4);

		assert!(cursor.require(0).is_ok());
		assert!(cursor.require(1).is_err());
		assert!(cursor.require(2).is_err());
	}
}
