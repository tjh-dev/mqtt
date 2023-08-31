use std::{ops, sync::Arc};

use crate::command::{Command, CommandTx};
use mqtt_core::FilterBuf;
use tokio::sync::{mpsc::Receiver, oneshot};

use super::ClientError;

#[derive(Debug)]
pub struct Message {
	pub topic: String,
	pub payload: Vec<u8>,
}
#[derive(Debug)]
pub enum MessageGuard {
	RequiresCompletion(Message, u16, CommandTx),
	NoCompletion(Message),
}

#[derive(Debug)]
pub struct Subscription {
	tx: CommandTx,
	rx: Receiver<mqtt_core::Publish>,
	filters: Arc<Vec<FilterBuf>>,
}

impl Subscription {
	pub(crate) fn new(
		filters: Arc<Vec<FilterBuf>>,
		rx: Receiver<mqtt_core::Publish>,
		tx: CommandTx,
	) -> Self {
		Self { tx, rx, filters }
	}

	pub async fn recv(&mut self) -> Option<MessageGuard> {
		match self.rx.recv().await? {
			mqtt_core::Publish::AtMostOnce { topic, payload, .. } => {
				Some(MessageGuard::NoCompletion(Message {
					topic,
					payload: payload.to_vec(),
				}))
			}
			mqtt_core::Publish::AtLeastOnce { topic, payload, .. } => {
				Some(MessageGuard::NoCompletion(Message {
					topic,
					payload: payload.to_vec(),
				}))
			}
			mqtt_core::Publish::ExactlyOnce {
				topic, payload, id, ..
			} => Some(MessageGuard::RequiresCompletion(
				Message {
					topic,
					payload: payload.to_vec(),
				},
				id,
				self.tx.clone(),
			)),
		}
	}

	pub async fn unsubscribe(self) -> Result<(), ClientError> {
		// PLAN:
		// - Send an Unsubscribe Command
		// - Wait for SubAck.
		// - Return.
		unimplemented!()
	}
}

impl Drop for MessageGuard {
	fn drop(&mut self) {
		if let Self::RequiresCompletion(_, id, tx) = self {
			tracing::trace!(
				"MessageGuard dropped, sending Command::PublishComplete {{ id: {id} }}"
			);
			let _ = tx.send(Command::PublishComplete { id: *id });
		}
	}
}

impl ops::Deref for MessageGuard {
	type Target = Message;
	fn deref(&self) -> &Self::Target {
		match self {
			Self::NoCompletion(message) => message,
			Self::RequiresCompletion(message, _, _) => message,
		}
	}
}

impl Drop for Subscription {
	fn drop(&mut self) {
		let (tx, _) = oneshot::channel();
		let _ = self.tx.send(Command::Unsubscribe {
			filters: Arc::clone(&self.filters),
			tx,
		});
	}
}
