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
	Publish {
		topic: String,
		payload: Bytes,
		qos: QoS,
		retain: bool,
		tx: oneshot::Sender<()>,
	},
	Subscribe {
		filters: Vec<(FilterBuf, QoS)>,
		publish_tx: mpsc::Sender<Publish>,
		result_tx: oneshot::Sender<Vec<Option<QoS>>>,
	},
	Unsubscribe {
		filters: Vec<FilterBuf>,
		tx: oneshot::Sender<()>,
	},
	PublishComplete {
		id: u16,
	},
	Shutdown,
}
