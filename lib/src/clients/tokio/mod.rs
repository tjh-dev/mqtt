pub(crate) mod client;
mod mqtt_stream;
mod packet_stream;
mod task;

use super::{
	holdoff::HoldOff, ClientConfiguration, ClientOptions, ClientState, Message, StateError,
	TcpConfiguration, Transport,
};
use crate::{clients::tokio::mqtt_stream::MqttStream, packets, FilterBuf, QoS};
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

pub fn create_client(
	options: ClientOptions,
) -> (client::Client, ::tokio::task::JoinHandle<crate::Result<()>>) {
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

pub fn tcp_client(
	transport: TcpConfiguration,
	configuration: ClientConfiguration,
) -> (client::Client, JoinHandle<crate::Result<()>>) {
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

#[cfg(all(feature = "tls", feature = "tokio-client"))]
mod tls {
	use super::client;
	use crate::{
		clients::{
			holdoff::HoldOff,
			tokio::{mqtt_stream::MqttStream, task},
			ClientConfiguration, ClientState, TcpConfiguration,
		},
		packets,
	};
	use std::{ops::ControlFlow::Break, sync::Arc, time::Duration};
	use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle};
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
	) -> (client::Client, JoinHandle<crate::Result<()>>) {
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

		let handle = tokio::spawn(async move {
			state.keep_alive = keep_alive;

			let mut reconnect_delay = HoldOff::new(Duration::from_millis(75)..keep_alive);
			loop {
				reconnect_delay
					.wait_and_increase_with_async(|delay| delay * 2)
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
