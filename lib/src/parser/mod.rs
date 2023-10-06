use crate::{Filter, FilterError, InvalidQoS, InvalidTopic, PacketId, QoS, Topic};

pub trait Packet<'src> {
	fn from_slice(header: u8, payload: &'src [u8]) -> Result<Self, PacketError>
	where
		Self: Sized;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Insufficient;

#[derive(Debug)]
pub struct Scanner<'b> {
	pos: usize,
	buf: &'b [u8],
}

#[derive(Debug)]
pub struct PacketScanner<'b> {
	inner: Scanner<'b>,
}

#[derive(Debug)]
pub enum PacketError {
	Insufficient,
	Utf8Error(core::str::Utf8Error),
	InvalidFilter(FilterError),
	InvalidPacketId,
	InvalidQoS,
	InvalidTopic(InvalidTopic),
}

impl<'b> Scanner<'b> {
	pub fn new(buf: &'b [u8]) -> Self {
		Self { pos: 0, buf }
	}

	pub fn require(&self, amount: usize) -> Result<(), Insufficient> {
		if self.buf.len() >= self.pos + amount {
			Ok(())
		} else {
			Err(Insufficient)
		}
	}

	pub fn advance(&mut self, amount: usize) -> Result<(), Insufficient> {
		self.require(amount)?;
		self.pos += amount;
		Ok(())
	}

	pub fn take_slice(&mut self, len: usize) -> Result<&'b [u8], Insufficient> {
		let start = self.pos;
		self.advance(len)?;
		let slice = &self.buf[start..self.pos];
		Ok(slice)
	}

	pub fn take_u8(&mut self) -> Result<u8, Insufficient> {
		let slice = self.take_slice(1)?;
		Ok(slice[0])
	}

	pub fn take_u16(&mut self) -> Result<u16, Insufficient> {
		let slice = self.take_slice(2)?;
		Ok(u16::from_be_bytes([slice[0], slice[1]]))
	}
}

impl<'b> From<&'b [u8]> for Scanner<'b> {
	#[inline]
	fn from(value: &'b [u8]) -> Self {
		Self::new(value)
	}
}

impl From<Insufficient> for PacketError {
	fn from(_: Insufficient) -> Self {
		Self::Insufficient
	}
}

impl From<core::str::Utf8Error> for PacketError {
	fn from(value: core::str::Utf8Error) -> Self {
		Self::Utf8Error(value)
	}
}

impl From<InvalidQoS> for PacketError {
	fn from(_: InvalidQoS) -> Self {
		Self::InvalidQoS
	}
}

impl From<FilterError> for PacketError {
	fn from(value: FilterError) -> Self {
		Self::InvalidFilter(value)
	}
}

impl From<InvalidTopic> for PacketError {
	fn from(value: InvalidTopic) -> Self {
		Self::InvalidTopic(value)
	}
}

impl<'b> PacketScanner<'b> {
	pub fn new(src: &'b [u8]) -> Self {
		Self {
			inner: Scanner::new(src),
		}
	}

	pub fn take_slice(&mut self, len: usize) -> Result<&'b [u8], Insufficient> {
		self.inner.take_slice(len)
	}

	pub fn take_u8(&mut self) -> Result<u8, Insufficient> {
		self.inner.take_u8()
	}

	pub fn take_str(&mut self) -> Result<&'b str, PacketError> {
		let len = self.inner.take_u16()?;
		let slice = self.inner.take_slice(len.into())?;
		let str = core::str::from_utf8(slice)?;
		Ok(str)
	}

	pub fn take_id(&mut self) -> Result<PacketId, PacketError> {
		let id = self.inner.take_u16()?;
		let id = PacketId::new(id).ok_or(PacketError::InvalidPacketId)?;
		Ok(id)
	}

	pub fn take_len(&mut self) -> Result<u32, PacketError> {
		unimplemented!()
	}

	pub fn has_remaining(&self) -> bool {
		self.inner.buf.len() < self.inner.pos
	}

	pub fn remaining(&self) -> usize {
		self.inner.buf.len() - self.inner.pos
	}
}

pub enum Publish<'src> {
	AtMostOnce {
		retain: bool,
		topic: &'src Topic,
		payload: &'src [u8],
	},
	AtLeastOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: &'src Topic,
		payload: &'src [u8],
	},
	ExactlyOnce {
		id: PacketId,
		retain: bool,
		duplicate: bool,
		topic: &'src Topic,
		payload: &'src [u8],
	},
}

pub struct Subscribe<'src> {
	id: PacketId,
	filters: Vec<(&'src Filter, QoS)>,
}

impl<'src> Packet<'src> for Subscribe<'src> {
	fn from_slice(_: u8, payload: &'src [u8]) -> Result<Self, PacketError> {
		let mut scanner = PacketScanner::new(payload);
		let id = scanner.take_id()?;

		let mut filters = Vec::new();
		while scanner.has_remaining() {
			let filter = scanner.take_str()?;
			let qos: QoS = scanner.take_u8()?.try_into()?;
			filters.push((Filter::new(filter)?, qos));
		}

		Ok(Self { id, filters })
	}
}

impl<'src> Publish<'src> {
	const RETAIN_FLAG: u8 = 0x01;
	const DUPLICATE_FLAG: u8 = 0x08;
	const QOS_MASK: u8 = 0x06;
}

impl<'src> Packet<'src> for Publish<'src> {
	fn from_slice(header: u8, payload: &'src [u8]) -> Result<Self, PacketError> {
		let mut scanner = PacketScanner::new(payload);

		// Extract properties from the header flags.
		let retain = header & Self::RETAIN_FLAG == Self::RETAIN_FLAG;
		let duplicate = header & Self::DUPLICATE_FLAG == Self::DUPLICATE_FLAG;
		let qos: QoS = ((header & Self::QOS_MASK) >> 1).try_into()?;

		let topic = Topic::new(scanner.take_str()?)?;

		// The interpretation of the remaining bytes depends on the QoS.
		match qos {
			QoS::AtMostOnce => {
				if duplicate {
					panic!("duplicate flag must be 0 for Publish packets with QoS of AtMostOnce",);
				}
				let remaining = scanner.remaining();
				let payload = scanner.take_slice(remaining)?;

				Ok(Self::AtMostOnce {
					retain,
					topic,
					payload,
				})
			}
			QoS::AtLeastOnce => {
				let id = scanner.take_id()?;
				let remaining = scanner.remaining();
				let payload = scanner.take_slice(remaining)?;

				Ok(Self::AtLeastOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
			QoS::ExactlyOnce => {
				let id = scanner.take_id()?;
				let remaining = scanner.remaining();
				let payload = scanner.take_slice(remaining)?;

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
}

pub enum PacketType<'src> {
  Subscribe(Subscribe<'src>),
  Publish(Publish<'src>)
}

pub fn parse_packet<'s>(src: &'s [u8]) -> Result<PacketType<'s>, PacketError> {
  let mut scanner = PacketScanner::new(src);
  let header = scanner.take_u8()?;
  let len = scanner.take_len()?;
  let payload = scanner.take_slice(len as usize)?;

  match header {
    0x01 => {
      let packet = Subscribe::from_slice(header, payload)?;
      Ok(PacketType::Subscribe(packet))
    }
    _ => unimplemented!()
  }
}

#[cfg(test)]
mod tests {

	use super::{Insufficient, Scanner};

	#[test]
	fn require() {
		let scanner = Scanner::new(&[0x00]);
		assert_eq!(scanner.require(1), Ok(()));
		assert_eq!(scanner.require(2), Err(Insufficient))
	}
}
