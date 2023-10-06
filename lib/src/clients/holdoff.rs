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
		self.cur = Some(self.min);
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
	#[allow(unused)]
	pub fn wait(&self) {
		if let Some(duration) = self.cur {
			std::thread::sleep(duration);
		}
	}

	#[allow(unused)]
	pub fn wait_and_increase_with(&mut self, f: impl FnOnce(Duration) -> Duration) {
		self.wait();
		self.increase_with(f);
	}

	/// Sleep for the hold-off period. Any call to `wait()` before
	/// `increase_with()` is always a no-op.
	#[inline]
	#[cfg(feature = "tokio-client")]
	pub async fn wait_async(&self) {
		if let Some(duration) = self.cur {
			tokio::time::sleep(duration).await
		}
	}

	#[inline]
	#[cfg(feature = "tokio-client")]
	pub async fn wait_and_increase_with_async(&mut self, f: impl FnOnce(Duration) -> Duration) {
		self.wait_async().await;
		self.increase_with(f);
	}
}
