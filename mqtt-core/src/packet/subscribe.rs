use super::{Error, Packet, WriteError};
use crate::{FilterBuf, PacketId, QoS};
use bytes::{Buf, BufMut};
use std::io;

#[derive(Debug)]
pub struct Subscribe {
	pub id: PacketId,
	pub filters: Vec<(FilterBuf, QoS)>,
}

#[derive(Debug)]
pub struct SubAck {
	pub id: PacketId,
	pub result: Vec<Option<QoS>>,
}

#[derive(Debug)]
pub struct Unsubscribe {
	pub id: PacketId,
	pub filters: Vec<String>,
}

super::id_packet!(UnsubAck, Packet::UnsubAck, 0xb0);

impl Subscribe {
	/// Parses the payload of a [`Subscribe`] packet.
	pub fn parse(payload: &[u8]) -> Result<Self, Error> {
		let mut cursor = io::Cursor::new(payload);
		let id = super::get_id(&mut cursor)?;

		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = super::get_str(&mut cursor)?;
			let qos: QoS = super::get_u8(&mut cursor)?.try_into()?;
			filters.push((FilterBuf::new(filter)?, qos));
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		let Self { id, filters } = self;
		super::put_u8(dst, 0x82)?;

		let len = 2 + filters
			.iter()
			.fold(0usize, |acc, (filter, _)| acc + 3 + filter.len());

		super::put_var(dst, len)?;
		super::put_u16(dst, *id)?;
		for (filter, qos) in filters {
			super::put_str(dst, filter.as_str())?;
			super::put_u8(dst, *qos as u8)?;
		}

		Ok(())
	}
}

impl SubAck {
	pub fn parse(payload: &[u8]) -> Result<Self, Error> {
		let mut cursor = io::Cursor::new(payload);
		let id = super::get_id(&mut cursor)?;

		let mut result = Vec::new();
		while cursor.has_remaining() {
			let return_code = super::get_u8(&mut cursor)?;
			let qos: Option<QoS> = match return_code.try_into() {
				Ok(qos) => Some(qos),
				Err(_) => {
					if return_code == 0x80 {
						None
					} else {
						return Err(Error::MalformedPacket("invalid return code in SubAck"));
					}
				}
			};

			result.push(qos);
		}

		Ok(Self { id, result })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		let Self { id, result } = self;
		super::put_u8(dst, 0x90)?;

		let len = 2 + result.len();

		super::put_var(dst, len)?;
		super::put_u16(dst, *id)?;
		for qos in result {
			super::put_u8(dst, qos.map(|qos| qos as u8).unwrap_or(0x80))?;
		}

		Ok(())
	}
}

impl Unsubscribe {
	/// Parses the payload of a [`Subscribe`] packet.
	pub fn parse(payload: &[u8]) -> Result<Self, Error> {
		let mut cursor = io::Cursor::new(payload);
		let id = super::get_id(&mut cursor)?;

		let mut filters = Vec::new();
		while cursor.has_remaining() {
			let filter = super::get_str(&mut cursor)?;
			filters.push(String::from(filter));
		}

		Ok(Self { id, filters })
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		let Self { id, filters } = self;
		super::put_u8(dst, 0xa2)?;

		let len = 2 + filters
			.iter()
			.fold(0usize, |acc, filter| acc + 2 + filter.len());

		super::put_var(dst, len)?;
		super::put_u16(dst, *id)?;
		for filter in filters {
			super::put_str(dst, filter)?;
		}

		Ok(())
	}
}

impl From<Subscribe> for Packet {
	fn from(value: Subscribe) -> Self {
		Self::Subscribe(value)
	}
}

impl From<SubAck> for Packet {
	fn from(value: SubAck) -> Self {
		Self::SubAck(value)
	}
}

impl From<Unsubscribe> for Packet {
	fn from(value: Unsubscribe) -> Self {
		Self::Unsubscribe(value)
	}
}
