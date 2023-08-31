use bytes::Bytes;
use mqtt_core::{FilterBuf, Publish, QoS};
use tokio::sync::{
	mpsc::{self, UnboundedReceiver, UnboundedSender},
	oneshot,
};

pub type CommandTx = UnboundedSender<Command>;
pub type CommandRx = UnboundedReceiver<Command>;

#[derive(Debug)]
pub enum Command {
	Publish(PublishCommand),
	Subscribe(SubscribeCommand),
	Unsubscribe {
		filters: Vec<FilterBuf>,
		tx: oneshot::Sender<()>,
	},
	PublishComplete {
		id: u16,
	},
	Shutdown,
}

#[derive(Debug)]
pub struct PublishCommand {
	pub topic: String,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
	pub tx: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct SubscribeCommand {
	pub filters: Vec<(FilterBuf, QoS)>,
	pub publish_tx: mpsc::Sender<Publish>,
	pub response_tx: oneshot::Sender<Vec<(FilterBuf, QoS)>>,
}
