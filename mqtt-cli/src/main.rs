use clap::{Parser, Subcommand, ValueEnum};
use mqtt_async::{FilterBuf, Options, QoS};
use std::{io::stdin, process, str::from_utf8};
use tokio::{io, signal, task::JoinHandle};
use tracing::subscriber::SetGlobalDefaultError;
use tracing_subscriber::{filter::LevelFilter, EnvFilter};

#[tokio::main(flavor = "current_thread")]
async fn main() -> mqtt_async::Result<()> {
	setup_tracing()?;

	let arguments = Arguments::parse();
	let options: Options = (&arguments).into();
	let Arguments { command, qos, .. } = arguments;

	// Create the MQTT client.
	let (client, handle) = mqtt_async::client(options);

	match command {
		Commands::Sub { topic, .. } => {
			let filter = FilterBuf::new(topic)?;
			// Create a subscription to the provided topic
			let mut subscription = client.subscribe(vec![(filter.clone(), qos.into())]).await?;

			let signal_handler: JoinHandle<io::Result<()>> = {
				let client = client.clone();
				tokio::spawn(async move {
					signal::ctrl_c().await?;
					client.unsubscribe(vec![filter]).await.unwrap();
					Ok(())
				})
			};

			// Receive messages ... forever.
			while let Some(message) = subscription.recv().await {
				println!(
					"{}: {}",
					message.topic,
					from_utf8(&message.payload).unwrap_or_default()
				);
			}

			signal_handler.await??;
		}
		Commands::Pub {
			count,
			topic,
			payload,
			..
		} => {
			match payload {
				Some(payload) => {
					// The user has supplied the payload as a command-line argument. Publish
					// the payload `count` times.
					let payload = payload.as_bytes().to_vec();
					for _ in 0..count.unwrap_or(1) {
						client
							.publish(&topic, payload.clone(), qos.into(), false)
							.await?;
					}
				}
				None => {
					// The user has *not* supplied a payload on the command-line. Read lines
					// from stdin, and publish upto `count` times if specified or until
					// end-of-stream.
					for (n, line) in stdin().lines().enumerate() {
						if let Some(max) = count {
							if n == max {
								break;
							}
						}
						let buffer = line.unwrap().trim_end_matches('\n').as_bytes().to_vec();
						client.publish(&topic, buffer, qos.into(), false).await?;
					}
				}
			}
		}
	}

	client.disconnect().await?;
	handle.await??;

	Ok(())
}

fn setup_tracing() -> Result<(), SetGlobalDefaultError> {
	let filter = EnvFilter::builder()
		.with_default_directive(LevelFilter::ERROR.into())
		.with_env_var("MQTT_LOG")
		.try_from_env();

	let subscriber = tracing_subscriber::fmt()
		.with_file(true)
		.with_target(false)
		.with_env_filter(filter.unwrap_or_default())
		.finish();

	tracing::subscriber::set_global_default(subscriber)
}

impl From<&Arguments> for Options {
	fn from(value: &Arguments) -> Self {
		let Arguments {
			host,
			port,
			id,
			keep_alive,
			disable_clean_session,
			..
		} = value;

		Options {
			host: host.clone(),
			port: *port,
			keep_alive: *keep_alive,
			clean_session: !disable_clean_session,
			client_id: id
				.clone()
				.unwrap_or_else(|| build_client_id(!disable_clean_session)),
			..Default::default()
		}
	}
}

fn build_client_id(clean_session: bool) -> String {
	if !clean_session {
		format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),)
	} else {
		format!(
			"{}/{}:{}",
			env!("CARGO_PKG_NAME"),
			env!("CARGO_PKG_VERSION"),
			process::id()
		)
	}
}

#[derive(Debug, Parser)]
struct Arguments {
	#[command(subcommand)]
	command: Commands,

	/// MQTT broker to connect to.
	#[arg(
		long,
		short = 'H',
		global = true,
		default_value = "localhost",
		env = "MQTT_HOST"
	)]
	host: String,

	#[arg(long, short, global = true, default_value = "1883", env = "MQTT_PORT")]
	port: u16,

	/// ID to use for this client.
	#[arg(long, short = 'i', global = true, env = "MQTT_ID")]
	id: Option<String>,

	/// Keep-alive timeout, in seconds.
	#[arg(short = 'k', global = true, default_value = "60")]
	keep_alive: u16,

	/// Disable clean session to enable persistent sessions.
	#[arg(short = 'c', global = true)]
	disable_clean_session: bool,

	#[arg(
		long,
		value_enum,
		global = true,
		default_value = "qos0",
		rename_all = "lower"
	)]
	qos: InputQoS,
}

#[derive(Debug, Subcommand)]
enum Commands {
	/// Subscribe to a topic
	Sub {
		#[arg(from_global)]
		host: String,

		#[arg(from_global)]
		port: u16,

		#[arg(from_global)]
		id: Option<String>,

		#[arg(from_global)]
		disable_clean_session: bool,

		#[arg(from_global)]
		keep_alive: u16,

		#[arg(from_global)]
		qos: InputQoS,

		#[clap(default_value = "#")]
		topic: String,
	},
	Pub {
		#[arg(from_global)]
		host: String,

		#[arg(from_global)]
		port: u16,

		#[arg(from_global)]
		id: Option<String>,

		#[arg(from_global)]
		disable_clean_session: bool,

		#[arg(from_global)]
		qos: InputQoS,

		#[arg(long, short = 'C')]
		count: Option<usize>,

		topic: String,

		payload: Option<String>,
	},
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum InputQoS {
	Qos0,
	Qos1,
	Qos2,
}

impl From<InputQoS> for QoS {
	fn from(value: InputQoS) -> Self {
		match value {
			InputQoS::Qos0 => QoS::AtMostOnce,
			InputQoS::Qos1 => QoS::AtLeastOnce,
			InputQoS::Qos2 => QoS::ExactlyOnce,
		}
	}
}
