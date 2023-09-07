use crate::async_client::{
	command::{Command, CommandRx},
	packetstream::PacketStream,
	state::State,
	Options,
};
use crate::{
	packets::{ConnAck, Connect, Disconnect, PingReq},
	Packet,
};
use std::time::Duration;
use tokio::{
	io::{AsyncRead, AsyncWrite},
	net::TcpStream,
	time::{self, Instant},
};

mod holdoff;
use self::holdoff::HoldOff;

const HOLDOFF_MIN: Duration = Duration::from_millis(50);

trait AsyncReadWrite: AsyncRead + AsyncWrite + Send {}
impl AsyncReadWrite for TcpStream {}
impl AsyncReadWrite for tokio_rustls::client::TlsStream<TcpStream> {}

struct MqttStream {
	stream: PacketStream<Box<dyn AsyncReadWrite + Unpin>>,
}

impl MqttStream {
	fn new(stream: Box<dyn AsyncReadWrite + Unpin>, len: usize) -> Self {
		Self {
			stream: PacketStream::new(stream, len),
		}
	}

	async fn write_packet(&mut self, packet: &Packet) -> crate::Result<()> {
		self.stream.write_packet(packet).await
	}

	async fn read_packet(&mut self) -> crate::Result<Option<Packet>> {
		self.stream.read_packet().await
	}
}

#[tracing::instrument(skip(options, rx), ret, err)]
pub async fn client_task(options: Options, mut rx: CommandRx) -> crate::Result<()> {
	//
	// Build the Connect packet.
	let connect: Packet = Connect {
		client_id: options.client_id.clone(),
		keep_alive: options.keep_alive,
		clean_session: options.clean_session,
		..Default::default()
	}
	.into();

	let keep_alive_duration = Duration::from_secs(options.keep_alive as u64);

	let mut client_state = State::default();
	let mut holdoff = HoldOff::new(HOLDOFF_MIN..keep_alive_duration);

	loop {
		// Use a hold-off when reconnecting. On the first connection attempt, this
		// won't wait at all.
		holdoff.wait_and_increase_with(|delay| delay * 2).await;
		tracing::debug!("{holdoff:?}");

		// Open the the connection to the broker.
		let Ok(stream) = TcpStream::connect((options.host.as_str(), options.port)).await else {
			continue;
		};
		stream.set_linger(Some(keep_alive_duration))?;

		let mut connection = match options.tls {
			#[cfg(feature = "tls")]
			true => {
				tracing::info!("Connecting with TLS");
				use std::sync::Arc;
				use tokio_rustls::{rustls::ServerName, TlsConnector};

				let config = tls::configure_tls();
				let connector = TlsConnector::from(Arc::clone(&config));
				let dnsname = ServerName::try_from(options.host.as_str()).unwrap();

				let stream = connector.connect(dnsname, stream).await?;
				MqttStream::new(Box::new(stream), 8 * 1024)
			}
			#[cfg(not(feature = "tls"))]
			true => {
				panic!("TLS not supported");
			}
			false => MqttStream::new(Box::new(stream), 8 * 1024),
		};

		// Send the Connect packet.
		connection.write_packet(&connect).await?;
		let mut resubscribe_packet =
			match wait_for_connack(&mut connection, keep_alive_duration).await? {
				ConnAckResult::Continue { session_present } => {
					tracing::debug!("connected! session_present = {session_present}");
					// if let Some((packet, response_rx)) = client_state.connected(session_present) {
					// 	connection.write_packet(&packet).await?;
					// 	response_rx.await?;
					// }
					holdoff.reset();
					client_state.connected(session_present)
				}
				ConnAckResult::Timeout => {
					tracing::error!("timeout waiting for ConnAck");
					continue;
				}
			};

		let mut pingreq_sent: Option<Instant> = None;

		// Discard the first tick from the keep-alive interval.
		let mut keep_alive = time::interval(keep_alive_duration);
		let _ = keep_alive.tick().await;

		loop {
			if let Some((resubscribe_packet, subscribe_response)) = resubscribe_packet.take() {
				connection.write_packet(&resubscribe_packet).await?;
				let Ok(Some(Packet::SubAck(suback))) = connection.read_packet().await else {
					tracing::error!("failed to read SubAck");
					holdoff.increase_with(|delay| delay * 4);
					break
				};
				if client_state
					.process_incoming_packet(Packet::SubAck(suback))
					.await
					.is_err()
				{
					return Ok(());
				};
				subscribe_response.await?;
			}

			tokio::select! {
				Some(command) = rx.recv() => {
					tracing::debug!(?command);

					if let Command::Shutdown = command {
						// TODO: This shutdown process could be better.
						connection.write_packet(&Disconnect.into()).await?;
						return Ok(())
					}

					if let Some(packet) = client_state.process_client_command(command) {
						if connection.write_packet(&packet).await.is_err() {
							break
						}
					}
				}
				Ok(packet) = connection.read_packet() => {
					let Some(packet) = packet else {
						tracing::warn!("connection reset by peer");
						break
					};

					tracing::trace!(packet = ?packet, "received from Server");

					match client_state.process_incoming_packet(packet).await {
						Err(error) => {
							tracing::error!("{error:?}");
							break
						}
						Ok(Some(packet)) => {
							if connection.write_packet(&packet).await.is_err() {
								break
							}
						}
						Ok(None) => {}
					};
				}
				_ = keep_alive.tick() => {
					tracing::debug!("{client_state:#?}");
					pingreq_sent.replace(Instant::now());
					connection.write_packet(&PingReq.into()).await?;
				}
				else => {
					tracing::warn!("ending client task");
					return Ok(())
				}
			}
		}
	}
}

enum ConnAckResult {
	Continue { session_present: bool },
	Timeout,
}

async fn wait_for_connack(
	connection: &mut MqttStream,
	timeout: time::Duration,
) -> crate::Result<ConnAckResult> {
	let mut timeout = time::interval_at(Instant::now() + timeout, timeout);
	loop {
		tokio::select! {
			Ok(packet) = connection.read_packet() => {
				match packet {
					Some(Packet::ConnAck(ConnAck { session_present, code })) => {
						if code == 0 {
							break Ok(ConnAckResult::Continue { session_present })
						} else {
							break Err("connect error, rejected by peer".into())
						}
					}
					Some(_) => break Err("protocol error".into()),
					None => {
						break Ok(ConnAckResult::Timeout)//Err("connection reset by peer".into())
					}
				}
			}
			_ = timeout.tick() => {
				break Ok(ConnAckResult::Timeout)
			}
		}
	}
}

#[cfg(feature = "tls")]
mod tls {
	use std::sync::Arc;
	use tokio_rustls::rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore};

	pub fn configure_tls() -> Arc<ClientConfig> {
		let mut root_cert_store = RootCertStore::empty();
		root_cert_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
			OwnedTrustAnchor::from_subject_spki_name_constraints(
				ta.subject,
				ta.spki,
				ta.name_constraints,
			)
		}));

		Arc::new(
			ClientConfig::builder()
				.with_safe_defaults()
				.with_root_certificates(root_cert_store)
				.with_no_client_auth(),
		)
	}
}
