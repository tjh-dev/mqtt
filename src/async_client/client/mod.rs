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
pub struct ClientTaskClosed;

impl Client {
	pub(crate) fn new(tx: CommandTx) -> Self {
		Self { tx }
	}

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn subscribe(
		&self,
		filters: Vec<(FilterBuf, QoS)>,
		len: usize,
	) -> Result<Subscription, ClientTaskClosed> {
		let start = Instant::now();

		let (response_tx, response_rx) = oneshot::channel();
		let (publish_tx, publish_rx) = mpsc::channel(len);
		self.tx.send(Command::Subscribe(SubscribeCommand {
			filters: filters.clone(),
			publish_tx,
			response_tx,
		}))?;

		let result = response_rx.await?;
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
	) -> Result<(), ClientTaskClosed> {
		let start = Instant::now();

		let (response_tx, response_rx) = oneshot::channel();
		self.tx.send(Command::Publish(PublishCommand {
			topic: topic.into(),
			payload: payload.into(),
			qos,
			retain,
			response_tx,
		}))?;

		response_rx.await?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	#[tracing::instrument(skip(self), ret, err)]
	pub async fn unsubscribe(&self, filters: Vec<FilterBuf>) -> Result<(), ClientTaskClosed> {
		let start = Instant::now();

		let (response_tx, response_rx) = oneshot::channel();
		self.tx.send(Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response_tx,
		}))?;

		response_rx.await?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	pub async fn disconnect(self) -> Result<(), ClientTaskClosed> {
		self.tx.send(Command::Shutdown)?;
		Ok(())
	}
}

impl fmt::Display for ClientTaskClosed {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl<T> From<mpsc::error::SendError<T>> for ClientTaskClosed {
	fn from(_: mpsc::error::SendError<T>) -> Self {
		Self
	}
}

impl From<oneshot::error::RecvError> for ClientTaskClosed {
	fn from(_: oneshot::error::RecvError) -> Self {
		Self
	}
}

impl std::error::Error for ClientTaskClosed {}
