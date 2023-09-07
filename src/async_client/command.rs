use crate::{FilterBuf, PacketId, Publish, QoS};
use bytes::Bytes;
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
	Unsubscribe(UnsubscribeCommand),
	PublishComplete { id: PacketId },
	Shutdown,
}

#[derive(Debug)]
pub struct PublishCommand {
	pub topic: String,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
	pub response_tx: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct SubscribeCommand {
	pub filters: Vec<(FilterBuf, QoS)>,
	pub publish_tx: mpsc::Sender<Publish>,
	pub response_tx: oneshot::Sender<Vec<(FilterBuf, QoS)>>,
}

#[derive(Debug)]
pub struct UnsubscribeCommand {
	pub filters: Vec<FilterBuf>,
	pub response_tx: oneshot::Sender<()>,
}
