use core::time::Duration;
use std::{cmp, ops::Range};

#[derive(Debug)]
pub struct HoldOff {
	min: Duration,
	max: Duration,
	cur: Option<Duration>,
}

impl HoldOff {
	pub fn new(r: Range<Duration>) -> Self {
		Self {
			min: r.start,
			max: r.end,
			cur: None,
		}
	}

	/// Reset the hold-off period to `min`.
	pub fn reset(&mut self) {
		self.cur = None;
	}

	/// Increase the hold-off period.
	///
	/// If the new hold-off period is more then `max` then `max` is used.
	pub fn increase_with(&mut self, f: impl FnOnce(Duration) -> Duration) {
		self.cur = Some(match self.cur {
			None => self.min,
			Some(cur) => cmp::min(cmp::max(cur, f(cur)), self.max),
		});
	}

	/// Sleep for the hold-off period. Any call to `wait()` before
	/// `increase_with()` is always a no-op.
	pub async fn wait(&self) {
		if let Some(duration) = self.cur {
			tokio::time::sleep(duration).await
		}
	}
}
