use crate::packets::{Frame, ParseError};
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

	pub fn parse_frame<'a>(&'a mut self) -> Result<Option<Frame>, ParseError> {
		use ParseError::Incomplete;

		let mut buf = Cursor::new(&self.buffer[..]);
		match Frame::check(&mut buf) {
			Ok(extent) => {
				let bytes = self.buffer.split_to(extent).freeze();
				Ok(Some(Frame::parse(bytes)?))
			}
			Err(Incomplete) => Ok(None),
			Err(error) => Err(error),
		}
	}
}

impl<T: AsyncRead + Unpin> PacketStream<T> {
	pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
		loop {
			// Attempt to parse a packet from the buffered data.
			if let Some(packet) = self.parse_frame()? {
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
	pub async fn write(&mut self, mut buffer: impl Buf) -> crate::Result<()> {
		tracing::trace!("writing {} bytes to stream", buffer.remaining());
		self.stream.write_all_buf(&mut buffer).await?;
		Ok(())
	}
}
