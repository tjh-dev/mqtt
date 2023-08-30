use super::{get_id, get_slice, get_str};
use crate::QoS;
use bytes::{Buf, Bytes};
use std::io;

const RETAIN_FLAG: u8 = 0x01;
const DUPLICATE_FLAG: u8 = 0x04;
const QOS_MASK: u8 = 0x06;

#[derive(Debug)]
pub struct Publish {
	pub topic: String,
	pub payload: Bytes,
	pub qos: crate::QoS,
	pub id: Option<u16>,
	pub retain: bool,
	pub duplicate: bool,
}

impl Publish {
	pub fn parse(flags: u8, src: &mut io::Cursor<&[u8]>) -> Result<Self, super::Error> {
		let retain = flags & RETAIN_FLAG == RETAIN_FLAG;
		let duplicate = flags & DUPLICATE_FLAG == DUPLICATE_FLAG;
		let qos: QoS = ((flags & QOS_MASK) >> 1).try_into()?;
		let id = match qos {
			QoS::AtMostOnce => None,
			QoS::AtLeastOnce | QoS::ExactlyOnce => {
				let id = get_id(src)?;
				Some(id)
			}
		};
		let topic = get_str(src)?;
		let payload = get_slice(src, src.remaining())?.to_vec();

		Ok(Self {
			topic: String::from(topic),
			payload: Bytes::from(payload),
			qos,
			id,
			retain,
			duplicate,
		})
	}
}
