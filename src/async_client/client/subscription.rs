use super::ClientTaskClosed;
use crate::async_client::{
	command::{Command, CommandTx, UnsubscribeCommand},
	state::PublishRx,
};
use crate::{FilterBuf, PacketId, QoS};
use bytes::Bytes;
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
		let Some(next_message) = self.rx.recv().await else {
			// All the matching senders for the channel have been closed or dropped.
			//
			// Drain the filters so the Drop impl does nothing.
			self.filters.drain(..);
			return None;
		};

		match next_message {
			crate::packets::Publish::AtMostOnce { topic, payload, .. } => Some(MessageGuard {
				msg: Some(Message { topic, payload }),
				sig: None,
			}),
			crate::packets::Publish::AtLeastOnce { topic, payload, .. } => Some(MessageGuard {
				msg: Some(Message { topic, payload }),
				sig: None,
			}),
			crate::packets::Publish::ExactlyOnce {
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
	#[tracing::instrument(ret, err)]
	pub async fn unsubscribe(mut self) -> Result<(), ClientTaskClosed> {
		let (response_tx, response_rx) = oneshot::channel();

		// Drain the filters from the Subscription. This will eliminate copying
		// and prevent the Drop impl from doing anything.
		let filters = self.filters.drain(..).map(|(f, _)| f).collect();
		self.tx.send(Command::Unsubscribe(UnsubscribeCommand {
			filters,
			response_tx,
		}))?;

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
			let _ = self.tx.send(Command::Unsubscribe(UnsubscribeCommand {
				filters: self.filters.drain(..).map(|(f, _)| f).collect(),
				response_tx: tx,
			}));
		}
	}
}
