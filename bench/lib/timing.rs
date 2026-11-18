//! Timing for the harness benchmarks: polling with a limit.

use std::future::Future;
use std::time::{Duration, Instant};

/// How long to poll and how often.
#[derive(Debug, Clone, Copy)]
pub struct WaitOptions {
	pub timeout_ms: u64,
	pub interval_ms: u64,
}

impl WaitOptions {
	/// Polls for `timeout_ms`, every 100 ms.
	pub fn within(timeout_ms: u64) -> Self {
		Self { timeout_ms, interval_ms: 100 }
	}
}

/// Polls `check` every `interval_ms` until it returns true; resolves the ms from the call to the first true, or `None`
/// past `timeout_ms`.
pub async fn wait_for<F, Fut>(mut check: F, options: WaitOptions) -> Option<f64>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = bool>,
{
	let started = Instant::now();
	loop {
		if check().await {
			return Some(started.elapsed().as_secs_f64() * 1000.0);
		}
		if started.elapsed() >= Duration::from_millis(options.timeout_ms) {
			return None;
		}
		tokio::time::sleep(Duration::from_millis(options.interval_ms)).await;
	}
}
