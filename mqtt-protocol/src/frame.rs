use crate::{bytes_reader, packets};

#[derive(Debug)]
pub struct Frame {
	pub header: u8,
	pub payload: bytes::Bytes,
}

impl Frame {
	/// Checks whether a complete packet frame can be deserialized from the
	/// cursor.
	pub fn check(cursor: &mut bytes_reader::Cursor) -> Result<usize, packets::DeserializeError> {
		use packets::DeserializeError::InvalidHeader;
		let header = cursor.take_u8()?;
		if header == 0 || header == 0xf0 {
			return Err(InvalidHeader);
		}

		let length = cursor.take_var()?;
		cursor.take_slice(length)?;

		Ok(cursor.position())
	}

	/// Deserializes a complete packet frame from the bytes.
	///
	/// `buffer` should contain *exactly* the length of the complete frame.
	pub fn parse(buffer: bytes::Bytes) -> Result<Self, packets::DeserializeError> {
		let mut reader = bytes_reader::BytesReader::new(buffer);

		let header = reader.take_u8()?;
		let length = reader.take_var()?;
		assert_eq!(length, reader.remaining());

		// Assume the payload is the reset of the buffer.
		let payload = reader.take_inner();
		Ok(Self { header, payload })
	}

	pub fn deserialize_packet<'a, T>(&'a self) -> Result<T, packets::DeserializeError>
	where
		T: packets::DeserializePacket<'a>,
	{
		T::deserialize_from(self)
	}
}
