use std::{borrow, error, fmt, ops};

const LEVEL_SEPARATOR: char = '/';
const SINGLE_LEVEL_WILDCARD: char = '+';
const SINGLE_LEVEL_WILDCARD_STR: &str = "+";
const MULTI_LEVEL_WILDCARD: char = '#';
const MULTI_LEVEL_WILDCARD_STR: &str = "#";
const WILDCARDS: [char; 2] = [SINGLE_LEVEL_WILDCARD, MULTI_LEVEL_WILDCARD];

/// An MQTT topic filter.
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Filter(str);

/// An owned MQTT topic Filter.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FilterBuf(String);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Match {
	pub exact: usize,
	pub wildcard: usize,
	pub multi_wildcard: usize,
}

impl Match {
	pub fn score(&self) -> usize {
		self.exact * 100 + self.wildcard * 10 + self.multi_wildcard
	}
}

#[derive(Debug)]
pub struct FilterError {
	pub kind: ErrorKind,
	pub message: &'static str,
}

#[derive(Debug)]
pub enum ErrorKind {
	Length,
	InvalidWildcard,
	WildcardPosition,
}

impl FilterError {
	fn new(kind: ErrorKind, message: &'static str) -> Self {
		Self { kind, message }
	}
}

impl fmt::Display for FilterError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "invalid mqtt filter: {:?}, {}", self.kind, self.message)
	}
}

impl error::Error for FilterError {}

impl Filter {
	pub fn new<S: AsRef<str> + ?Sized>(filter: &S) -> Result<&Filter, FilterError> {
		let filter = filter.as_ref();

		if filter.is_empty() {
			return Err(FilterError::new(
				ErrorKind::Length,
				"filter must not be empty",
			));
		}

		if filter.len() > u16::MAX as usize {
			return Err(FilterError::new(ErrorKind::Length, "filter is too long"));
		}

		let mut multi_wildcard_position = None;
		let mut total_levels = 0;
		for (position, level) in filter.split(LEVEL_SEPARATOR).enumerate() {
			total_levels = position;

			if level.chars().any(|c| WILDCARDS.contains(&c)) && level.len() > 1 {
				return Err(FilterError::new(
					ErrorKind::InvalidWildcard,
					"wildcards '+' and '#' must occupy the whole filter level",
				));
			}

			if level.contains(MULTI_LEVEL_WILDCARD)
				&& multi_wildcard_position.replace(position).is_some()
			{
				return Err(FilterError::new(
					ErrorKind::WildcardPosition,
					"multi-level wildcard '#' can only appear once",
				));
			}
		}

		if let Some(position) = multi_wildcard_position {
			if position != total_levels {
				return Err(FilterError::new(
					ErrorKind::WildcardPosition,
					"multi-level wildcard '#' can only occupy the last level of the filter",
				));
			}
		}

		Ok(unsafe { &*(filter as *const str as *const Filter) })
	}

	fn from_str(s: &str) -> &Self {
		unsafe { &*(s as *const str as *const Filter) }
	}

	/// Checks `topic` to determine if it would be matched by the `Filter`.
	///
	/// Returns `None` if the topic does not match. If `topic` does match, a
	/// tuple of the number of levels matched exactly and the number of levels
	/// matched by wildcards is returned.
	///
	pub fn matches_topic(&self, topic: &str) -> Option<Match> {
		let filter_levels = self.as_str().split(LEVEL_SEPARATOR);
		let mut topic_levels = topic.split(LEVEL_SEPARATOR);

		let mut result = Match::default();

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

	#[inline]
	pub fn len(&self) -> usize {
		let Self(inner) = self;
		inner.len()
	}

	#[inline]
	pub fn is_empty(&self) -> bool {
		let Self(inner) = self;
		inner.is_empty()
	}

	#[inline]
	pub fn as_str(&self) -> &str {
		let Self(inner) = self;
		inner
	}

	/// Converts a `Filter` to a owned [`FilterBuf`]
	#[inline]
	pub fn to_filter_buf(&self) -> FilterBuf {
		FilterBuf::from(self)
	}
}

impl Filter {
	#[inline]
	pub const fn from_static(filter: &'static str) -> &'static Filter {
		unsafe { &*(filter as *const str as *const Filter) }
	}
}

impl AsRef<str> for Filter {
	#[inline]
	fn as_ref(&self) -> &str {
		let Self(inner) = self;
		inner
	}
}

impl AsRef<Filter> for Filter {
	#[inline]
	fn as_ref(&self) -> &Filter {
		self
	}
}

impl FilterBuf {
	#[inline]
	pub fn new(filter: impl Into<String>) -> Result<Self, FilterError> {
		let filter = filter.into();

		// Check the filter is valid
		Filter::new(&filter)?;
		Ok(Self(filter))
	}

	pub fn matches_topic(&self, topic: &str) -> Option<Match> {
		Filter::matches_topic(self, topic)
	}

	#[inline]
	pub fn len(&self) -> usize {
		let Self(inner) = self;
		inner.len()
	}

	#[inline]
	pub fn is_empty(&self) -> bool {
		let Self(inner) = self;
		inner.is_empty()
	}

	#[inline]
	pub fn as_str(&self) -> &str {
		let Self(inner) = self;
		inner
	}
}

impl ops::Deref for FilterBuf {
	type Target = Filter;
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

impl ToOwned for Filter {
	type Owned = FilterBuf;
	#[inline]
	fn to_owned(&self) -> Self::Owned {
		self.to_filter_buf()
	}
}

#[cfg(test)]
mod tests {
	use crate::filter::Match;

	use super::Filter;

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
		assert_eq!(filter.matches_topic("/b"), None);
		assert_eq!(filter.matches_topic("a/b"), None);
		assert_eq!(
			filter.matches_topic("a/b/c"),
			Some(Match {
				exact: 2,
				wildcard: 0,
				multi_wildcard: 1
			})
		);
		assert_eq!(
			filter.matches_topic("a/b/c/d"),
			Some(Match {
				exact: 2,
				wildcard: 0,
				multi_wildcard: 2
			})
		);

		let filter = Filter::from_static("+/+/c/#");
		assert_eq!(filter.matches_topic("/b"), None);
		assert_eq!(filter.matches_topic("a/b/c"), None);
		assert_eq!(filter.matches_topic("a/b/cd/e"), None);
		assert_eq!(
			filter.matches_topic("//c//"),
			Some(Match {
				exact: 1,
				wildcard: 2,
				multi_wildcard: 2
			})
		);
	}
}
