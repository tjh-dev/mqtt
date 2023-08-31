use crate::{command::CommandRx, connection::Connection, state::State, Options};
use mqtt_core::{Connect, Packet};
use std::time::Duration;
use tokio::{
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
	let connect = Packet::Connect(Connect {
		client_id: options.client_id.clone(),
		keep_alive: options.keep_alive,
		clean_session: options.clean_session,
		..Default::default()
	});

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
		let mut connection = Connection::new(stream);

		// Send the Connect packet.
		connection.write_packet(&connect).await?;

		match wait_for_connack(
			&mut connection,
			time::Duration::from_secs(options.keep_alive as u64),
		)
		.await?
		{
			ConnAckResult::Continue { session_present } => {
				tracing::debug!("connected! session_present = {session_present}");
				holdoff.reset();
			}
			ConnAckResult::Timeout => {
				tracing::error!("timeout waiting for ConnAck");
				continue;
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
					let Some(response) = client_state.process_client_command(command) else {
						continue
					};

					if connection.write_packet(&response).await.is_err() {
						break;
					}
				}
				Ok(packet) = connection.read_packet() => {
					tracing::trace!(?packet);
					let Some(packet) = packet else {
						break
					};

					let response = match client_state.process_incoming_packet(packet) {
						Ok(Some(packet)) => packet,
						Ok(None) => {
							tracing::debug!("processed packet, no response to send");
							continue;
						}
						Err(error) => {
							tracing::error!("{error:?}");
							break;
						}
					};

					if connection.write_packet(&response).await.is_err() {
						break;
					}
				}
				_ = keep_alive.tick() => {
					pingreq_sent.replace(Instant::now());
					connection.write_packet(&Packet::PingReq).await?;
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
	connection: &mut Connection,
	timeout: time::Duration,
) -> crate::Result<ConnAckResult> {
	let mut timeout = time::interval_at(Instant::now() + timeout, timeout);
	loop {
		tokio::select! {
			Ok(packet) = connection.read_packet() => {
				match packet {
					Some(Packet::ConnAck { session_present, code }) => {
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
