pub(crate) mod client;
mod mqtt_stream;
mod packet_stream;
mod task;

use super::{
	holdoff::HoldOff, ClientConfiguration, ClientState, Message, StateError, TcpConfiguration,
};
use crate::{clients::tokio::mqtt_stream::MqttStream, misc::Credentials, packets, FilterBuf, QoS};
use std::{ops::ControlFlow::Break, time::Duration};
use tokio::{
	net::TcpStream,
	sync::{mpsc, oneshot},
	task::JoinHandle,
};

pub use client::{Client, Subscription};

pub type PublishTx = mpsc::Sender<Message>;
pub type PublishRx = mpsc::Receiver<Message>;

type Command = super::command::Command<
	mpsc::Sender<Message>,
	oneshot::Sender<()>,
	oneshot::Sender<Vec<(FilterBuf, QoS)>>,
	oneshot::Sender<()>,
>;
type CommandTx = mpsc::UnboundedSender<Box<Command>>;
type CommandRx = mpsc::UnboundedReceiver<Box<Command>>;

pub fn tcp_client(
	transport: TcpConfiguration,
	configuration: ClientConfiguration,
) -> (client::Client, JoinHandle<crate::Result<()>>) {
	let (tx, mut rx) = mpsc::unbounded_channel();

	let credentials = if let Some(username) = configuration.username.as_ref() {
		Some(Credentials {
			username,
			password: configuration.password.as_deref(),
		})
	} else {
		None
	};

	let keep_alive = Duration::from_secs(configuration.keep_alive.into());

	// Construct a Connect packet.
	let connect = packets::Connect {
		client_id: &configuration.client_id,
		keep_alive: configuration.keep_alive,
		clean_session: configuration.clean_session,
		credentials,
		will: configuration.will,
		..Default::default()
	};
	tracing::debug!(?connect);

	let mut state = ClientState::new(&connect);

	let handle = tokio::spawn(async move {
		state.keep_alive = keep_alive;

		let mut reconnect_delay = HoldOff::new(Duration::from_millis(75)..keep_alive);
		loop {
			reconnect_delay
				.wait_and_increase_with_async(|delay| delay * 2)
				.await;

			// Open the the connection to the broker.
			let Ok(stream) = TcpStream::connect((transport.host.as_str(), transport.port)).await
			else {
				continue;
			};

			if transport.linger {
				stream.set_linger(Some(keep_alive))?;
			}

			let mut connection = MqttStream::new(Box::new(stream), 8 * 1024);

			let Ok(connack) = task::wait_for_connack(&mut state, &mut connection).await else {
				continue;
			};

			reconnect_delay.reset();

			if let Ok(Break(_)) = task::connected_task(
				&mut state,
				&mut rx,
				&mut connection,
				connack.session_present,
			)
			.await
			{
				tracing::info!("break from client_task");
				break Ok(());
			}
		}
	});

	(client::Client::new(tx), handle)
}

#[cfg(feature = "tls")]
mod tls {
	use std::sync::Arc;
	use tokio_rustls::rustls::{ClientConfig, RootCertStore};

	pub fn configure_tls() -> Arc<ClientConfig> {
		let mut root_cert_store = RootCertStore::empty();
		root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

		Arc::new(
			ClientConfig::builder()
				.with_root_certificates(root_cert_store)
				.with_no_client_auth(),
		)
	}
}