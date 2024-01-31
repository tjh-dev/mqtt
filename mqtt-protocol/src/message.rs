use crate::TopicBuf;
use bytes::Bytes;

/// A published message received from the Server.
#[derive(Debug)]
pub struct Message {
	/// Message topic.
	pub topic: TopicBuf,
	/// Indicates whether the sender of the message set the retain flag.
	pub retain: bool,
	/// Message payload.
	pub payload: Bytes,
}

impl<T: Into<TopicBuf>> From<(T, bool, Bytes)> for Message {
	#[inline]
	fn from((topic, retain, payload): (T, bool, Bytes)) -> Self {
		Self {
			topic: topic.into(),
			retain,
			payload,
		}
	}
}
