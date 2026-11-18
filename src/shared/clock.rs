//! The wall clock in ms since the epoch: the one every module reads, as the TypeScript's every module read
//! `Date.now()`. A test stands it still as Bun's `setSystemTime` did.

/// Now, in ms since the epoch.
pub fn now_ms() -> i64 {
	#[cfg(any(test, feature = "test-support"))]
	{
		let still = testing::STILL_MS.load(std::sync::atomic::Ordering::SeqCst);
		if still != 0 && crate::shared::env::overlay::active() {
			return still;
		}
	}
	chrono::Utc::now().timestamp_millis()
}

#[cfg(any(test, feature = "test-support"))]
pub mod testing {
	use std::sync::atomic::{AtomicI64, Ordering};

	pub(super) static STILL_MS: AtomicI64 = AtomicI64::new(0);

	/// Stands the clock still at `ms` until the guard drops: Bun's `setSystemTime(date)`, the drop its
	/// `setSystemTime()`. The clock is the process's, so a test that stands it still holds a temp Jocasta, which
	/// serializes such tests within one process.
	#[must_use = "the clock runs again when the guard drops"]
	pub fn stand_still_at(ms: i64) -> StillClock {
		STILL_MS.store(ms, Ordering::SeqCst);
		StillClock
	}

	/// The standing clock; [`StillClock::at`] moves it, dropping it lets it run.
	pub struct StillClock;

	impl StillClock {
		pub fn at(&self, ms: i64) {
			STILL_MS.store(ms, Ordering::SeqCst);
		}
	}

	impl Drop for StillClock {
		fn drop(&mut self) {
			STILL_MS.store(0, Ordering::SeqCst);
		}
	}

	/// The values stay near the real time: the clock is the process's, and `cargo test` runs other tests beside this one.
	#[test]
	fn the_clock_stands_where_a_test_puts_it_and_runs_again_once_the_guard_drops() {
		let _j = crate::shared::testing::temp_jocasta();
		let t = super::now_ms();
		let still = stand_still_at(t + 1_000);
		assert_eq!(super::now_ms(), t + 1_000);
		still.at(t + 2_000);
		assert_eq!(super::now_ms(), t + 2_000);
		drop(still);
		let after = super::now_ms();
		assert!(after >= t && after < t + 1_000, "{after} runs from {t}");
	}
}
