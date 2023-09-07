use crate::Packet;
use bytes::{Buf, BytesMut};
use std::io::Cursor;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct Connection<T> {
	stream: T,
	buffer: BytesMut,
}

impl<T> Connection<T> {
	pub fn new(stream: T, len: usize) -> Self {
		Self {
			stream,
			buffer: BytesMut::with_capacity(len),
		}
	}

	fn parse_packet(&mut self) -> Result<Option<Packet>, crate::PacketError> {
		use crate::PacketError::Incomplete;

		let mut buf = Cursor::new(&self.buffer[..]);
		match Packet::check(&mut buf) {
			Ok(_) => {
				let len = buf.position() as usize;
				buf.set_position(0);

				let packet = Packet::parse(&mut buf)?;
				self.buffer.advance(len);
				Ok(Some(packet))
			}
			Err(Incomplete) => Ok(None),
			Err(error) => Err(error),
		}
	}
}

impl<T: AsyncRead + Unpin> Connection<T> {
	/// Read a single [`Packet`] from the underlying stream.
	pub async fn read_packet(&mut self) -> crate::Result<Option<Packet>> {
		loop {
			// Attempt to parse a packet from the buffered data.
			if let Some(packet) = self.parse_packet()? {
				tracing::trace!("incoming {packet:?}");
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

impl<T: AsyncWrite + Unpin> Connection<T> {
	pub async fn write_packet(&mut self, packet: &Packet) -> crate::Result<()> {
		let mut buf = BytesMut::new();
		packet.serialize_to_bytes(&mut buf).unwrap();
		tracing::trace!("serialized to {:02x?}", &buf[..]);

		self.stream.write_all(&buf).await?;
		self.stream.flush().await?;

		tracing::debug!("wrote {packet:?} to stream");
		Ok(())
	}

	pub async fn flush(&mut self) -> crate::Result<()> {
		self.stream.flush().await?;
		Ok(())
	}
}
