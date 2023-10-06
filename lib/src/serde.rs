use crate::{packets::DeserializeError, PacketId};
use bytes::{Buf, BufMut};
use std::{io, mem, str::from_utf8};

#[derive(Debug)]
pub struct SerializeError;

pub fn require(src: &io::Cursor<&[u8]>, len: usize) -> Result<(), DeserializeError> {
	if src.remaining() < len {
		Err(DeserializeError::Incomplete)
	} else {
		Ok(())
	}
}

pub fn require_mut(dst: &impl BufMut, len: usize) -> Result<(), SerializeError> {
	if dst.remaining_mut() < len {
		Err(SerializeError)
	} else {
		Ok(())
	}
}

pub fn get_u8(src: &mut io::Cursor<&[u8]>) -> Result<u8, DeserializeError> {
	require(src, mem::size_of::<u8>())?;
	Ok(src.get_u8())
}

pub fn put_u8(dst: &mut impl BufMut, val: u8) -> Result<(), SerializeError> {
	require_mut(dst, mem::size_of::<u8>())?;
	dst.put_u8(val);
	Ok(())
}

pub fn get_u16(src: &mut io::Cursor<&[u8]>) -> Result<u16, DeserializeError> {
	require(src, mem::size_of::<u16>())?;
	Ok(src.get_u16())
}

pub fn put_u16(dst: &mut impl BufMut, val: u16) -> Result<(), SerializeError> {
	require_mut(dst, mem::size_of::<u16>())?;
	dst.put_u16(val);
	Ok(())
}

pub fn get_id(src: &mut io::Cursor<&[u8]>) -> Result<PacketId, DeserializeError> {
	let id = get_u16(src)?;
	let id = PacketId::new(id).ok_or(DeserializeError::ZeroPacketId)?;
	Ok(id)
}

pub fn get_slice<'s>(
	src: &mut io::Cursor<&'s [u8]>,
	len: usize,
) -> Result<&'s [u8], DeserializeError> {
	require(src, len)?;
	let position = src.position() as usize;
	src.advance(len);
	Ok(&src.get_ref()[position..position + len])
}

pub fn put_slice(dst: &mut impl BufMut, slice: &[u8]) -> Result<(), SerializeError> {
	require_mut(dst, slice.len())?;
	dst.put_slice(slice);
	Ok(())
}

pub fn get_str<'s>(src: &mut io::Cursor<&'s [u8]>) -> Result<&'s str, DeserializeError> {
	let len = get_u16(src)? as usize;
	let slice = get_slice(src, len)?;
	let s = from_utf8(slice)?;
	Ok(s)
}

pub fn put_str(dst: &mut impl BufMut, s: &str) -> Result<(), SerializeError> {
	if s.len() > u16::MAX as usize {
		return Err(SerializeError);
	}
	put_u16(dst, s.len() as u16)?;
	put_slice(dst, s.as_bytes())
}

pub fn get_var(src: &mut io::Cursor<&[u8]>) -> Result<usize, DeserializeError> {
	let mut value = 0;
	for multiplier in [0x01, 0x80, 0x4000, 0x200000, usize::MAX] {
		// Detect if we've read too many bytes.
		if multiplier == usize::MAX {
			return Err(DeserializeError::MalformedLength);
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

pub fn put_var(dst: &mut impl BufMut, mut value: usize) -> Result<(), SerializeError> {
	if value > 268_435_455 {
		return Err(SerializeError);
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
