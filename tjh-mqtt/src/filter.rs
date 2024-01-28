use crate::{Topic, TopicBuf};
use std::{borrow, cmp, convert, fmt, ops};

const LEVEL_SEPARATOR: char = '/';
const SINGLE_LEVEL_WILDCARD: char = '+';
const SINGLE_LEVEL_WILDCARD_STR: &str = "+";
const MULTI_LEVEL_WILDCARD: char = '#';
const MULTI_LEVEL_WILDCARD_STR: &str = "#";
const WILDCARDS: [char; 2] = [SINGLE_LEVEL_WILDCARD, MULTI_LEVEL_WILDCARD];

const DEFAULT: &Filter = Filter::from_static(MULTI_LEVEL_WILDCARD_STR);

/// An MQTT topic filter.
///
/// Internally this is just an `&str`. For the owned variant see [`FilterBuf`].
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Filter(str);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Matches {
	pub exact: usize,
	pub wildcard: usize,
	pub multi_wildcard: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidFilter {
	#[error("filter cannot be empty")]
	Empty,
	#[error("filter cannont exceed maximum length for an MQTT string (65,535 bytes)")]
	TooLong,
	#[error("filter levels cannot contain both wildcard and non-wildcard characters")]
	InvalidLevel,
	#[error("filter cannot contain multiple multi-level wildcards")]
	MultipleMultiLevelWildcards,
	#[error("multi-level wildcard can only appear in final filter level")]
	NonTerminalMultiLevelWildcard,
}

impl Filter {
	pub fn new<S: AsRef<str> + ?Sized>(filter: &S) -> Result<&Filter, InvalidFilter> {
		let filter = filter.as_ref();

		if filter.is_empty() {
			return Err(InvalidFilter::Empty);
		}

		if filter.len() > u16::MAX as usize {
			return Err(InvalidFilter::TooLong);
		}

		let mut multi_wildcard_position = None;
		let mut total_levels = 0;
		for (position, level) in filter.split(LEVEL_SEPARATOR).enumerate() {
			total_levels = position;

			if level.chars().any(|c| WILDCARDS.contains(&c)) && level.len() > 1 {
				return Err(InvalidFilter::InvalidLevel);
			}

			if level.contains(MULTI_LEVEL_WILDCARD)
				&& multi_wildcard_position.replace(position).is_some()
			{
				return Err(InvalidFilter::MultipleMultiLevelWildcards);
			}
		}

		if let Some(position) = multi_wildcard_position {
			if position != total_levels {
				return Err(InvalidFilter::NonTerminalMultiLevelWildcard);
			}
		}

		Ok(unsafe { &*(filter as *const str as *const Filter) })
	}

	/// Checks `topic` to determine if it would be matched by the `Filter`.
	///
	/// Returns `None` if the topic does not match. If `topic` does match, a
	/// tuple of the number of levels matched exactly and the number of levels
	/// matched by wildcards is returned.
	pub fn matches_topic(&self, topic: &Topic) -> Option<Matches> {
		let filter_levels = self.as_str().split(LEVEL_SEPARATOR);
		let mut topic_levels = topic.levels();

		let mut result = Matches::default();

		for filter_level in filter_levels {
			match filter_level {
				MULTI_LEVEL_WILDCARD_STR => {
					let matches = topic_levels.by_ref().count();
					result.multi_wildcard = (matches != 0).then_some(matches)?;
					break;
				}
				SINGLE_LEVEL_WILDCARD_STR => {
					topic_levels.next()?;
					result.wildcard += 1;
				}
				exact_match => {
					if !topic_levels.next().map_or(false, |t| t == exact_match) {
						return None;
					}
					result.exact += 1;
				}
			}
		}

		// Ensure all levels of the topic have been matched
		(topic_levels.count() == 0).then_some(result)
	}

	/// Returns the length of the filter in bytes when encoded as UTF-8.
	#[inline]
	pub const fn len(&self) -> usize {
		let Self(inner) = self;
		inner.len()
	}

	/// Returns the inner filter.
	#[inline]
	pub const fn as_str(&self) -> &str {
		let Self(inner) = self;
		inner
	}

	/// Converts a `Filter` to a owned [`FilterBuf`]
	#[inline]
	pub fn to_filter_buf(&self) -> FilterBuf {
		FilterBuf::from(self)
	}

	/// Returns an iterator over the levels of the filter.
	///
	/// # Example
	/// ```
	/// # use tjh_mqtt::Filter;
	/// let mut levels = Filter::new("a/b/c").unwrap().levels();
	/// assert_eq!(levels.next(), Some("a"));
	/// assert_eq!(levels.next(), Some("b"));
	/// assert_eq!(levels.next(), Some("c"));
	/// assert_eq!(levels.next(), None);
	/// ```
	#[inline]
	pub fn levels(&self) -> impl Iterator<Item = &str> {
		let Self(inner) = self;
		inner.split(LEVEL_SEPARATOR)
	}

	/// Creates a Filter from an `&'static str`. The validity of the filter is
	/// *not* checked.
	///
	/// # Example
	/// ```
	/// # use tjh_mqtt::Filter;
	/// const TOPIC: &Filter = Filter::from_static("a/b");
	/// ```
	#[inline]
	pub const fn from_static(filter: &'static str) -> &'static Filter {
		unsafe { &*(filter as *const str as *const Filter) }
	}

	const fn from_str(s: &str) -> &Self {
		unsafe { &*(s as *const str as *const Filter) }
	}
}

impl Default for &Filter {
	#[inline]
	fn default() -> Self {
		DEFAULT
	}
}

impl AsRef<str> for Filter {
	#[inline]
	fn as_ref(&self) -> &str {
		self.as_str()
	}
}

impl AsRef<Filter> for Filter {
	#[inline]
	fn as_ref(&self) -> &Filter {
		self
	}
}

impl ToOwned for Filter {
	type Owned = FilterBuf;
	#[inline]
	fn to_owned(&self) -> Self::Owned {
		self.to_filter_buf()
	}
}

// Any valid topic is also a valid filter.
impl<'a> From<&'a Topic> for &'a Filter {
	fn from(value: &'a Topic) -> &'a Filter {
		Filter::from_str(value.as_str())
	}
}

impl fmt::Display for Filter {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let Self(inner) = self;
		inner.fmt(f)
	}
}

#[cfg(feature = "serde")]
struct FilterVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for FilterVisitor {
	type Value = &'de Filter;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("an MQTT filter")
	}

	fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let filter = Filter::new(v).map_err(serde::de::Error::custom)?;
		Ok(filter)
	}
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for &'de Filter {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		deserializer.deserialize_str(FilterVisitor)
	}
}

/// An owned MQTT topic Filter.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FilterBuf(String);

impl FilterBuf {
	#[inline]
	pub fn new(filter: impl Into<String>) -> Result<Self, InvalidFilter> {
		let filter = filter.into();

		// Check the filter is valid
		Filter::new(&filter)?;
		Ok(Self(filter))
	}

	pub fn to_inner(self) -> String {
		self.0
	}
}

impl Default for FilterBuf {
	#[inline]
	fn default() -> Self {
		DEFAULT.to_owned()
	}
}

impl ops::Deref for FilterBuf {
	type Target = Filter;
	#[inline]
	fn deref(&self) -> &Self::Target {
		let Self(inner) = self;
		Filter::from_str(inner)
	}
}

impl borrow::Borrow<Filter> for FilterBuf {
	#[inline]
	fn borrow(&self) -> &Filter {
		use ops::Deref;
		self.deref()
	}
}

impl From<&Filter> for FilterBuf {
	#[inline]
	fn from(value: &Filter) -> Self {
		let Filter(inner) = value;
		Self(String::from(inner))
	}
}

impl AsRef<Filter> for FilterBuf {
	#[inline]
	fn as_ref(&self) -> &Filter {
		Filter::from_str(self.as_str())
	}
}

impl AsRef<str> for FilterBuf {
	#[inline]
	fn as_ref(&self) -> &str {
		&self.0
	}
}

impl TryFrom<&str> for FilterBuf {
	type Error = InvalidFilter;
	#[inline]
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		Self::new(value)
	}
}

impl TryFrom<String> for FilterBuf {
	type Error = InvalidFilter;
	#[inline]
	fn try_from(value: String) -> Result<Self, Self::Error> {
		Self::new(value)
	}
}

impl From<TopicBuf> for FilterBuf {
	#[inline]
	fn from(value: TopicBuf) -> Self {
		// Any valid topic is also a valid filter
		Self(value.to_inner())
	}
}

// This may be a bad idea?
impl From<convert::Infallible> for InvalidFilter {
	fn from(_: convert::Infallible) -> Self {
		unreachable!()
	}
}

impl fmt::Display for FilterBuf {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let Self(inner) = self;
		inner.fmt(f)
	}
}

#[cfg(feature = "serde")]
struct FilterBufVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for FilterBufVisitor {
	type Value = FilterBuf;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("an MQTT topic")
	}

	fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let filter = FilterBuf::new(v).map_err(serde::de::Error::custom)?;
		Ok(filter)
	}

	fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
	where
		E: serde::de::Error,
	{
		let filter = FilterBuf::new(v).map_err(serde::de::Error::custom)?;
		Ok(filter)
	}
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FilterBuf {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		deserializer.deserialize_string(FilterBufVisitor)
	}
}

#[cfg(test)]
mod tests {
	use super::{Filter, Matches};
	use crate::Topic;

	#[test]
	fn parses_filters() {
		// Valid filters
		for filter in [
			"a", "+", "#", "/", "a/", "/b", "a/b", "+/b", "a/+", "+/+", "+/#", "/#", "a/b/c/#",
		] {
			Filter::new(filter).unwrap();
		}

		// Invalid filters
		for filter in ["a/b+", "a/+b", "a/b#", "a/#b", "a/#/c", "#/"] {
			assert!(Filter::new(filter).is_err());
		}
	}

	#[test]
	fn matches_topics() {
		let filter = Filter::from_static("a/b/#");
		assert_eq!(filter.matches_topic(Topic::from_static("/b")), None);
		assert_eq!(filter.matches_topic(Topic::from_static("a/b")), None);
		assert_eq!(
			filter.matches_topic(Topic::from_static("a/b/c")),
			Some(Matches {
				exact: 2,
				wildcard: 0,
				multi_wildcard: 1
			})
		);
		assert_eq!(
			filter.matches_topic(Topic::from_static("a/b/c/d")),
			Some(Matches {
				exact: 2,
				wildcard: 0,
				multi_wildcard: 2
			})
		);

		let filter = Filter::from_static("+/+/c/#");
		assert_eq!(filter.matches_topic(Topic::from_static("/b")), None);
		assert_eq!(filter.matches_topic(Topic::from_static("a/b/c")), None);
		assert_eq!(filter.matches_topic(Topic::from_static("a/b/cd/e")), None);
		assert_eq!(
			filter.matches_topic(Topic::from_static("//c//")),
			Some(Matches {
				exact: 1,
				wildcard: 2,
				multi_wildcard: 2
			})
		);
	}
}

impl Matches {
	#[inline]
	pub fn score(&self) -> usize {
		self.exact * 100 + self.wildcard * 10 + self.multi_wildcard
	}
}

impl cmp::PartialOrd for Matches {
	#[inline]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl cmp::Ord for Matches {
	#[inline]
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.score().cmp(&other.score())
	}
}
