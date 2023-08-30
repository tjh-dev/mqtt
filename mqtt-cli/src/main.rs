use clap::Parser;
use mqtt_async::QoS;

#[derive(Parser)]
struct Arguments {}

#[tokio::main]
async fn main() -> mqtt_async::Result<()> {
	tracing_subscriber::fmt::init();

	let (client, handle) = mqtt_async::client(("mqtt.tjh.dev", 1883));
	let result = client
		.subscribe(vec![(String::from("fizzle/#"), QoS::AtMostOnce)])
		.await;

	tracing::info!(?result);

	handle.await??;
	Ok(())
}
