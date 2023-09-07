use std::cmp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Matches {
	pub exact: usize,
	pub wildcard: usize,
	pub multi_wildcard: usize,
}

impl Matches {
	pub fn score(&self) -> usize {
		self.exact * 100 + self.wildcard * 10 + self.multi_wildcard
	}
}

impl cmp::PartialOrd for Matches {
	#[inline]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.score().partial_cmp(&other.score())
	}
}

impl cmp::Ord for Matches {
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.score().cmp(&other.score())
	}
}
