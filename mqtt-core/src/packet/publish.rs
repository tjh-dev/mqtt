use super::{get_id, get_slice, get_str};
use crate::{QoS, WriteError};
use bytes::{Buf, BufMut, Bytes};
use std::io;

const HEADER: u8 = 0x30;
const FLAG_RETAIN: u8 = 0x01;
const FLAG_DUPLICATE: u8 = 0x04;
const MASK_QOS: u8 = 0x06;

#[derive(Debug)]
pub enum Publish {
	AtMostOnce {
		retain: bool,
		topic: String,
		payload: Bytes,
	},
	AtLeastOnce {
		id: u16,
		retain: bool,
		duplicate: bool,
		topic: String,
		payload: Bytes,
	},
	ExactlyOnce {
		id: u16,
		retain: bool,
		duplicate: bool,
		topic: String,
		payload: Bytes,
	},
}

impl Publish {
	pub fn parse(flags: u8, src: &mut io::Cursor<&[u8]>) -> Result<Self, super::Error> {
		// Extract properties from the header flags.
		let retain = flags & FLAG_RETAIN == FLAG_RETAIN;
		let duplicate = flags & FLAG_DUPLICATE == FLAG_DUPLICATE;
		let qos: QoS = ((flags & MASK_QOS) >> 1).try_into()?;

		let topic = String::from(get_str(src)?);

		// The interpretation of the remaining bytes depends on the QoS.
		match qos {
			QoS::AtMostOnce => {
				if duplicate {
					return Err(super::Error::MalformedPacket(
						"duplicate flag must be 0 for Publish packets with QoS of AtMostOnce",
					));
				}
				let payload = get_slice(src, src.remaining())?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::AtMostOnce {
					retain,
					topic,
					payload,
				})
			}
			QoS::AtLeastOnce => {
				let id = get_id(src)?;
				let payload = get_slice(src, src.remaining())?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::AtLeastOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
			QoS::ExactlyOnce => {
				let id = get_id(src)?;
				let payload = get_slice(src, src.remaining())?.to_vec();
				let payload = Bytes::from(payload);

				Ok(Self::AtLeastOnce {
					id,
					retain,
					duplicate,
					topic,
					payload,
				})
			}
		}
	}

	pub fn serialize_to_bytes(&self, dst: &mut impl BufMut) -> Result<(), WriteError> {
		match self {
			Self::AtMostOnce {
				retain,
				topic,
				payload,
			} => {
				let flags = retain.then_some(FLAG_RETAIN).unwrap_or(0);
				super::put_u8(dst, HEADER | flags)?;
				super::put_var(dst, 2 + topic.len() + payload.len())?;
				super::put_str(dst, topic)?;
				super::put_slice(dst, payload)?;
			}
			Self::AtLeastOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				let flags = retain.then_some(FLAG_RETAIN).unwrap_or(0)
					| duplicate.then_some(FLAG_DUPLICATE).unwrap_or(0);
				super::put_u8(dst, HEADER | flags)?;
				super::put_var(dst, 4 + topic.len() + payload.len())?;
				super::put_str(dst, topic)?;
				super::put_u16(dst, *id)?;
				super::put_slice(dst, payload)?;
			}
			Self::ExactlyOnce {
				id,
				retain,
				duplicate,
				topic,
				payload,
			} => {
				let flags = retain.then_some(FLAG_RETAIN).unwrap_or(0)
					| duplicate.then_some(FLAG_DUPLICATE).unwrap_or(0);
				super::put_u8(dst, HEADER | flags)?;
				super::put_var(dst, 4 + topic.len() + payload.len())?;
				super::put_str(dst, topic)?;
				super::put_u16(dst, *id)?;
				super::put_slice(dst, payload)?;
			}
		}

		Ok(())
	}

	/// Returns the topic of the Publish packet.
	#[inline(always)]
	pub fn topic(&self) -> &str {
		match self {
			Self::AtMostOnce { topic, .. } => topic,
			Self::AtLeastOnce { topic, .. } => topic,
			Self::ExactlyOnce { topic, .. } => topic,
		}
	}

	/// Returns the payload of the Publish packet.
	#[inline(always)]
	pub fn payload(&self) -> &Bytes {
		match self {
			Self::AtMostOnce { payload, .. } => payload,
			Self::AtLeastOnce { payload, .. } => payload,
			Self::ExactlyOnce { payload, .. } => payload,
		}
	}

	/// Returns the QoS of the Publish packet.
	#[inline(always)]
	pub fn qos(&self) -> QoS {
		match self {
			Self::AtMostOnce { .. } => QoS::AtMostOnce,
			Self::AtLeastOnce { .. } => QoS::AtLeastOnce,
			Self::ExactlyOnce { .. } => QoS::ExactlyOnce,
		}
	}

	/// Returns the retain flag of the Publish packet.
	#[inline(always)]
	pub fn retain(&self) -> bool {
		match self {
			Self::AtMostOnce { retain, .. } => *retain,
			Self::AtLeastOnce { retain, .. } => *retain,
			Self::ExactlyOnce { retain, .. } => *retain,
		}
	}

	/// Returns the Packet ID of the Publish packet.
	///
	/// This will always return `None` for Publish packets with [`QoS`] of
	/// [`AtMostOnce`].
	///
	/// [`AtMostOnce`]: QoS#variant.AtMostOnce
	#[inline(always)]
	pub fn id(&self) -> Option<u16> {
		match self {
			Self::AtMostOnce { .. } => None,
			Self::AtLeastOnce { id, .. } => Some(*id),
			Self::ExactlyOnce { id, .. } => Some(*id),
		}
	}

	/// Returns the duplicate flag of the Publish packet.
	///
	/// This will always return `false` for Publish packets with [`QoS`] of
	/// [`AtMostOnce`].
	///
	/// [`AtMostOnce`]: QoS#variant.AtMostOnce
	#[inline(always)]
	pub fn duplicate(&self) -> bool {
		match self {
			Self::AtMostOnce { .. } => false,
			Self::AtLeastOnce { duplicate, .. } => *duplicate,
			Self::ExactlyOnce { duplicate, .. } => *duplicate,
		}
	}
}
