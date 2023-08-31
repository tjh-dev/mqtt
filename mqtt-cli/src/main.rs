use std::{process, str::from_utf8};

use clap::{Parser, Subcommand};
use mqtt_async::{FilterBuf, Options, QoS};
use tracing_subscriber::{filter::LevelFilter, EnvFilter};

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

	/// ID to use for this client. Defaults to a randomly generated string.
	#[arg(long, short = 'i', global = true, env = "MQTT_ID")]
	id: Option<String>,

	/// Keep-alive timeout, in seconds.
	#[arg(short = 'k', global = true, default_value = "60")]
	keep_alive: u16,

	/// Disable clean session to enable persistent sessions. Ignored if client ID
	/// is not specified.
	#[arg(short = 'c', global = true)]
	disable_clean_session: bool,
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
	Pub,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> mqtt_async::Result<()> {
	// Setup tracing
	//
	let filter = EnvFilter::builder()
		.with_default_directive(LevelFilter::ERROR.into())
		.with_env_var("MQTT_LOG")
		.try_from_env();

	let subscriber = tracing_subscriber::fmt()
		.with_file(true)
		.with_target(false)
		.with_env_filter(filter.unwrap_or_default())
		.finish();

	tracing::subscriber::set_global_default(subscriber)?;

	let arguments = Arguments::parse();
	match arguments.command {
		Commands::Sub {
			topic,
			host,
			port,
			keep_alive,
			disable_clean_session,
			id,
		} => {
			let options = Options {
				host,
				port,
				keep_alive,
				clean_session: !disable_clean_session,
				client_id: id.unwrap_or_else(|| {
					if disable_clean_session {
						format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),)
					} else {
						format!(
							"{}/{}:{}",
							env!("CARGO_PKG_NAME"),
							env!("CARGO_PKG_VERSION"),
							process::id()
						)
					}
				}),
			};

			let (client, handle) = mqtt_async::client(options);
			let mut subscription = client
				.subscribe(vec![(FilterBuf::new(topic)?, QoS::ExactlyOnce)])
				.await?;

			while let Some(message) = subscription.recv().await {
				println!(
					"{}: {}",
					message.topic,
					from_utf8(&message.payload).unwrap_or_default()
				);
			}

			drop(subscription);
			drop(client);
			handle.await??;
		}
		Commands::Pub => {}
	}

	Ok(())
}
