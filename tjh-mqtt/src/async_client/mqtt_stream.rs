use super::packet_stream::PacketStream;
use crate::Packet;
use tokio::{
	io::{AsyncRead, AsyncWrite},
	net::TcpStream,
};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send {}
impl AsyncReadWrite for TcpStream {}
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

	pub async fn write_packet(&mut self, packet: &Packet) -> crate::Result<()> {
		self.stream.write_packet(packet).await
	}

	pub async fn read_packet(&mut self) -> crate::Result<Option<Packet>> {
		self.stream.read_packet().await
	}
}
