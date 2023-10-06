use bytes::BufMut;
use std::mem;

#[derive(Debug)]
pub struct SerializeError;

pub fn require_mut(dst: &impl BufMut, len: usize) -> Result<(), SerializeError> {
	if dst.remaining_mut() < len {
		Err(SerializeError)
	} else {
		Ok(())
	}
}

pub fn put_u8(dst: &mut impl BufMut, val: u8) -> Result<(), SerializeError> {
	require_mut(dst, mem::size_of::<u8>())?;
	dst.put_u8(val);
	Ok(())
}

pub fn put_u16(dst: &mut impl BufMut, val: u16) -> Result<(), SerializeError> {
	require_mut(dst, mem::size_of::<u16>())?;
	dst.put_u16(val);
	Ok(())
}

pub fn put_slice(dst: &mut impl BufMut, slice: &[u8]) -> Result<(), SerializeError> {
	require_mut(dst, slice.len())?;
	dst.put_slice(slice);
	Ok(())
}

pub fn put_str(dst: &mut impl BufMut, s: &str) -> Result<(), SerializeError> {
	if s.len() > u16::MAX as usize {
		return Err(SerializeError);
	}
	put_u16(dst, s.len() as u16)?;
	put_slice(dst, s.as_bytes())
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
