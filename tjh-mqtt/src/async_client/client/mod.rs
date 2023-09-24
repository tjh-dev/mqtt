mod subscription;

use super::command::{Command, CommandTx, PublishCommand, SubscribeCommand, UnsubscribeCommand};
use crate::{FilterBuf, FilterError, InvalidTopic, QoS, TopicBuf};
use bytes::Bytes;
use core::fmt;
use tokio::sync::{mpsc, oneshot};

pub use subscription::{Message, Subscription};

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

pub struct Filters(Vec<FilterBuf>);
pub struct FiltersWithQoS(Vec<(FilterBuf, QoS)>);

impl Client {
	pub(crate) fn new(tx: CommandTx) -> Self {
		Self { tx }
	}

	/// Sends a [`Subscribe`] packet with the requested filters to the Server.
	///
	/// Upon receiving a corresponding [`SubAck`], the client will return a
	/// [`Subscription`] which will yield any packets received matching the
	/// filters. The subscription will buffer upto the specified
	/// number of messages.
	///
	/// # Example
	///
	/// ```no_run
	/// # tokio_test::block_on(async {
	/// # use core::str::from_utf8;
	/// use tjh_mqtt::async_client;
	/// let (client, handle) = async_client::tcp_client(("localhost", 1883));
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
	#[inline]
	pub async fn subscribe<TryIntoFiltersWithQoS, E>(
		&self,
		filters: TryIntoFiltersWithQoS,
		len: usize,
	) -> Result<Subscription, ClientError>
	where
		TryIntoFiltersWithQoS: TryInto<FiltersWithQoS, Error = E>,
		ClientError: From<E>,
	{
		self.subscribe_impl(filters.try_into()?, len).await
	}

	async fn subscribe_impl(
		&self,
		filters: FiltersWithQoS,
		buffer: usize,
	) -> Result<Subscription, ClientError> {
		let FiltersWithQoS(filters) = filters;

		let (response_tx, response_rx) = oneshot::channel();
		let (publish_tx, publish_rx) = mpsc::channel(buffer);

		self.tx.send(Command::Subscribe(SubscribeCommand {
			filters,
			publish_tx,
			response_tx,
		}))?;

		let subscribed_filters = response_rx.await?;
		let subscription = Subscription::new(subscribed_filters, publish_rx, self.tx.clone());

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
	/// let (client, handle) = async_client::tcp_client(("localhost", 1883));
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
	#[inline]
	pub async fn publish<TryIntoTopic, E>(
		&self,
		topic: TryIntoTopic,
		payload: impl Into<Bytes> + fmt::Debug,
		qos: QoS,
		retain: bool,
	) -> Result<(), ClientError>
	where
		TryIntoTopic: TryInto<TopicBuf, Error = E>,
		ClientError: From<E>,
	{
		self.publish_impl(topic.try_into()?, payload.into(), qos, retain)
			.await
	}

	async fn publish_impl(
		&self,
		topic: TopicBuf,
		payload: Bytes,
		qos: QoS,
		retain: bool,
	) -> Result<(), ClientError> {
		let (response_tx, response_rx) = oneshot::channel();

		self.tx.send(Command::Publish(PublishCommand {
			topic,
			payload,
			qos,
			retain,
			response_tx,
		}))?;

		response_rx.await?;
		Ok(())
	}

	/// Sends an [`Unsubscribe`] packet with `filters` to the Server. On
	/// receiving a corresponding [`UnsubAck`], the client will drop any
	/// matching filters.
	///
	/// [`Unsubscribe`]: crate::packets::Unsubscribe
	/// [`UnsubAck`]: crate::packets::UnsubAck
	#[inline]
	pub async fn unsubscribe<TryIntoFilters, E>(
		&self,
		filters: TryIntoFilters,
	) -> Result<(), ClientError>
	where
		TryIntoFilters: TryInto<Filters, Error = E>,
		ClientError: From<E>,
	{
		self.unsubscribe_impl(filters.try_into()?).await
	}

	async fn unsubscribe_impl(&self, filters: Filters) -> Result<(), ClientError> {
		let Filters(filters) = filters;

		let (response_tx, response_rx) = oneshot::channel();
		self.tx.send(Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response_tx,
		}))?;

		response_rx.await?;
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

impl TryFrom<&[&str]> for Filters {
	type Error = FilterError;
	fn try_from(value: &[&str]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push(FilterBuf::new(*s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<&[String]> for Filters {
	type Error = FilterError;
	fn try_from(value: &[String]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<&str>> for Filters {
	type Error = FilterError;
	fn try_from(value: Vec<&str>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.into_iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<String>> for Filters {
	type Error = FilterError;
	fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.into_iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<&str> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		let filter = FilterBuf::new(value)?;
		Ok(Self(vec![(filter, QoS::default())]))
	}
}

impl TryFrom<String> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: String) -> Result<Self, Self::Error> {
		let filter = FilterBuf::new(value)?;
		Ok(Self(vec![(filter, QoS::default())]))
	}
}

impl TryFrom<(&str, QoS)> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: (&str, QoS)) -> Result<Self, Self::Error> {
		let (raw_filter, qos) = value;
		let filter = FilterBuf::new(raw_filter)?;
		Ok(Self(vec![(filter, qos)]))
	}
}

impl TryFrom<(String, QoS)> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: (String, QoS)) -> Result<Self, Self::Error> {
		let (raw_filter, qos) = value;
		let filter = FilterBuf::new(raw_filter)?;
		Ok(Self(vec![(filter, qos)]))
	}
}

impl TryFrom<Vec<(&str, QoS)>> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: Vec<(&str, QoS)>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for (raw_filter, qos) in value.into_iter() {
			let filter = FilterBuf::new(raw_filter)?;
			filters.push((filter, qos));
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<(String, QoS)>> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: Vec<(String, QoS)>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for (raw_filter, qos) in value.into_iter() {
			let filter = FilterBuf::new(raw_filter)?;
			filters.push((filter, qos));
		}
		Ok(Self(filters))
	}
}

impl TryFrom<(Vec<&str>, QoS)> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: (Vec<&str>, QoS)) -> Result<Self, Self::Error> {
		let (raw_filters, qos) = value;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(Self(filters))
	}
}

impl TryFrom<(Vec<String>, QoS)> for FiltersWithQoS {
	type Error = FilterError;
	fn try_from(value: (Vec<String>, QoS)) -> Result<Self, Self::Error> {
		let (raw_filters, qos) = value;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(Self(filters))
	}
}
