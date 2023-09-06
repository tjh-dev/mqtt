use crate::{
	command::{Command, CommandRx},
	connection::Connection,
	state::State,
	Options,
};
use mqtt_core::{ConnAck, Connect, Disconnect, Packet, PingReq};
use std::time::Duration;
use tokio::{
	io::{AsyncRead, AsyncWrite},
	net::TcpStream,
	time::{self, Instant},
};

mod holdoff;
use self::holdoff::HoldOff;

const HOLDOFF_MIN: Duration = Duration::from_millis(50);

#[tracing::instrument(skip(options, rx), ret, err)]
pub async fn client_task(options: Options, mut rx: CommandRx) -> mqtt_core::Result<()> {
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
		holdoff.wait().await;
		holdoff.increase_with(|delay| delay * 2);
		tracing::debug!("{holdoff:?}");

		// Open the the connection to the broker.
		let Ok(stream) = TcpStream::connect((options.host.as_str(), options.port)).await else {
			continue;
		};
		stream.set_linger(Some(keep_alive_duration))?;

		let should_continue = match options.tls {
			#[cfg(feature = "tls")]
			true => {
				tracing::info!("Connecting with TLS");
				use std::sync::Arc;
				use tokio_rustls::{rustls::ServerName, TlsConnector};

				let config = tls::configure_tls();
				let connector = TlsConnector::from(Arc::clone(&config));
				let dnsname = ServerName::try_from(options.host.as_str()).unwrap();

				let stream = connector.connect(dnsname, stream).await?;
				let mut connection = Connection::new(stream, 8 * 1024);

				connected_task(
					&mut client_state,
					&mut holdoff,
					&connect,
					&mut connection,
					keep_alive_duration,
					&mut rx,
				)
				.await?
			}
			#[cfg(not(feature = "tls"))]
			true => {
				panic!("TLS not supported");
			}
			false => {
				let mut connection = Connection::new(stream, 8 * 1024);
				connected_task(
					&mut client_state,
					&mut holdoff,
					&connect,
					&mut connection,
					keep_alive_duration,
					&mut rx,
				)
				.await?
			}
		};

		if !should_continue {
			break Ok(());
		}
	}
}

async fn connected_task<T: AsyncRead + AsyncWrite + Unpin>(
	client_state: &mut State,
	holdoff: &mut HoldOff,
	connect: &Packet,
	connection: &mut Connection<T>,
	keep_alive_duration: time::Duration,
	rx: &mut CommandRx,
) -> mqtt_core::Result<bool> {
	// Send the Connect packet.
	connection.write_packet(connect).await?;

	match wait_for_connack(connection, keep_alive_duration).await? {
		ConnAckResult::Continue { session_present } => {
			tracing::debug!("connected! session_present = {session_present}");
			holdoff.reset();
		}
		ConnAckResult::Timeout => {
			tracing::error!("timeout waiting for ConnAck");
			return Ok(false);
		}
	}

	let mut pingreq_sent: Option<Instant> = None;

	// Discard the first tick from the keep-alive interval.
	let mut keep_alive = time::interval(keep_alive_duration);
	let _ = keep_alive.tick().await;

	// let mut command_queue = VecDeque::new();

	loop {
		tokio::select! {
			Some(command) = rx.recv() => {
				tracing::debug!(?command);

				if let Command::Shutdown = command {
					// TODO: This shutdown process could be better.
					connection.flush().await?;
					connection.write_packet(&Disconnect.into()).await?;
					connection.flush().await?;
					return Ok(false)
				}

				let Some(response) = client_state.process_client_command(command) else {
					continue
				};

				if connection.write_packet(&response).await.is_err() {
					break Ok(true);
				}
			}
			Ok(packet) = connection.read_packet() => {
				tracing::trace!(?packet);
				let Some(packet) = packet else {
					break Ok(true)
				};

				tracing::debug!("recevied {packet:?}");
				let response = match client_state.process_incoming_packet(packet) {
					Ok(Some(packet)) => packet,
					Ok(None) => {
						tracing::debug!("processed packet, no response to send");
						continue;
					}
					Err(error) => {
						tracing::error!("{error:?}");
						break Ok(true);
					}
				};

				if connection.write_packet(&response).await.is_err() {
					break Ok(true);
				}
			}
			_ = keep_alive.tick() => {
				tracing::debug!("{client_state:#?}");
				pingreq_sent.replace(Instant::now());
				connection.write_packet(&PingReq.into()).await?;
			}
			else => {
				tracing::warn!("ending client task");
				return Ok(false)
			}
		}
	}
}

enum ConnAckResult {
	Continue { session_present: bool },
	Timeout,
}

async fn wait_for_connack<T: AsyncRead + Unpin>(
	connection: &mut Connection<T>,
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
