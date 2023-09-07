use crate::{packets::ParseError, Packet};
use bytes::{Buf, BytesMut};
use std::io::Cursor;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct PacketStream<T> {
	stream: T,
	buffer: BytesMut,
}

impl<T> PacketStream<T> {
	/// Create a new `PacketStream` with the given stream and buffer length.
	pub fn new(stream: T, len: usize) -> Self {
		Self {
			stream,
			buffer: BytesMut::with_capacity(len),
		}
	}

	/// Attempt to parse a single [`Packet`] from the buffered data.
	fn parse_packet(&mut self) -> Result<Option<Packet>, ParseError> {
		use ParseError::Incomplete;

		let mut buf = Cursor::new(&self.buffer[..]);
		match Packet::check(&mut buf) {
			Ok(extent) => {
				// Rewind the cursor and parse the packet.
				buf.set_position(0);
				let packet = Packet::parse(&mut buf)?;

				// Advance the read buffer.
				self.buffer.advance(extent as usize);
				Ok(Some(packet))
			}
			Err(Incomplete) => Ok(None),
			Err(error) => Err(error),
		}
	}
}

impl<T: AsyncRead + Unpin> PacketStream<T> {
	/// Read a single [`Packet`] from the underlying stream.
	pub async fn read_packet(&mut self) -> crate::Result<Option<Packet>> {
		loop {
			// Attempt to parse a packet from the buffered data.
			if let Some(packet) = self.parse_packet()? {
				return Ok(Some(packet));
			}

			// There is not enough buffered data to read a packet. Attempt
			// to read more.
			//
			if 0 == self.stream.read_buf(&mut self.buffer).await? {
				// If the buffer is empty the connection was shutdown cleanly,
				// otherwise the peer closed the socket while sending a packet.
				//
				if self.buffer.is_empty() {
					return Ok(None);
				} else {
					return Err("connection reset by peer".into());
				}
			}
		}
	}
}

impl<T: AsyncWrite + Unpin> PacketStream<T> {
	/// Write a single [`Packet`] to the underlying stream.
	pub async fn write_packet(&mut self, packet: &Packet) -> crate::Result<()> {
		let mut buf = BytesMut::new();
		packet.serialize_to_bytes(&mut buf).unwrap();

		self.stream.write_all(&buf).await?;
		self.stream.flush().await?;

		tracing::debug!("wrote {packet:?} to stream");
		Ok(())
	}

	/// Flush the underlying stream.
	pub async fn flush(&mut self) -> crate::Result<()> {
		self.stream.flush().await?;
		Ok(())
	}
}
