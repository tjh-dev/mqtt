use super::command::{Command, CommandTx, PublishCommand, SubscribeCommand, UnsubscribeCommand};
use crate::{FilterBuf, QoS};
use bytes::Bytes;
use core::fmt;
use tokio::{
	sync::{mpsc, oneshot},
	time::Instant,
};

mod subscription;
pub use subscription::{Message, Subscription};

#[derive(Clone, Debug)]
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

impl<T> From<mpsc::error::SendError<T>> for ClientError {
	fn from(_: mpsc::error::SendError<T>) -> Self {
		Self::Disconnected
	}
}

impl From<oneshot::error::RecvError> for ClientError {
	fn from(_: oneshot::error::RecvError) -> Self {
		Self::Disconnected
	}
}

impl std::error::Error for ClientError {}

impl Client {
	pub(crate) fn new(tx: CommandTx) -> Self {
		Self { tx }
	}

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn subscribe(
		&self,
		filters: Vec<(FilterBuf, QoS)>,
	) -> Result<Subscription, ClientError> {
		let start = Instant::now();

		let (response_tx, response_rx) = oneshot::channel();
		let (publish_tx, publish_rx) = mpsc::channel(32);
		self.tx
			.send(Command::Subscribe(SubscribeCommand {
				filters: filters.clone(),
				publish_tx,
				response_tx,
			}))
			.map_err(|_| ClientError::Disconnected)?;

		let result = response_rx.await.map_err(|_| ClientError::Disconnected)?;
		let subscription = Subscription::new(result, publish_rx, self.tx.clone());

		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(subscription)
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

		let (response_tx, response_rx) = oneshot::channel();
		self.tx
			.send(Command::Publish(PublishCommand {
				topic: topic.into(),
				payload: payload.into(),
				qos,
				retain,
				response_tx,
			}))
			.map_err(|_| ClientError::Disconnected)?;

		response_rx.await.map_err(|_| ClientError::Disconnected)?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn unsubscribe(&self, filters: Vec<FilterBuf>) -> Result<(), ClientError> {
		let start = Instant::now();

		let (response_tx, response_rx) = oneshot::channel();
		self.tx
			.send(Command::Unsubscribe(UnsubscribeCommand {
				filters,
				response_tx,
			}))
			.map_err(|_| ClientError::Disconnected)?;

		response_rx.await.map_err(|_| ClientError::Disconnected)?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	pub async fn disconnect(self) -> Result<(), ClientError> {
		self.tx
			.send(Command::Shutdown)
			.map_err(|_| ClientError::Disconnected)?;
		Ok(())
	}
}

// NOTE: This doesn't work, we don't want to disconnect when any clone of
// Client is dropped.
//
// impl Drop for Client {
// 	fn drop(&mut self) {
// 		let _ = self.tx.send(Command::Shutdown);
// 	}
// }
