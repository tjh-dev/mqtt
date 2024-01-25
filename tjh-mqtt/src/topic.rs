use core::borrow;
use std::{fmt, ops};

/// An MQTT topic.
///
/// Internally this is just an `&str`. For the owned variant, see [`TopicBuf`].
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Topic(str);

#[derive(Debug, thiserror::Error)]
pub enum InvalidTopic {
	#[error("topic cannot be empty")]
	Empty,
	#[error("topic cannot exceed maximum length for an MQTT string (65,535 bytes)")]
	TooLong,
	#[error("topic cannot contain a wildcard character ('{0}' at position {1})")]
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
	pub const fn len(&self) -> usize {
		let Self(inner) = self;
		inner.len()
	}

	/// Returns `true` if the topic has length of zero bytes.
	///
	/// Empty topics are not valid, so this should *always* be `false`.
	#[inline]
	pub const fn is_empty(&self) -> bool {
		let Self(inner) = self;
		inner.is_empty()
	}

	/// Returns the inner topic str.
	#[inline]
	pub const fn as_str(&self) -> &str {
		let Self(inner) = self;
		inner
	}

	/// Converts a `Topic` to an owned [`TopicBuf`]
	#[inline]
	pub fn to_topic_buf(&self) -> TopicBuf {
		TopicBuf::from(self)
	}

	/// Returns an iterator over the levels in the topic.
	///
	/// # Example
	/// ```
	/// # use tjh_mqtt::Topic;
	/// let mut levels = Topic::new("a/b/c").unwrap().levels();
	/// assert_eq!(levels.next(), Some("a"));
	/// assert_eq!(levels.next(), Some("b"));
	/// assert_eq!(levels.next(), Some("c"));
	/// assert_eq!(levels.next(), None);
	/// ```
	#[inline]
	pub fn levels(&self) -> impl Iterator<Item = &str> {
		let Self(inner) = self;
		inner.split('/')
	}

	/// Creates a Topic from an `&'static str`. The validity of the topic is *not* checked.
	///
	/// # Example
	/// ```
	/// # use tjh_mqtt::Topic;
	/// const TOPIC: &Topic = Topic::from_static("a/b");
	/// ```
	pub const fn from_static(s: &'static str) -> &'static Topic {
		Self::from_str(s)
	}

	const fn from_str(s: &str) -> &Self {
		unsafe { &*(s as *const str as *const Topic) }
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
		self.to_topic_buf()
	}
}

impl<'t> TryFrom<&'t str> for &'t Topic {
	type Error = InvalidTopic;
	fn try_from(value: &'t str) -> Result<Self, Self::Error> {
		Topic::new(value)
	}
}

impl fmt::Display for Topic {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let Self(inner) = self;
		inner.fmt(f)
	}
}

#[cfg(feature = "serde")]
struct TopicVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for TopicVisitor {
	type Value = &'de Topic;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("an MQTT topic")
	}

	fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let topic = Topic::new(v).map_err(serde::de::Error::custom)?;
		Ok(topic)
	}
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for &'de Topic {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		deserializer.deserialize_str(TopicVisitor)
	}
}

/// An owned MQTT topic.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TopicBuf(String);

impl TopicBuf {
	/// Creates a new TopicBuf.
	pub fn new(topic: impl Into<String>) -> Result<Self, InvalidTopic> {
		let topic = topic.into();

		Topic::new(&topic)?;
		Ok(Self(topic))
	}

	pub fn to_inner(self) -> String {
		self.0
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

impl TryFrom<&str> for TopicBuf {
	type Error = InvalidTopic;
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		TopicBuf::new(value)
	}
}

impl TryFrom<String> for TopicBuf {
	type Error = InvalidTopic;
	#[inline]
	fn try_from(value: String) -> Result<Self, Self::Error> {
		Self::new(value)
	}
}

impl fmt::Display for TopicBuf {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let Self(inner) = self;
		inner.fmt(f)
	}
}

#[cfg(feature = "serde")]
struct TopicBufVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for TopicBufVisitor {
	type Value = TopicBuf;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("an MQTT topic")
	}

	fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let topic = TopicBuf::new(v).map_err(serde::de::Error::custom)?;
		Ok(topic)
	}

	fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let topic = TopicBuf::new(v).map_err(serde::de::Error::custom)?;
		Ok(topic)
	}
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TopicBuf {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		deserializer.deserialize_string(TopicBufVisitor)
	}
}
