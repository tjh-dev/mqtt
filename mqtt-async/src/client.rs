use crate::command::{Command, CommandTx};
use bytes::Bytes;
use core::fmt;
use mqtt_core::QoS;
use tokio::{
	sync::{mpsc, oneshot},
	time::Instant,
};

#[derive(Debug)]
pub struct Client {
	tx: CommandTx,
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

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn subscribe(
		&self,
		filters: Vec<(String, QoS)>,
	) -> Result<Vec<Option<QoS>>, ClientError> {
		let start = Instant::now();

		let (tx, rx) = oneshot::channel();
		self.tx
			.send(Command::Subscribe { filters, tx })
			.map_err(|_| ClientError::Disconnected)?;

		let result = rx.await.map_err(|_| ClientError::Disconnected)?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(result)
	}

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn publish(
		&self,
		topic: impl Into<String> + fmt::Debug,
		payload: impl Into<Bytes> + fmt::Debug,
		qos: QoS,
		retain: bool,
	) -> Result<(), ClientError> {
		let start = Instant::now();

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
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}
}

impl Drop for Client {
	fn drop(&mut self) {
		let _ = self.tx.send(Command::Shutdown);
	}
}
