use super::packet_stream::PacketStream;
use crate::Frame;
use bytes::Buf;
use tokio::{
	io::{AsyncRead, AsyncWrite},
	net::TcpStream,
};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send {}
impl AsyncReadWrite for TcpStream {}

#[cfg(feature = "tls")]
impl AsyncReadWrite for tokio_rustls::client::TlsStream<TcpStream> {}

pub struct MqttStream {
	stream: PacketStream<Box<dyn AsyncReadWrite + Unpin>>,
}

impl MqttStream {
	pub fn new(stream: Box<dyn AsyncReadWrite + Unpin>, len: usize) -> Self {
		Self {
			stream: PacketStream::new(stream, len),
		}
	}

	pub async fn write(&mut self, buffer: impl Buf) -> crate::Result<()> {
		self.stream.write(buffer).await
	}

	pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
		self.stream.read_frame().await
	}
}
