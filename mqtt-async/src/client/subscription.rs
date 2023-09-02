use super::ClientError;
use crate::command::{Command, CommandTx};
use bytes::Bytes;
use mqtt_core::{FilterBuf, QoS};
use std::ops;
use tokio::sync::{mpsc::Receiver, oneshot};

#[derive(Debug)]
pub struct Message {
	pub topic: String,
	pub payload: Bytes,
}
#[derive(Debug)]
pub struct MessageGuard {
	msg: Option<Message>,
	sig: Option<(u16, CommandTx)>,
}

#[derive(Debug)]
pub struct Subscription {
	tx: CommandTx,
	rx: Receiver<mqtt_core::Publish>,
	filters: Vec<(FilterBuf, QoS)>,
}

impl Subscription {
	pub(crate) fn new(
		filters: Vec<(FilterBuf, QoS)>,
		rx: Receiver<mqtt_core::Publish>,
		tx: CommandTx,
	) -> Self {
		Self { tx, rx, filters }
	}

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

	pub async fn unsubscribe(mut self) -> Result<(), ClientError> {
		let (response_tx, response_rx) = oneshot::channel();

		let filters = self.filters.drain(..).map(|(f, _)| f).collect();
		self.tx.send(Command::Unsubscribe {
			filters,
			response_tx,
		})?;

		response_rx.await?;
		Ok(())
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
