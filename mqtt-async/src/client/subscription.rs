use super::ClientError;
use crate::command::{Command, CommandTx};
use mqtt_core::{FilterBuf, QoS};
use std::ops;
use tokio::sync::{mpsc::Receiver, oneshot};

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

	pub async fn unsubscribe(mut self) -> Result<(), ClientError> {
		let (tx, rx) = oneshot::channel();

		let filters = self.filters.drain(..).map(|(f, _)| f).collect();
		self.tx.send(Command::Unsubscribe { filters, response_tx: tx })?;

		rx.await?;
		Ok(())
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
		if !self.filters.is_empty() {
			let (tx, _) = oneshot::channel();
			let _ = self.tx.send(Command::Unsubscribe {
				filters: self.filters.drain(..).map(|(f, _)| f).collect(),
				response_tx: tx,
			});
		}
	}
}
