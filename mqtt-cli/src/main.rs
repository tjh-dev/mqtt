use clap::{Parser, Subcommand};
use mqtt_async::QoS;

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

#[tokio::main]
async fn main() -> mqtt_async::Result<()> {
	tracing_subscriber::fmt::init();

	let arguments = Arguments::parse();
	match arguments.command {
		Commands::Sub { topic, host, .. } => {
			let (client, handle) = mqtt_async::client((host, 1883));
			client.subscribe(vec![(topic, QoS::AtMostOnce)]).await?;
			handle.await??;
		}
		Commands::Pub => {}
	}

	Ok(())
}
