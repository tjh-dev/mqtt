use std::sync::Arc;

use bytes::Bytes;
use mqtt_core::{FilterBuf, QoS};
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};

use crate::client::Subscription;

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
		tx: oneshot::Sender<Subscription>,
	},
	Unsubscribe {
		filters: Arc<Vec<FilterBuf>>,
		tx: oneshot::Sender<()>,
	},
	PublishComplete {
		id: u16,
	},
	Shutdown,
}
