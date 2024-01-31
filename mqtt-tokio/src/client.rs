use crate::subscription::Subscription;

use super::{Command, CommandTx};
use bytes::Bytes;
use mqtt_client::{command::{PublishCommand, SubscribeCommand, UnsubscribeCommand}, conversions::{Filters, FiltersWithQoS}};
use mqtt_protocol::{InvalidFilter, InvalidTopic, QoS, TopicBuf};
use core::fmt;
use std::convert;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

/// An asychronous MQTT client, based on the tokio runtime.
#[derive(Clone, Debug)]
pub struct Client {
	tx: CommandTx,
}

#[derive(Debug, Error)]
pub enum ClientError {
	#[error("client task closed")]
	ClientTaskClosed,
	#[error("invalid filter(s): {0}")]
	InvalidFilter(#[from] InvalidFilter),
	#[error("invalid topic: {0}")]
	InvalidTopic(#[from] InvalidTopic),
}

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
	/// use tjh_mqtt::clients::create_client;
	/// let (client, handle) = create_client("mqtt://localhost".try_into().unwrap());
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
	pub async fn subscribe<T, E>(&self, filters: T, len: usize) -> Result<Subscription, ClientError>
	where
		T: TryInto<FiltersWithQoS, Error = E>,
		ClientError: From<E>,
	{
		self.subscribe_impl(filters.try_into()?, len).await
	}

	async fn subscribe_impl(
		&self,
		FiltersWithQoS(filters): FiltersWithQoS,
		buffer: usize,
	) -> Result<Subscription, ClientError> {
		// Create a channel to receive incoming Messages for this subscription.
		//
		// The client task will create a clone of `channel` for each topic filter.
		let (channel, publish_rx) = mpsc::channel(buffer);
		let (response, response_rx) = oneshot::channel();

		self.tx.send(
			Command::Subscribe(SubscribeCommand {
				filters,
				channel,
				response,
			})
			.into(),
		)?;

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
	/// use tjh_mqtt::{clients::create_client, QoS::AtMostOnce};
	/// let (client, handle) = create_client("mqtt://localhost".try_into().unwrap());
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
		let (response, response_rx) = oneshot::channel();

		self.tx.send(
			Command::Publish(PublishCommand {
				topic,
				payload,
				qos,
				retain,
				response,
			})
			.into(),
		)?;

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
	pub async fn unsubscribe<T, E>(&self, filters: T) -> Result<(), ClientError>
	where
		T: TryInto<Filters, Error = E>,
		ClientError: From<E>,
	{
		self.unsubscribe_impl(filters.try_into()?).await
	}

	async fn unsubscribe_impl(&self, Filters(filters): Filters) -> Result<(), ClientError> {
		let (response, response_rx) = oneshot::channel();
		self.tx
			.send(Command::Unsubscribe(UnsubscribeCommand { filters, response }).into())?;

		response_rx.await?;
		Ok(())
	}

	/// Sends a [`Disconnect`] packet to the Server.
	///
	/// A compliant Server must immediately close the connection.
	///
	/// [`Disconnect`]: crate::packets::Disconnect
	#[inline]
	pub async fn disconnect(self) -> Result<(), ClientError> {
		self.tx.send(Command::Shutdown.into())?;
		Ok(())
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

impl From<convert::Infallible> for ClientError {
	fn from(_: convert::Infallible) -> Self {
		unreachable!("infallible conversions cannot fail")
	}
}
