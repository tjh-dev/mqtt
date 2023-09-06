use super::ClientError;
use crate::{
	command::{Command, CommandTx},
	state::PublishRx,
};
use bytes::Bytes;
use mqtt_core::{FilterBuf, PacketId, QoS};
use std::ops;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct Message {
	pub topic: String,
	pub payload: Bytes,
}
#[derive(Debug)]
pub struct MessageGuard {
	msg: Option<Message>,
	sig: Option<(PacketId, CommandTx)>,
}

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
	/// ```ignore
	/// let mut subscription = client.subscribe(("a/b", AtMostOnce)).await?;
	/// while let Ok(message) = subscription.recv().await {
	///     println!("{}: {:?}", &message.topic, &message.payload[..]);
	/// }
	/// ```
	pub async fn recv(&mut self) -> Option<MessageGuard> {
		match self.rx.recv().await? {
			mqtt_core::Publish::AtMostOnce { topic, payload, .. } => Some(MessageGuard {
				msg: Some(Message { topic, payload }),
				sig: None,
			}),
			mqtt_core::Publish::AtLeastOnce { topic, payload, .. } => Some(MessageGuard {
				msg: Some(Message { topic, payload }),
				sig: None,
			}),
			mqtt_core::Publish::ExactlyOnce {
				topic, payload, id, ..
			} => Some(MessageGuard {
				msg: Some(Message { topic, payload }),
				sig: Some((id, self.tx.clone())),
			}),
		}
	}

	/// Unsubscribe all the filters associated with the Subscription.
	///
	/// This will send an 'Unsubscribe' packet to the broker, and won't return
	/// until a corresponding 'UnsubAck' packet has been recevied.
	pub async fn unsubscribe(mut self) -> Result<(), ClientError> {
		let (response_tx, response_rx) = oneshot::channel();

		// Drain the filters from the Subscription. This will eliminate copying
		// and prevent the Drop impl from doing anything.
		let filters = self.filters.drain(..).map(|(f, _)| f).collect();
		self.tx.send(Command::Unsubscribe {
			filters,
			response_tx,
		})?;

		response_rx.await?;
		Ok(())
	}

	/// Returns a slice of the Filters associated with the Subscription.
	pub fn filters(&self) -> &[(FilterBuf, QoS)] {
		&self.filters
	}
}

impl MessageGuard {
	/// Mark the message as complete and take the contents.
	///
	/// For messages published with a Quality of Service of ExactlyOnce, this
	/// will trigger a PubComp message to be sent to the Server.
	pub fn complete(mut self) -> Message {
		if let Some((id, tx)) = self.sig.take() {
			let _ = tx.send(Command::PublishComplete { id });
		}
		self.msg.take().unwrap()
	}
}

impl Drop for MessageGuard {
	fn drop(&mut self) {
		if let Some((id, tx)) = self.sig.take() {
			let _ = tx.send(Command::PublishComplete { id });
		}
	}
}

impl ops::Deref for MessageGuard {
	type Target = Message;
	fn deref(&self) -> &Self::Target {
		self.msg.as_ref().unwrap()
	}
}

impl Drop for Subscription {
	fn drop(&mut self) {
		if !self.filters.is_empty() {
			let (tx, _) = oneshot::channel();
			let _ = self.tx.send(Command::Unsubscribe {
				filters: self.filters.drain(..).map(|(f, _)| f).collect(),
				response_tx: tx,
			});
		}
	}
}
