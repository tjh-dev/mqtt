mod client;
mod mqtt_stream;
mod packet_stream;
mod task;

use super::{holdoff::HoldOff, ClientState, Message, StateError};
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
pub struct Options<'a> {
	pub host: String,
	pub port: u16,
	pub tls: bool,
	pub keep_alive: u16,
	pub clean_session: bool,
	pub client_id: String,
	pub credentials: Option<Credentials<'a>>,
	pub will: Option<Will<'a>>,
}

impl<'a> Default for Options<'a> {
	fn default() -> Self {
		Self {
			host: Default::default(),
			port: 1883,
			tls: false,
			keep_alive: 60,
			clean_session: true,
			client_id: Default::default(),
			credentials: Default::default(),
			will: Default::default(),
		}
	}
}

impl<'a, H: AsRef<str>> From<(H, u16)> for Options<'a> {
	#[inline]
	fn from(value: (H, u16)) -> Self {
		let (host, port) = value;
		Self {
			host: host.as_ref().into(),
			port,
			..Default::default()
		}
	}
}

pub fn tcp_client<'o>(
	options: impl Into<Options<'o>>,
) -> (client::Client, JoinHandle<crate::Result<()>>) {
	let (tx, mut rx) = mpsc::unbounded_channel();
	let options = options.into();

	let keep_alive = Duration::from_secs(options.keep_alive.into());

	// Construct a Connect packet.
	let connect = packets::Connect {
		client_id: &options.client_id,
		keep_alive: options.keep_alive,
		clean_session: options.clean_session,
		credentials: options.credentials,
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
			let Ok(stream) = TcpStream::connect((options.host.as_str(), options.port)).await else {
				continue;
			};
			stream.set_linger(Some(keep_alive))?;
			let mut connection = match options.tls {
				#[cfg(feature = "tls")]
				true => {
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

			if let Ok(Break(_)) =
				task::preconnect_task(&mut state, &mut rx, &mut connection, &mut reconnect_delay)
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
