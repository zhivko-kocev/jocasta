//! Today's spend for the hud (the Rust port of `spend.ts`).
//!
//! The TypeScript keeps each index's state in a `WeakMap` and reads a new file "off the call" in a promise. Rust's
//! [`SessionIndex`] is one owner's `&mut` value, so the state is a [`SpendTracker`] the index's owner keeps beside
//! it, and the stepped read of the files a list found is [`spend_settled`], which the owner runs off the call (a task
//! that holds the index while it steps); [`spend_today`] itself never parses a new file.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::frontier::server::sessions::SessionIndex;
use crate::shared::day::{day_start, local_day};

/// How often the whole sessions folder is listed for today's files (CO-15, PERF-3); known files are read every call.
const LIST_MS: i64 = 30_000;

/// Today's session files known to [`spend_today`] for one index, and the files a list found that wait to be read in
/// steps ([`spend_settled`]).
#[derive(Debug, Default)]
pub struct SpendTracker {
	day: String,
	listed_at: Option<i64>,
	files: HashSet<PathBuf>,
	/// The files a list found that the index doesn't follow yet; set while they wait to be read (`warming`).
	warming: Option<Vec<PathBuf>>,
}

impl SpendTracker {
	pub fn new() -> Self {
		Self::default()
	}

	/// Whether a list found files that wait to be read ([`spend_settled`]).
	pub fn is_warming(&self) -> bool {
		self.warming.is_some()
	}
}

/// Today's spend (0274, 0403, 0433, 0452): the `usage.cost.total` of today's model calls (local day) in every omp
/// session, its subagent and advisor transcripts included, as `omp stats` counts it (CR-04, F-06). Every session
/// counts, wherever it ran: a merged thread's worktree is gone, yet its spend was today's. Each call counts once, by
/// the session index's ledger (0464): a moved session's copies don't count twice, and a call stays counted when the
/// file holding it is rewritten without it, moved or deleted, so the figure never drops within the day.
///
/// The sessions folder is listed at most every 30 s (PERF-3); in between only the files already known to be today's
/// are read (one stat each when unchanged). A file the list finds that the index doesn't follow yet is read in steps
/// off this call ([`spend_settled`], which runs `SessionIndex::warm`), so a refresh never parses a large session in
/// one block; its spend shows once read.
pub fn spend_today(tracker: &mut SpendTracker, sessions: &mut SessionIndex, now: i64) -> f64 {
	let today = local_day(now);
	if tracker.day != today {
		*tracker = SpendTracker { day: today.clone(), ..SpendTracker::default() };
	}
	if tracker.listed_at.is_none_or(|listed| now - listed >= LIST_MS || now < listed) {
		tracker.listed_at = Some(now);
		let mut fresh = Vec::new();
		for session in sessions.list(day_start(&today, 0)) {
			if sessions.follows(&session.file) {
				tracker.files.insert(session.file);
			} else {
				fresh.push(session.file);
			}
		}
		if !fresh.is_empty() && tracker.warming.is_none() {
			tracker.warming = Some(fresh);
		}
	}
	for file in &tracker.files {
		sessions.read(file, false);
	}
	sessions.spent_on(&today)
}

/// Reads the sessions [`spend_today`] found new, in steps (tests; the next refresh shows their spend).
pub async fn spend_settled(tracker: &mut SpendTracker, sessions: &mut SessionIndex) {
	let Some(fresh) = tracker.warming.clone() else { return };
	sessions.warm(&fresh, || false).await;
	tracker.files.extend(fresh);
	tracker.warming = None;
}

#[cfg(test)]
#[path = "spend_tests.rs"]
mod tests;
