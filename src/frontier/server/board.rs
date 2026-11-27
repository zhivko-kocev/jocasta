//! The board (0718): `board json`, what the board window shows, built from the dock's rows; and the phase lines, one
//! line of `<data>/notifications.jsonl` each time an open thread's phase changes, for ultron to push to his phone.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use serde_json::json;

use crate::frontier::protocol::{Board, BoardRow, ThreadState};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::events::log_notification;
use crate::frontier::server::thread::{Phase, Thread, ThreadRef, subject};
use crate::frontier::server::wording::money;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};

#[cfg(test)]
#[path = "board_tests.rs"]
mod tests;

/// A thread's phase as the board and its lines say it: a stop first, then the task cycle's step, then what `today`
/// shows.
pub fn phase(t: &Thread) -> &'static str {
	if t.stuck.is_some() {
		return "stuck";
	}
	match t.phase {
		Phase::Review => return "review",
		Phase::Handoff => return "handoff",
		Phase::Finish => return "finishing",
		Phase::Task => {}
	}
	if t.shown == ThreadState::Done {
		return "done";
	}
	if t.plan.as_ref().and_then(|plan| plan.awaiting.as_deref()).is_some_and(|title| !title.is_empty()) {
		return "plan ready";
	}
	match t.shown {
		ThreadState::Blocked => "awaiting you",
		ThreadState::Working => "working",
		_ => "idle",
	}
}

/// One open thread's row: the dock's facts, its phase and its pull request's CI from the PR cache.
fn row(c: &ServiceContext, thread: &ThreadRef) -> BoardRow {
	let day = c.status.day_thread(thread);
	let (phase, pr_url) =
		thread.with(|t| (phase(t), t.task.as_ref().and_then(|task| task.pr.as_ref()).map(|pr| pr.url.clone())));
	BoardRow {
		pane_id: day.pane_id,
		project: day.project.unwrap_or_default(),
		title: day.title,
		phase: phase.to_owned(),
		line: day.line,
		since: day.changed_at,
		cost: day.cost,
		budget: day.budget,
		context: day.context,
		check: day.check,
		pr: pr_url.and_then(|url| c.thread_facts.pr_facts(&url)),
	}
}

/// The bridge command `board json` (0718): every open thread's row, the waiting starts, the check queue and today's
/// spend, from what the service already holds.
pub fn board_json(c: &ServiceContext) -> Result<String> {
	let board = Board {
		rows: c.threads.all().iter().map(|thread| row(c, thread)).collect(),
		queued: c.pending.rows(),
		checks: c.checks.load(),
		spend_today: c.status.spend_last(),
	};
	serde_json::to_string(&board).map_err(|error| Error::other(format!("board json: {error}")))
}

/// Each open thread's phase as last looked at, by pane id.
#[derive(Default)]
pub struct PhaseLines {
	seen: Mutex<HashMap<String, &'static str>>,
}

impl PhaseLines {
	/// Which threads' phase changed since the last look: their pane ids. A thread seen for the first time is noted, not
	/// returned; one that is gone is forgotten.
	fn changed(&self, now: &[(String, &'static str)]) -> Vec<String> {
		let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
		seen.retain(|pane, _| now.iter().any(|(open, _)| open == pane));
		now.iter()
			.filter_map(|(pane, phase)| {
				let before = seen.insert(pane.clone(), phase)?;
				(before != *phase).then(|| pane.clone())
			})
			.collect()
	}
}

/// The phase line of one thread: `<project>/<title>: <phase> · $<spent>`, logged and not shown.
fn phase_line(c: &ServiceContext, thread: &ThreadRef) -> serde_json::Value {
	let spent = c.status.day_thread(thread).cost.unwrap_or(0.0);
	let (pane, title) =
		thread.with(|t| (t.pane_id.clone(), format!("{}: {} · {}", subject(t), phase(t), money(spent))));
	json!({ "at": now_ms(), "paneId": pane, "reason": "phase", "title": title, "body": "", "result": "logged" })
}

/// On each refresh tick: one `phase` line in `notifications.jsonl` per open thread whose phase changed (0718).
pub fn log_phase_changes(c: &ServiceContext) -> Result<()> {
	let threads = c.threads.all();
	let now: Vec<(String, &'static str)> =
		threads.iter().map(|thread| thread.with(|t| (t.pane_id.clone(), phase(t)))).collect();
	for pane in c.phase_lines.changed(&now) {
		let Some(thread) = threads.iter().find(|thread| thread.with(|t| t.pane_id == pane)) else { continue };
		log_notification(&phase_line(c, thread)).map_err(|error| Error::other(format!("a phase line: {error}")))?;
	}
	Ok(())
}
