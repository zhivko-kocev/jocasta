//! The inbox (0302) in `me.db`: notes for a project's next work and reminders at a time.

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};

use crate::shared::errors::{Error, Result};

/// An inbox row (me.db v1, 0302): a note in a project's next work, or a reminder at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
	pub id: i64,
	pub text: String,
	pub project_id: Option<String>,
	pub due_at: Option<String>,
	pub created_at: String,
	pub done_at: Option<String>,
}

impl InboxItem {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			id: row.get("id")?,
			text: row.get("text")?,
			project_id: row.get("project_id")?,
			due_at: row.get("due_at")?,
			created_at: row.get("created_at")?,
			done_at: row.get("done_at")?,
		})
	}
}

/// Where an item goes: a project's next work (no time), or `today` at a time (with or without a project).
#[derive(Debug, Clone, Copy, Default)]
pub struct InboxWhere<'a> {
	pub project_id: Option<&'a str>,
	/// The reminder's time, ms since the epoch.
	pub due_at: Option<i64>,
}

/// `Date.toISOString()` of `ms` since the epoch.
fn iso(ms: i64) -> String {
	DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
	Utc::now().timestamp_millis()
}

/// The inbox (0302) in `me.db`.
pub struct Inbox<'a> {
	db: &'a Connection,
	now: Box<dyn Fn() -> i64 + 'a>,
}

impl<'a> Inbox<'a> {
	/// The inbox on `db`, with the real clock.
	pub fn new(db: &'a Connection) -> Self {
		Self { db, now: Box::new(now_ms) }
	}

	/// The inbox on `db` with `now` as its clock (ms since the epoch).
	pub fn with_clock(db: &'a Connection, now: impl Fn() -> i64 + 'a) -> Self {
		Self { db, now: Box::new(now) }
	}

	/// Files an item: a project's next work (no time), or `today` at a time (with or without a project).
	pub fn add(&self, text: &str, at: InboxWhere<'_>) -> Result<InboxItem> {
		let project_id = at.project_id.filter(|project| !project.is_empty());
		if project_id.is_none() && at.due_at.is_none() {
			return Err(Error::other("an inbox item needs a project or a time"));
		}
		Ok(self.db.query_row(
			"INSERT INTO inbox (text, project_id, due_at, created_at) VALUES (?, ?, ?, ?) RETURNING *",
			params![text, project_id, at.due_at.map(iso), iso((self.now)())],
			InboxItem::from_row,
		)?)
	}

	pub fn get(&self, id: i64) -> Result<Option<InboxItem>> {
		self.query("SELECT * FROM inbox WHERE id = ?", params![id]).map(|mut rows| rows.pop())
	}

	/// Marks an item done; false when there is no open item with that id.
	pub fn done(&self, id: i64) -> Result<bool> {
		let changes = self
			.db
			.execute("UPDATE inbox SET done_at = ? WHERE id = ? AND done_at IS NULL", params![iso((self.now)()), id])?;
		Ok(changes > 0)
	}

	/// A project's open notes: its next work.
	pub fn notes(&self, project_id: &str) -> Result<Vec<InboxItem>> {
		self.query(
			"SELECT * FROM inbox WHERE done_at IS NULL AND due_at IS NULL AND project_id = ? ORDER BY id",
			params![project_id],
		)
	}

	/// Open reminders due between `from` and `to` (ms since the epoch), by time.
	pub fn reminders(&self, from: i64, to: i64) -> Result<Vec<InboxItem>> {
		self.query(
			"SELECT * FROM inbox WHERE done_at IS NULL AND due_at >= ? AND due_at < ? ORDER BY due_at",
			params![iso(from), iso(to)],
		)
	}

	/// Open reminders whose time has come.
	pub fn due(&self) -> Result<Vec<InboxItem>> {
		self.query(
			"SELECT * FROM inbox WHERE done_at IS NULL AND due_at <= ? ORDER BY due_at",
			params![iso((self.now)())],
		)
	}

	fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<InboxItem>> {
		let mut statement = self.db.prepare_cached(sql)?;
		let rows = statement.query_map(params, InboxItem::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
		Ok(rows)
	}
}

#[cfg(test)]
mod tests {
	use std::cell::Cell;

	use chrono::TimeZone;

	use super::*;

	/// `me.db` v1 (the TypeScript's `ME_MIGRATIONS`, `shared/schema.ts`).
	fn me_db() -> Connection {
		let db = Connection::open_in_memory().unwrap();
		db.execute_batch(
			"CREATE TABLE inbox (
  id         INTEGER PRIMARY KEY,
  text       TEXT NOT NULL,
  project_id TEXT,
  due_at     TEXT,
  created_at TEXT NOT NULL,
  done_at    TEXT,
  CHECK (project_id IS NOT NULL OR due_at IS NOT NULL)
) STRICT;",
		)
		.unwrap();
		db
	}

	fn at(day: u32, hour: u32) -> i64 {
		Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap().timestamp_millis()
	}

	#[test]
	fn the_inbox_holds_notes_for_a_projects_next_work_and_reminders_at_a_time() {
		let db = me_db();
		let now = Cell::new(at(8, 9));
		let inbox = Inbox::with_clock(&db, || now.get());
		let note = inbox.add("try the new parser", InboxWhere { project_id: Some("p1"), due_at: None }).unwrap();
		let reminder = inbox.add("call the bank", InboxWhere { project_id: None, due_at: Some(at(8, 15)) }).unwrap();
		assert_eq!(
			inbox.add("nowhere", InboxWhere::default()).unwrap_err().to_string(),
			"an inbox item needs a project or a time"
		);
		assert_eq!(
			inbox.notes("p1").unwrap().iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
			["try the new parser"]
		);
		assert_eq!(
			inbox.reminders(at(8, 0), at(9, 0)).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(),
			[reminder.id]
		);
		assert_eq!(inbox.due().unwrap(), vec![]);
		assert!(inbox.done(note.id).unwrap());
		assert!(!inbox.done(note.id).unwrap());
		assert_eq!(inbox.notes("p1").unwrap(), vec![]);
	}

	#[test]
	fn items_carry_their_times_as_iso_strings_and_a_reminder_falls_due() {
		let db = me_db();
		let now = Cell::new(at(8, 9));
		let inbox = Inbox::with_clock(&db, || now.get());
		let reminder =
			inbox.add("call the bank", InboxWhere { project_id: Some("p1"), due_at: Some(at(8, 15)) }).unwrap();
		assert_eq!(reminder.created_at, "2026-10-08T09:00:00.000Z");
		assert_eq!(reminder.due_at.as_deref(), Some("2026-10-08T15:00:00.000Z"));
		assert_eq!(inbox.get(reminder.id).unwrap(), Some(reminder.clone()));
		assert_eq!(inbox.get(reminder.id + 1).unwrap(), None);
		now.set(at(8, 15));
		assert_eq!(inbox.due().unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), [reminder.id]);
		assert_eq!(inbox.notes("p1").unwrap(), vec![]);
	}
}
