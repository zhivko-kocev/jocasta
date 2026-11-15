//! Polling with a limit: "wait until this holds, at most so long". Real retries (0517's `attempt`) are kept apart.

use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

use crate::shared::errors::Result;

/// Asks `check` at once and then every `interval_ms` until it holds or `timeout_ms` pass (on the monotonic clock, so a
/// clock change neither cuts nor stretches the wait); resolves whether it held. A check that fails ends the wait with
/// its error.
pub async fn wait_for<F, Fut>(mut check: F, timeout_ms: i64, interval_ms: i64) -> Result<bool>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = Result<bool>>,
{
	let end = Instant::now() + Duration::from_millis(timeout_ms.max(0).unsigned_abs());
	loop {
		if check().await? {
			return Ok(true);
		}
		let left = end.saturating_duration_since(Instant::now());
		if left.is_zero() {
			return Ok(false);
		}
		tokio::time::sleep(left.min(Duration::from_millis(interval_ms.max(0).unsigned_abs()))).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::errors::Error;

	#[tokio::test]
	async fn wait_for_checks_at_once_then_every_interval_and_resolves_true_once_the_check_holds() {
		let mut calls = 0;
		let held = wait_for(
			|| {
				calls += 1;
				let ok = calls == 3;
				async move { Ok(ok) }
			},
			5_000,
			1,
		)
		.await;
		assert!(held.unwrap());
		assert_eq!(calls, 3);
		let mut once = 0;
		let held = wait_for(
			|| {
				once += 1;
				async { Ok(true) }
			},
			0,
			1_000,
		)
		.await;
		assert!(held.unwrap());
		assert_eq!(once, 1);
	}

	#[tokio::test]
	async fn wait_for_resolves_false_past_its_time_and_a_check_that_fails_rejects_it() {
		let started = Instant::now();
		assert!(!wait_for(|| async { Ok(false) }, 60, 20).await.unwrap());
		assert!(started.elapsed() >= Duration::from_millis(55));
		let broken = wait_for(|| async { Err(Error::other("broken")) }, 100, 10).await;
		assert_eq!(broken.unwrap_err().to_string(), "broken");
	}
}
