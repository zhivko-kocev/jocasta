//! Daily spend totals for the hud's 7-day average (0668): `<data>/spend-days.json`, `{local-day: usd}`, the last 14
//! days kept. Today's total is recorded as it grows and written at most once a minute and at day rollover; at service
//! start [`backfill`] fills the prior days from omp's session files so the average doesn't start cold.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::frontier::server::sessions::{Clock, SessionIndex};
use crate::shared::day::{day_start, local_day};
use crate::shared::errors::Result;
use crate::shared::fsx::write_atomically;
use crate::shared::paths::data_dir;

/// Days kept in the file.
const KEEP_DAYS: usize = 14;
/// The average is over this many prior days at most.
const AVERAGE_DAYS: usize = 7;
/// The file is written at most this often, except at day rollover.
const WRITE_EVERY_MS: i64 = 60_000;
/// The backfill reads the session files modified in this many days back.
pub const BACKFILL_DAYS: i64 = 8;

struct State {
	days: BTreeMap<String, f64>,
	/// The day of the last `record`, so a new one is noticed.
	last_day: Option<String>,
	written_at: Option<i64>,
	dirty: bool,
}

/// The recorded daily totals; a cheap handle, clones share one record.
#[derive(Clone)]
pub struct SpendDays {
	file: PathBuf,
	state: Arc<Mutex<State>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The days of a file's text; anything that isn't a `{day: number}` object reads as none.
fn parse(text: &str) -> BTreeMap<String, f64> {
	let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(text) else {
		return BTreeMap::new();
	};
	object
		.into_iter()
		.filter_map(|(day, spent)| spent.as_f64().filter(|spent| spent.is_finite() && *spent >= 0.0).map(|s| (day, s)))
		.collect()
}

impl SpendDays {
	/// The record in `file`; missing or unreadable reads as empty.
	pub fn at(file: PathBuf) -> Self {
		let days = std::fs::read_to_string(&file).map(|text| parse(&text)).unwrap_or_default();
		Self { file, state: Arc::new(Mutex::new(State { days, last_day: None, written_at: None, dirty: false })) }
	}

	/// The record in `<data>/spend-days.json`.
	pub fn open() -> Self {
		Self::at(data_dir().join("spend-days.json"))
	}

	/// Notes today's total so far (it never drops within the day); nothing for none. Written when a new day began
	/// since the last call, else at most once a minute. A failed write is returned and tried again next call.
	pub fn record(&self, today: &str, spent: f64, now_ms: i64) -> Result<()> {
		let mut state = lock(&self.state);
		let rollover = state.last_day.as_deref().is_some_and(|last| last != today);
		state.last_day = Some(today.to_owned());
		if spent.is_finite() && spent > 0.0 && state.days.get(today).is_none_or(|known| spent > *known) {
			state.days.insert(today.to_owned(), spent);
			state.dirty = true;
		}
		let due = rollover || state.written_at.is_none_or(|at| now_ms - at >= WRITE_EVERY_MS || now_ms < at);
		if state.dirty && due { self.write(&mut state, now_ms) } else { Ok(()) }
	}

	/// The mean of the recorded days before `today`, the last 7 of them at most; `None` with none.
	pub fn average(&self, today: &str) -> Option<f64> {
		let state = lock(&self.state);
		let prior: Vec<f64> =
			state.days.range(..today.to_owned()).rev().take(AVERAGE_DAYS).map(|(_, spent)| *spent).collect();
		(!prior.is_empty()).then(|| prior.iter().sum::<f64>() / prior.len() as f64)
	}

	/// The recorded total of `day`.
	pub fn get(&self, day: &str) -> Option<f64> {
		lock(&self.state).days.get(day).copied()
	}

	/// Records `day`'s total unless it has one: the backfill never overwrites. Whether it was added.
	fn fill(&self, day: &str, spent: f64) -> bool {
		let mut state = lock(&self.state);
		if state.days.contains_key(day) {
			return false;
		}
		state.days.insert(day.to_owned(), spent);
		state.dirty = true;
		true
	}

	/// Writes now if anything changed.
	fn flush(&self, now_ms: i64) -> Result<()> {
		let mut state = lock(&self.state);
		if state.dirty { self.write(&mut state, now_ms) } else { Ok(()) }
	}

	fn write(&self, state: &mut State, now_ms: i64) -> Result<()> {
		while state.days.len() > KEEP_DAYS {
			state.days.pop_first();
		}
		let text = serde_json::to_string_pretty(&state.days)?;
		write_atomically(&self.file, format!("{text}\n"), None)?;
		state.dirty = false;
		state.written_at = Some(now_ms);
		Ok(())
	}
}

/// Fills the days before today that `store` lacks from the omp session files modified in the last 8 days, summed per
/// local day by the session ledger `spend_today` uses. Today is left out (it is live), a recorded day is never
/// overwritten and a day without spend stays missing (a day off doesn't pull the average down). Reads through a
/// throwaway index with a longer horizon: the shared one forgets days before yesterday. Run it off the event loop.
pub async fn backfill(sessions_dir: &Path, now_ms: i64, store: &SpendDays) {
	let today = local_day(now_ms);
	let clock: Clock = Arc::new(move || now_ms);
	let mut index = SessionIndex::with_horizon(sessions_dir.to_path_buf(), BACKFILL_DAYS, clock);
	let files: Vec<PathBuf> =
		index.list(day_start(&today, -BACKFILL_DAYS)).into_iter().map(|session| session.file).collect();
	index.warm(&files, || false).await;
	for file in &files {
		index.read(file, true);
	}
	for back in 1..=BACKFILL_DAYS {
		let day = local_day(day_start(&today, -back));
		let spent = index.spent_on(&day);
		if spent > 0.0 {
			store.fill(&day, spent);
		}
	}
	// A failed write leaves the days in memory; the next `record` writes them.
	let _ = store.flush(now_ms);
}

#[cfg(test)]
#[path = "spend_days_tests.rs"]
mod tests;
