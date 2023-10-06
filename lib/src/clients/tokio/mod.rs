mod client;
mod mqtt_stream;
mod packet_stream;
mod task;

use super::{holdoff::HoldOff, ClientState, Message, StateError, TcpConnectOptions};
use crate::{
	clients::tokio::mqtt_stream::MqttStream,
	misc::{Credentials, Will},
	packets, FilterBuf, QoS,
};
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

#[derive(Debug)]
pub struct ClientConfiguration<'a> {
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,
	pub will: Option<Will<'a>>,
}

impl<'a> Default for ClientConfiguration<'a> {
	fn default() -> Self {
		Self {
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
			will: Default::default(),
		}
	}
}

pub fn tcp_client(
	connect_options: impl Into<TcpConnectOptions>,
	options: ClientConfiguration,
) -> (client::Client, JoinHandle<crate::Result<()>>) {
	let (tx, mut rx) = mpsc::unbounded_channel();

	let connect_options: TcpConnectOptions = connect_options.into();
	let credentials = if let Some(username) = connect_options.user.as_ref() {
		Some(Credentials {
			username,
			password: connect_options.password.as_deref(),
		})
	} else {
		None
	};

	let keep_alive = Duration::from_secs(options.keep_alive.into());

	// Construct a Connect packet.
	let connect = packets::Connect {
		client_id: &options.client_id,
		keep_alive: options.keep_alive,
		clean_session: options.clean_session,
		credentials,
		will: options.will,
		..Default::default()
	};

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
				TcpStream::connect((connect_options.host.as_str(), connect_options.port)).await
			else {
				continue;
			};

			if connect_options.linger {
				stream.set_linger(Some(keep_alive))?;
			}

			#[cfg(feature = "tls")]
			let mut connection = match connect_options.tls {
				true => {
					let connector = tls::configure_tls();
					let dnsname = connect_options.host.as_str().try_into()?;
					let stream = connector.connect(dnsname, stream).await?;
					MqttStream::new(Box::new(stream), 8 * 1024)
				}
				false => MqttStream::new(Box::new(stream), 8 * 1024),
			};

			#[cfg(not(feature = "tls"))]
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
	use tokio_rustls::{
		rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore},
		TlsConnector,
	};

	pub fn configure_tls() -> TlsConnector {
		let mut root_cert_store = RootCertStore::empty();
		root_cert_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
			OwnedTrustAnchor::from_subject_spki_name_constraints(
				ta.subject,
				ta.spki,
				ta.name_constraints,
			)
		}));

		let config = Arc::new(
			ClientConfig::builder()
				.with_safe_defaults()
				.with_root_certificates(root_cert_store)
				.with_no_client_auth(),
		);

		TlsConnector::from(config)
	}
}
