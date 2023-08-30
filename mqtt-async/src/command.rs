use bytes::Bytes;
use mqtt_core::QoS;
use tokio::sync::oneshot;

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
		filters: Vec<(String, QoS)>,
		tx: oneshot::Sender<Vec<Option<QoS>>>,
	},
	Shutdown,
}
