use crate::TopicBuf;
use bytes::Bytes;

/// A published message received from the Server.
#[derive(Debug)]
pub struct Message {
	/// The topic the published message.
	pub topic: TopicBuf,

	pub retain: bool,

	/// The payload of the published message.
	pub payload: Bytes,
}
