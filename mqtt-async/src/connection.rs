use bytes::{Buf, BytesMut};
use mqtt_core::Packet;
use std::io::Cursor;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt, BufWriter},
	net::TcpStream,
};

#[derive(Debug)]
pub struct Connection {
	stream: BufWriter<TcpStream>,
	buffer: BytesMut,
}

impl Connection {
	pub fn new(socket: TcpStream) -> Self {
		Self {
			stream: BufWriter::new(socket),
			buffer: BytesMut::with_capacity(8 * 1024),
		}
	}

	/// Read a single [`Packet`] from the underlying stream.
	#[tracing::instrument(skip(self), err)]
	pub async fn read_packet(&mut self) -> mqtt_core::Result<Option<Packet>> {
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
					tracing::warn!("buffer empty");
					return Ok(None);
				} else {
					return Err("connection reset by peer".into());
				}
			}
		}
	}

	pub fn parse_packet(&mut self) -> mqtt_core::Result<Option<Packet>> {
		use mqtt_core::PacketError::Incomplete;

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
			Err(error) => Err(error.into()),
		}
	}

	#[tracing::instrument(skip(self), err)]
	pub async fn write_packet(&mut self, packet: &Packet) -> mqtt_core::Result<()> {
		let mut buf = BytesMut::new();
		packet.serialize_to_bytes(&mut buf).unwrap();
		tracing::trace!("serialized to {:02x?}", &buf[..]);

		self.stream.write_all(&buf).await?;
		self.stream.flush().await?;

		tracing::debug!("wrote packet to stream");
		Ok(())
	}
}
