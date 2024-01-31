pub(crate) mod client;
mod holdoff;
mod mqtt_stream;
mod packet_stream;
mod state;
mod subscription;
mod task;

use mqtt_client::{
	client_configuration::ClientConfiguration,
	client_options::ClientOptions,
	transport::{TcpConfiguration, Transport},
};
use mqtt_protocol::{packets, FilterBuf, Message, QoS};
use std::{future::Future, ops::ControlFlow::Break, pin::Pin, time::Duration};
use tokio::{
	net::TcpStream,
	sync::{mpsc, oneshot},
};

pub use client::{Client, ClientError};
pub use subscription::Subscription;

use crate::{holdoff::HoldOff, mqtt_stream::MqttStream, state::ClientState};

pub type PublishTx = mpsc::Sender<Message>;
pub type PublishRx = mpsc::Receiver<Message>;

type Command = mqtt_client::command::Command<
	mpsc::Sender<Message>,
	oneshot::Sender<()>,
	oneshot::Sender<Vec<(FilterBuf, QoS)>>,
	oneshot::Sender<()>,
>;
type CommandTx = mpsc::UnboundedSender<Box<Command>>;
type CommandRx = mpsc::UnboundedReceiver<Box<Command>>;

pub fn create_client(options: ClientOptions) -> (client::Client, Task) {
	let ClientOptions {
		transport,
		configuration,
	} = options;
	match transport {
		Transport::Tcp(transport) => tcp_client(transport, configuration),
		#[cfg(feature = "tls")]
		Transport::Tls(transport) => tls::tls_client(transport, configuration),
		Transport::Socket(_) => unimplemented!(),
	}
}

type Task = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>>;

pub fn tcp_client(
	transport: TcpConfiguration,
	configuration: ClientConfiguration,
) -> (client::Client, Task) {
	let (command_tx, mut command_rx) = mpsc::unbounded_channel();

	let keep_alive = Duration::from_secs(configuration.keep_alive.into());
	let credentials = configuration.credentials();
	let will = configuration.will.clone();

	// Construct a Connect packet.
	let connect = packets::Connect {
		client_id: &configuration.client_id,
		keep_alive: configuration.keep_alive,
		clean_session: configuration.clean_session,
		credentials,
		will,
		..Default::default()
	};
	tracing::debug!(?connect);

	let mut state = ClientState::new(&connect);

	let handle = Box::pin(async move {
		state.keep_alive = keep_alive;

		let mut reconnect_delay = HoldOff::new(Duration::from_millis(75)..keep_alive);
		loop {
			reconnect_delay
				.wait_and_increase_with(|delay| delay * 2)
				.await;

			// Open the the connection to the broker.
			let Ok(stream) = TcpStream::connect((transport.host.as_str(), transport.port)).await
			else {
				tracing::error!("error connecting to host, retrying ...");
				continue;
			};

			if transport.linger {
				stream.set_linger(Some(keep_alive))?;
			}

			let mut connection = MqttStream::new(Box::new(stream), 8 * 1024);
			let Ok(connack) = task::wait_for_connack(&mut state, &mut connection).await else {
				tracing::warn!("timeout waiting for ConnAck, restarting connection ...");
				continue;
			};

			// We have successfully connected, reset the hold-off delay.
			reconnect_delay.reset();
			if let Ok(Break(_)) = task::connected_task(
				&mut state,
				&mut command_rx,
				&mut connection,
				connack.session_present,
			)
			.await
			{
				tracing::info!("client shutdown");
				break Ok(());
			}
		}
	});

	(client::Client::new(command_tx), handle)
}

#[cfg(feature = "tls")]
mod tls {
	use crate::{holdoff::HoldOff, mqtt_stream::MqttStream, state::ClientState, task, Task};

	use super::client;
	use mqtt_client::{client_configuration::ClientConfiguration, transport::TcpConfiguration};
	use mqtt_protocol::packets;
	use std::{ops::ControlFlow::Break, sync::Arc, time::Duration};
	use tokio::{net::TcpStream, sync::mpsc};
	use tokio_rustls::{
		rustls::{pki_types::ServerName, ClientConfig, RootCertStore},
		TlsConnector,
	};

	pub fn configure_tls() -> Arc<ClientConfig> {
		let mut root_cert_store = RootCertStore::empty();
		root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

		Arc::new(
			ClientConfig::builder()
				.with_root_certificates(root_cert_store)
				.with_no_client_auth(),
		)
	}

	pub fn tls_client(
		transport: TcpConfiguration,
		configuration: ClientConfiguration,
	) -> (client::Client, Task) {
		let (tx, mut rx) = mpsc::unbounded_channel();

		let keep_alive = Duration::from_secs(configuration.keep_alive.into());
		let credentials = configuration.credentials();
		let will = configuration.will.clone();

		// Construct a Connect packet.
		let connect = packets::Connect {
			client_id: &configuration.client_id,
			keep_alive: configuration.keep_alive,
			clean_session: configuration.clean_session,
			credentials,
			will,
			..Default::default()
		};
		tracing::debug!(?connect);

		let mut state = ClientState::new(&connect);

		let handle = Box::pin(async move {
			state.keep_alive = keep_alive;

			let mut reconnect_delay = HoldOff::new(Duration::from_millis(75)..keep_alive);
			loop {
				reconnect_delay
					.wait_and_increase_with(|delay| delay * 2)
					.await;

				// Open the the connection to the broker.
				let Ok(stream) =
					TcpStream::connect((transport.host.as_str(), transport.port)).await
				else {
					tracing::error!("error connecting to host, retrying ...");
					continue;
				};

				if transport.linger {
					stream.set_linger(Some(keep_alive))?;
				}

				// Start the TLS connection.
				let config = configure_tls();
				let connector = TlsConnector::from(Arc::clone(&config));
				let dns_name = ServerName::try_from(transport.host.clone())?;
				let stream = connector.connect(dns_name, stream).await?;

				let mut connection = MqttStream::new(Box::new(stream), 8 * 1024);
				let Ok(connack) = task::wait_for_connack(&mut state, &mut connection).await else {
					tracing::warn!("timeout waiting for ConnAck, restarting connection ...");
					continue;
				};

				// We have successfully connected, reset the hold-off delay.
				reconnect_delay.reset();
				if let Ok(Break(_)) = task::connected_task(
					&mut state,
					&mut rx,
					&mut connection,
					connack.session_present,
				)
				.await
				{
					tracing::info!("client shutdown");
					break Ok(());
				}
			}
		});

		(client::Client::new(tx), handle)
	}
}
