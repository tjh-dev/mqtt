use super::command::{Command, CommandTx, PublishCommand, SubscribeCommand, UnsubscribeCommand};
use crate::{
	traits::{IntoFilters, IntoTopicBuf},
	FilterError, IntoFiltersWithQoS, InvalidTopic, QoS, TopicBuf,
};
use bytes::Bytes;
use core::fmt;
use tokio::{
	sync::{mpsc, oneshot},
	time::Instant,
};

mod subscription;
pub use subscription::{Message, MessageGuard, Subscription};

#[derive(Clone, Debug)]
pub struct Client {
	tx: CommandTx,
}

#[derive(Debug)]
pub struct ClientTaskClosed;

#[derive(Debug)]
pub enum ClientError {
	ClientTaskClosed,
	InvalidFilter(FilterError),
	InvalidTopic(InvalidTopic),
}

impl Client {
	pub(crate) fn new(tx: CommandTx) -> Self {
		Self { tx }
	}

	/// Sends a [`Subscribe`] packet with the requested filters to the Server.
	///
	/// Upon receiving a corresponding [`SubAck`], the client will return a
	/// [`Subscription`] which will yield any packets received matching the
	/// filters. The subscription will buffer upto the provided
	/// number of messages.
	///
	/// # Example
	///
	/// ```no_run
	/// # tokio_test::block_on(async {
	/// use tjh_mqtt::async_client;
	/// let (client, handle) = async_client::client(("localhost", 1883));
	///
	/// // Subscribe to topic "a/b" with the default quality of service (AtMostOnce).
	/// let mut subscription = client.subscribe("a/b", 8).await.unwrap();
	///
	/// // Receive messages matching the filter.
	/// while let Some(message) = subscription.recv().await {
	/// 	println!(
	/// 		"{}: {}",
	/// 		message.topic,
	/// 		from_utf8(&message.payload).unwrap_or_default()
	/// 	);
	/// }
	/// # })
	/// ```
	///
	/// [`Subscribe`]: crate::packets::Subscribe
	/// [`SubAck`]: crate::packets::SubAck
	#[tracing::instrument(skip(self), ret, err)]
	pub async fn subscribe<Filters>(
		&self,
		filters: Filters,
		buffer: usize,
	) -> Result<Subscription, ClientError>
	where
		Filters: IntoFiltersWithQoS + fmt::Debug,
	{
		let start = Instant::now();

		let filters = filters.into_filters_with_qos()?;
		let (response_tx, response_rx) = oneshot::channel();
		let (publish_tx, publish_rx) = mpsc::channel(buffer);
		self.tx.send(Command::Subscribe(SubscribeCommand {
			filters,
			publish_tx,
			response_tx,
		}))?;

		let subscribed_filters = response_rx.await?;
		let subscription = Subscription::new(subscribed_filters, publish_rx, self.tx.clone());

		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(subscription)
	}

	/// Sends a [`Publish`] packet with the provided topic and payload to the
	/// Server.
	///
	/// With a QoS of [`AtMostOnce`], the call will return as soon as the packet
	/// has been written to the transport stream; with [`AtLeastOnce`] the call
	/// will return when the corresponding [`PubAck`] has been received from the
	/// Server; and with [`ExactlyOnce`] the call will return when the
	/// corresponding [`PubComp`] has been received.
	///
	/// # Example
	///
	/// ```no_run
	/// # tokio_test::block_on(async {
	/// use tjh_mqtt::{async_client, QoS::AtMostOnce};
	/// let (client, handle) = async_client::client(("localhost", 1883));
	///
	/// // Publish a message.
	/// if client
	/// 	.publish("a/b", "Hello, world!", AtMostOnce, false)
	/// 	.await
	/// 	.is_ok()
	/// {
	/// 	println!("Message published.");
	/// }
	/// # })
	/// ```
	///
	/// [`AtMostOnce`]: crate::QoS#variant.AtMostOnce
	/// [`AtLeastOnce`]: crate::QoS#variant.AtLeastOnce
	/// [`ExactlyOnce`]: crate::QoS#variant.ExactlyOnce
	/// [`Publish`]: crate::packets::Publish
	/// [`PubAck`]: crate::packets::PubAck
	/// [`PubComp`]: crate::packets::PubComp
	#[tracing::instrument(skip(self), ret, err)]
	pub async fn publish<Topic>(
		&self,
		topic: Topic,
		payload: impl Into<Bytes> + fmt::Debug,
		qos: QoS,
		retain: bool,
	) -> Result<(), ClientError>
	where
		Topic: IntoTopicBuf + fmt::Debug,
	{
		let start = Instant::now();

		let topic: TopicBuf = topic.into_topic_buf()?;
		let (response_tx, response_rx) = oneshot::channel();
		self.tx.send(Command::Publish(PublishCommand {
			topic,
			payload: payload.into(),
			qos,
			retain,
			response_tx,
		}))?;

		response_rx.await?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	/// Sends an [`Unsubscribe`] packet with `filters` to the Server. On
	/// receiving a corresponding [`UnsubAck`], the client will drop any
	/// matching filters.
	///
	/// [`Unsubscribe`]: crate::packets::Unsubscribe
	/// [`UnsubAck`]: crate::packets::UnsubAck
	#[tracing::instrument(skip(self), ret, err)]
	pub async fn unsubscribe<Filters>(&self, filters: Filters) -> Result<(), ClientError>
	where
		Filters: IntoFilters + fmt::Debug,
	{
		let start = Instant::now();

		let filters = filters.into_filters()?;
		let (response_tx, response_rx) = oneshot::channel();
		self.tx.send(Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response_tx,
		}))?;

		response_rx.await?;
		tracing::debug!("completed in {:?}", start.elapsed());
		Ok(())
	}

	/// Sends a [`Disconnect`] packet to the Server.
	///
	/// A compliant Server must immediately close the connection.
	///
	/// [`Disconnect`]: crate::packets::Disconnect
	#[inline]
	pub async fn disconnect(self) -> Result<(), ClientTaskClosed> {
		self.tx.send(Command::Shutdown)?;
		Ok(())
	}
}

impl fmt::Display for ClientTaskClosed {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl<T> From<mpsc::error::SendError<T>> for ClientTaskClosed {
	#[inline]
	fn from(_: mpsc::error::SendError<T>) -> Self {
		Self
	}
}

impl From<oneshot::error::RecvError> for ClientTaskClosed {
	#[inline]
	fn from(_: oneshot::error::RecvError) -> Self {
		Self
	}
}

impl std::error::Error for ClientTaskClosed {}

impl fmt::Display for ClientError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

impl<T> From<mpsc::error::SendError<T>> for ClientError {
	fn from(_: mpsc::error::SendError<T>) -> Self {
		Self::ClientTaskClosed
	}
}

impl From<oneshot::error::RecvError> for ClientError {
	fn from(_: oneshot::error::RecvError) -> Self {
		Self::ClientTaskClosed
	}
}

impl From<FilterError> for ClientError {
	fn from(value: FilterError) -> Self {
		Self::InvalidFilter(value)
	}
}

impl From<InvalidTopic> for ClientError {
	fn from(value: InvalidTopic) -> Self {
		Self::InvalidTopic(value)
	}
}

impl std::error::Error for ClientError {}
