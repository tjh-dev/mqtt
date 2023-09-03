use clap::{Parser, Subcommand, ValueEnum};
use mqtt_async::{FilterBuf, Options, QoS};
use std::{process, str::from_utf8};
use tracing::subscriber::SetGlobalDefaultError;
use tracing_subscriber::{filter::LevelFilter, EnvFilter};

#[tokio::main(flavor = "current_thread")]
async fn main() -> mqtt_async::Result<()> {
	setup_tracing()?;

	let arguments = Arguments::parse();
	let Arguments {
		command,
		host,
		port,
		id,
		keep_alive,
		disable_clean_session,
		qos,
	} = arguments;

	let options = Options {
		host,
		port,
		keep_alive,
		clean_session: !disable_clean_session,
		client_id: id.unwrap_or_else(|| build_client_id(!disable_clean_session)),
	};

	// Create the MQTT client.
	let (client, handle) = mqtt_async::client(options);

	match command {
		Commands::Sub { topic, .. } => {
			// Create a subscription to the provided topic
			let mut subscription = client
				.subscribe(vec![(FilterBuf::new(topic)?, qos.into())])
				.await?;

			// Receive messages ... forever.
			while let Some(message) = subscription.recv().await {
				println!(
					"{}: {}",
					message.topic,
					from_utf8(&message.payload).unwrap_or_default()
				);
			}

			// Dropping the subscription will cause the topic to be unsubscribed.
			drop(subscription);
		}
		Commands::Pub {
			count,
			topic,
			payload,
			..
		} => {
			for _ in 0..count {
				client
					.publish(&topic, payload.as_bytes().to_vec(), qos.into(), false)
					.await?;
			}
		}
	}

	drop(client);
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

	#[arg(long, value_enum, default_value = "qos0", rename_all = "lower")]
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

		#[arg(long, short = 'C', default_value = "1")]
		count: usize,

		#[arg(default_value = "#")]
		topic: String,

		payload: String,
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
