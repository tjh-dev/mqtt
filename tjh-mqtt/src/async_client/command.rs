use super::PublishTx;
use crate::{FilterBuf, PacketId, QoS, TopicBuf};
use bytes::Bytes;
use tokio::sync::{mpsc::UnboundedSender, oneshot};

/// Command responses are sent back to the caller via oneshot::Sender.
pub type ResponseTx<T> = oneshot::Sender<T>;
pub type CommandTx = UnboundedSender<Command>;

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
	pub topic: TopicBuf,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
	pub response_tx: ResponseTx<()>,
}

#[derive(Debug)]
pub struct SubscribeCommand {
	pub filters: Vec<(FilterBuf, QoS)>,
	pub publish_tx: PublishTx,
	pub response_tx: ResponseTx<Vec<(FilterBuf, QoS)>>,
}

#[derive(Debug)]
pub struct UnsubscribeCommand {
	pub filters: Vec<FilterBuf>,
	pub response_tx: ResponseTx<()>,
}
