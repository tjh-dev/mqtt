use super::{ClientError, CommandTx};
use crate::{
	clients::{
		command::{Command, UnsubscribeCommand},
		tokio::PublishRx,
		Message,
	},
	FilterBuf, QoS,
};
use tokio::sync::oneshot;

/// A subscription to one or more topics.
#[derive(Debug)]
pub struct Subscription {
	tx: CommandTx,
	rx: PublishRx,
	filters: Vec<(FilterBuf, QoS)>,
}

impl Subscription {
	pub(crate) fn new(filters: Vec<(FilterBuf, QoS)>, rx: PublishRx, tx: CommandTx) -> Self {
		Self { tx, rx, filters }
	}

	/// Receive the next message from the Subscription.
	///
	/// # Example
	/// ```no_run
	/// # tokio_test::block_on(async {
	/// # use core::str::from_utf8;
	/// # use tjh_mqtt::clients::create_client;
	/// # let (client, handle) = create_client("mqtt://localhost".try_into().unwrap());
	/// let mut subscription = client.subscribe("a/b", 2).await.unwrap();
	/// while let Some(message) = subscription.recv().await {
	/// 	println!("{}: {:?}", &message.topic, &message.payload[..]);
	/// }
	/// # });
	/// ```
	#[inline]
	pub async fn recv(&mut self) -> Option<Message> {
		let Some(next_message) = self.rx.recv().await else {
			// All the matching senders for the channel have been closed or dropped.
			//
			// Drain the filters so the Drop impl does nothing.
			self.filters.drain(..);
			return None;
		};

		Some(next_message)
	}

	/// Unsubscribe all the filters associated with the Subscription.
	///
	/// This will send an 'Unsubscribe' packet to the Server, and won't return
	/// until a corresponding 'UnsubAck' packet has been recevied.
	#[tracing::instrument(ret, err)]
	pub async fn unsubscribe(mut self) -> Result<(), ClientError> {
		let (response, response_rx) = oneshot::channel();

		// Drain the filters from the Subscription. This will eliminate copying
		// and prevent the Drop impl from doing anything.
		let filters = self.filters.drain(..).map(|(f, _)| f).collect();
		self.tx
			.send(Command::Unsubscribe(UnsubscribeCommand { filters, response }).into())?;

		response_rx.await?;
		Ok(())
	}

	/// Returns a slice of the Filters associated with the Subscription.
	#[inline]
	pub fn filters(&self) -> &[(FilterBuf, QoS)] {
		&self.filters
	}
}

impl Drop for Subscription {
	#[inline]
	fn drop(&mut self) {
		if !self.filters.is_empty() {
			let (tx, _) = oneshot::channel();
			let _ = self.tx.send(
				Command::Unsubscribe(UnsubscribeCommand {
					filters: self.filters.drain(..).map(|(f, _)| f).collect(),
					response: tx,
				})
				.into(),
			);
		}
	}
}
