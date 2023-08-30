use crate::command::Command;
use mqtt_core::QoS;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub struct Client {
	tx: mpsc::UnboundedSender<Command>,
}

impl Client {
	pub(crate) fn new(tx: mpsc::UnboundedSender<Command>) -> Self {
		Self { tx }
	}

	pub async fn subscribe(&self, filters: Vec<(String, QoS)>) -> Vec<Option<QoS>> {
		let (tx, rx) = oneshot::channel();
		self.tx.send(Command::Subscribe { filters, tx }).unwrap();

		let result = rx.await.unwrap();
		result
	}
	//
}

impl Drop for Client {
	fn drop(&mut self) {
		let _ = self.tx.send(Command::Shutdown);
	}
}
