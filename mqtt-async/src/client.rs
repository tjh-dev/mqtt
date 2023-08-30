use core::fmt;

use crate::command::Command;
use bytes::Bytes;
use mqtt_core::QoS;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub struct Client {
	tx: mpsc::UnboundedSender<Command>,
}

#[derive(Debug)]
pub enum ClientError {
	Disconnected,
}

impl fmt::Display for ClientError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl std::error::Error for ClientError {}

impl Client {
	pub(crate) fn new(tx: mpsc::UnboundedSender<Command>) -> Self {
		Self { tx }
	}

	pub async fn subscribe(
		&self,
		filters: Vec<(String, QoS)>,
	) -> Result<Vec<Option<QoS>>, ClientError> {
		let (tx, rx) = oneshot::channel();
		self.tx
			.send(Command::Subscribe { filters, tx })
			.map_err(|_| ClientError::Disconnected)?;

		let result = rx.await.map_err(|_| ClientError::Disconnected)?;
		Ok(result)
	}

	pub async fn publish(
		&self,
		topic: impl Into<String>,
		payload: impl Into<Bytes>,
		qos: QoS,
		retain: bool,
	) -> Result<(), ClientError> {
		let (tx, rx) = oneshot::channel();
		self.tx
			.send(Command::Publish {
				topic: topic.into(),
				payload: payload.into(),
				qos,
				retain,
				tx,
			})
			.map_err(|_| ClientError::Disconnected)?;

		rx.await.map_err(|_| ClientError::Disconnected)?;
		Ok(())
	}
}

impl Drop for Client {
	fn drop(&mut self) {
		let _ = self.tx.send(Command::Shutdown);
	}
}
