//! What each check costs (0671): the peak memory and the cores a check's process tree used on its last runs, kept in
//! `<data>/checks/costs.json` as `{"<project dir> :: <command>": {peak_rss_bytes, avg_cores, runs, at}}`, and the
//! sampler that measures a running check. The queue in `checks.rs` (`Admission`) admits a check by these records.
//!
//! - **Peak:** the maximum over the samples of the summed resident memory of the check's tree (about every 500 ms,
//!   so a spike shorter than that can be missed). A new record keeps three quarters of the old peak, so one outlier
//!   (a check recorded alone, a cold build) fades within a few runs.
//! - **Cores:** the tree's CPU time over wall time up to the last sample, averaged with the old record. A process that
//!   ended between two samples takes its CPU time with it: accepted.
//! - **A file that can't be read** (missing, not JSON, wrong shape) is no records; an entry that doesn't parse is
//!   dropped. The records are only a hint for the queue, never a result.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::shared::errors::Result;
use crate::shared::fsx::{read_json_object, write_atomically};
use crate::shared::log::{LogLevel, log};
use crate::shared::proctree::tree_usage;

/// The first look at a running check's tree, then one every [`SAMPLE_MS`]: a short check still gets one.
const FIRST_SAMPLE_MS: u64 = 100;
const SAMPLE_MS: u64 = 500;

/// One check's recorded cost.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CheckCost {
	pub peak_rss_bytes: u64,
	pub avg_cores: f64,
	pub runs: u32,
	/// When it was last recorded, epoch ms.
	pub at: i64,
}

impl CheckCost {
	/// The record after one more run measured `peak` bytes and `cores`.
	fn after(old: Option<&Self>, peak: u64, cores: f64, at: i64) -> Self {
		match old {
			None => Self { peak_rss_bytes: peak, avg_cores: cores, runs: 1, at },
			Some(old) => Self {
				peak_rss_bytes: peak.max(old.peak_rss_bytes - old.peak_rss_bytes / 4),
				avg_cores: (old.avg_cores + cores) / 2.0,
				runs: old.runs.saturating_add(1),
				at,
			},
		}
	}
}

/// The records, read from a file at start and written back at each change.
pub struct CheckCosts {
	file: Option<PathBuf>,
	records: Mutex<HashMap<String, CheckCost>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl CheckCosts {
	/// The records in `file`; none when it is missing or can't be read.
	pub fn load(file: PathBuf) -> Self {
		let records = read_json_object(&file)
			.map(|map| {
				map.into_iter()
					.filter_map(|(key, value)| Some((key, serde_json::from_value::<CheckCost>(value).ok()?)))
					.collect()
			})
			.unwrap_or_default();
		Self { file: Some(file), records: Mutex::new(records) }
	}

	/// No records and no file: nothing is kept past the process.
	pub fn in_memory() -> Self {
		Self { file: None, records: Mutex::new(HashMap::new()) }
	}

	/// A check's key: its project's folder and its command.
	pub fn key(project: &Path, command: &str) -> String {
		format!("{} :: {command}", project.display())
	}

	/// `key`'s record, when one was made.
	pub fn get(&self, key: &str) -> Option<CheckCost> {
		locked(&self.records).get(key).copied()
	}

	/// Every record with its key (`<project folder> :: <command>`), the biggest peak first.
	pub fn all(&self) -> Vec<(String, CheckCost)> {
		let mut all: Vec<(String, CheckCost)> =
			locked(&self.records).iter().map(|(key, cost)| (key.clone(), *cost)).collect();
		all.sort_by(|a, b| b.1.peak_rss_bytes.cmp(&a.1.peak_rss_bytes).then_with(|| a.0.cmp(&b.0)));
		all
	}

	/// Folds one run's measurement into `key`'s record and writes the file. A failed write is logged.
	pub fn record(&self, key: &str, peak_rss_bytes: u64, avg_cores: f64, at: i64) {
		let json = {
			let mut records = locked(&self.records);
			let next = CheckCost::after(records.get(key), peak_rss_bytes, avg_cores, at);
			records.insert(key.to_owned(), next);
			self.file.as_ref().map(|_| serde_json::to_string_pretty(&*records))
		};
		let (Some(file), Some(json)) = (&self.file, json) else { return };
		let written = json.map_err(crate::shared::errors::Error::from).and_then(|json| Self::write(file, &json));
		if let Err(error) = written {
			log(LogLevel::Warn, "checks", &format!("couldn't write {}: {error}", file.display()), None);
		}
	}

	fn write(file: &Path, json: &str) -> Result<()> {
		if let Some(dir) = file.parent() {
			std::fs::create_dir_all(dir)?;
		}
		write_atomically(file, format!("{json}\n"), None)
	}
}

/// What a run measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
	pub peak_rss_bytes: u64,
	pub avg_cores: f64,
}

#[derive(Default)]
struct Samples {
	peak_rss_bytes: u64,
	cpu_ms: u64,
	/// Wall time since the start at the last sample.
	at_ms: u64,
}

/// Looks at a running check's process tree until stopped.
pub struct Sampler {
	samples: Arc<Mutex<Samples>>,
	task: JoinHandle<()>,
}

impl Sampler {
	/// Samples the tree under `root` on the current tokio runtime; the reads run off the event loop.
	pub fn start(root: u32) -> Self {
		let samples = Arc::new(Mutex::new(Samples::default()));
		let shared = Arc::clone(&samples);
		let task = tokio::spawn(async move {
			let began = Instant::now();
			let mut wait = FIRST_SAMPLE_MS;
			loop {
				tokio::time::sleep(Duration::from_millis(wait)).await;
				wait = SAMPLE_MS;
				let Ok(usage) = tokio::task::spawn_blocking(move || tree_usage(root)).await else { return };
				let mut samples = locked(&shared);
				samples.peak_rss_bytes = samples.peak_rss_bytes.max(usage.rss_bytes);
				// The sum can fall when a process ends: the most seen is kept.
				samples.cpu_ms = samples.cpu_ms.max(usage.cpu_ms);
				samples.at_ms = began.elapsed().as_millis() as u64;
			}
		});
		Self { samples, task }
	}

	/// Ends the sampling; `None` when no sample saw any memory (a check that ended before the first look).
	pub fn stop(self) -> Option<Measured> {
		let samples = locked(&self.samples);
		(samples.peak_rss_bytes > 0).then(|| Measured {
			peak_rss_bytes: samples.peak_rss_bytes,
			avg_cores: samples.cpu_ms as f64 / samples.at_ms.max(1) as f64,
		})
	}
}

impl Drop for Sampler {
	fn drop(&mut self) {
		self.task.abort();
	}
}

#[cfg(test)]
#[path = "check_costs_tests.rs"]
mod tests;
