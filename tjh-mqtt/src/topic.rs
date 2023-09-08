use core::borrow;
use std::ops;

/// An MQTT topic.
#[derive(Debug)]
pub struct Topic(str);

/// An owned MQTT topic.
#[derive(Debug)]
pub struct TopicBuf(String);

#[derive(Debug)]
pub enum InvalidTopic {
	Empty,
	TooLong,
	InvalidCharacter(usize, char),
}

impl Topic {
	/// Creates a new Topic.
	#[inline]
	pub fn new<S: AsRef<str> + ?Sized>(topic: &S) -> Result<&Topic, InvalidTopic> {
		let topic = topic.as_ref();

		if topic.is_empty() {
			return Err(InvalidTopic::Empty);
		}

		if topic.len() > u16::MAX as usize {
			return Err(InvalidTopic::TooLong);
		}

		for (position, character) in topic.chars().enumerate() {
			if ['+', '#'].contains(&character) {
				return Err(InvalidTopic::InvalidCharacter(position, character));
			}
		}

		Ok(unsafe { &*(topic as *const str as *const Topic) })
	}

	/// Returns the length of the topic in bytes when encoded as UTF-8.
	#[inline]
	pub fn len(&self) -> usize {
		let Self(inner) = self;
		inner.len()
	}

	/// Returns `true` if the topic has length of zero bytes.
	///
	/// Empty topics are not valid, so this should *always* be `false`.
	#[inline]
	pub fn is_empty(&self) -> bool {
		let Self(inner) = self;
		inner.is_empty()
	}

	/// Returns the inner topic str.
	#[inline]
	pub fn as_str(&self) -> &str {
		let Self(inner) = self;
		inner
	}

	/// Converts a `Topic` to an owned [`TopicBuf`]
	#[inline]
	pub fn to_filter_buf(&self) -> TopicBuf {
		TopicBuf::from(self)
	}

	/// Returns an iterator over the levels in the topic.
	#[inline]
	pub fn levels(&self) -> impl Iterator<Item = &str> {
		let Self(inner) = self;
		inner.split('/')
	}

	fn from_str(s: &str) -> &Self {
		unsafe { &*(s as *const str as *const Topic) }
	}
}

impl TopicBuf {
	/// Creates a new TopicBuf.
	pub fn new(topic: impl Into<String>) -> Result<Self, InvalidTopic> {
		let topic = topic.into();

		Topic::new(&topic)?;
		Ok(Self(topic))
	}
}

impl AsRef<str> for Topic {
	#[inline]
	fn as_ref(&self) -> &str {
		self.as_str()
	}
}

impl AsRef<Topic> for Topic {
	#[inline]
	fn as_ref(&self) -> &Topic {
		self
	}
}

impl ToOwned for Topic {
	type Owned = TopicBuf;
	#[inline]
	fn to_owned(&self) -> Self::Owned {
		self.to_filter_buf()
	}
}

impl ops::Deref for TopicBuf {
	type Target = Topic;
	#[inline]
	fn deref(&self) -> &Self::Target {
		let Self(inner) = self;
		Topic::from_str(inner)
	}
}

impl borrow::Borrow<Topic> for TopicBuf {
	#[inline]
	fn borrow(&self) -> &Topic {
		use ops::Deref;
		self.deref()
	}
}

impl From<&Topic> for TopicBuf {
	#[inline]
	fn from(value: &Topic) -> Self {
		let Topic(inner) = value;
		Self(String::from(inner))
	}
}

impl AsRef<Topic> for TopicBuf {
	#[inline]
	fn as_ref(&self) -> &Topic {
		Topic::from_str(self.as_str())
	}
}
