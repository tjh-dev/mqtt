use mqtt_core::QoS;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum Command {
	Subscribe {
		filters: Vec<(String, QoS)>,
		tx: oneshot::Sender<Vec<Option<QoS>>>,
	},
	Shutdown,
}
