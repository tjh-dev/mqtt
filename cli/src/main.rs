use clap::Parser;
use mqtt::QoS;

#[derive(Parser)]
struct Arguments {}

#[tokio::main]
async fn main() -> mqtt::Result<()> {
	tracing_subscriber::fmt::init();

	let (client, handle) = mqtt::async_client::client(("mqtt.tjh.dev", 1883));
	let result = client
		.subscribe(vec![(String::from("#"), QoS::AtMostOnce)])
		.await;

	tracing::info!(?result);

	handle.await??;
	Ok(())
}
