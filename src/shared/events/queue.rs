//! The event queue in `catalog.db` (0298): one row per source id.

use std::collections::HashSet;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, Row, named_params, params};
use serde::{Deserialize, Serialize};

use crate::shared::errors::Result;

/// Where an event came from (0298).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventSource {
	Github,
	Azure,
	Calendar,
	Thread,
	Machine,
	Inbox,
}

impl EventSource {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Github => "github",
			Self::Azure => "azure",
			Self::Calendar => "calendar",
			Self::Thread => "thread",
			Self::Machine => "machine",
			Self::Inbox => "inbox",
		}
	}

	fn parse(text: &str) -> Option<Self> {
		[Self::Github, Self::Azure, Self::Calendar, Self::Thread, Self::Machine, Self::Inbox]
			.into_iter()
			.find(|source| source.as_str() == text)
	}
}

/// How much an event matters (0298).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventPriority {
	Urgent,
	Normal,
	Low,
}

impl EventPriority {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Urgent => "urgent",
			Self::Normal => "normal",
			Self::Low => "low",
		}
	}

	fn parse(text: &str) -> Option<Self> {
		[Self::Urgent, Self::Normal, Self::Low].into_iter().find(|priority| priority.as_str() == text)
	}
}

/// A new event: its `source_id` deduplicates it (0298).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEvent {
	pub source_id: String,
	pub source: EventSource,
	/// What it is, for `today` and resolving: `review`, `pipeline`, `meeting-soon`, `disk`, `reminder`, …
	pub kind: String,
	pub priority: EventPriority,
	/// One plain line.
	pub text: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub project_id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub thread_id: Option<String>,
}

/// One round of a poller that owns kinds of events (SHR-8): its current events, the source ids it could see (open events
/// of its kinds outside that scope aren't resolved), and the failures that shrank the scope without ending the round.
pub struct PollRound {
	pub events: Vec<NewEvent>,
	pub observed: Box<dyn Fn(&str) -> bool>,
	pub failures: Vec<String>,
}

/// An `events` row (catalog.db v5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRow {
	pub id: i64,
	pub source_id: String,
	pub source: EventSource,
	pub kind: String,
	pub priority: EventPriority,
	pub text: String,
	pub url: Option<String>,
	pub project_id: Option<String>,
	pub thread_id: Option<String>,
	pub created_at: String,
	pub raised_at: Option<String>,
	pub reminded_at: Option<String>,
	pub resolved_at: Option<String>,
}

fn word<T>(row: &Row<'_>, column: &str, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
	let index = row.as_ref().column_index(column)?;
	let text: String = row.get(index)?;
	parse(&text).ok_or_else(|| {
		rusqlite::Error::FromSqlConversionFailure(index, Type::Text, format!("unknown {column} {text:?}").into())
	})
}

impl EventRow {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			id: row.get("id")?,
			source_id: row.get("source_id")?,
			source: word(row, "source", EventSource::parse)?,
			kind: row.get("kind")?,
			priority: word(row, "priority", EventPriority::parse)?,
			text: row.get("text")?,
			url: row.get("url")?,
			project_id: row.get("project_id")?,
			thread_id: row.get("thread_id")?,
			created_at: row.get("created_at")?,
			raised_at: row.get("raised_at")?,
			reminded_at: row.get("reminded_at")?,
			resolved_at: row.get("resolved_at")?,
		})
	}
}

/// `Date.toISOString()` of `ms` since the epoch.
fn iso(ms: i64) -> String {
	DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
	Utc::now().timestamp_millis()
}

/// The event queue in `catalog.db` (0298): one row per source id. A source id seen again while open changes nothing; one
/// whose cause had gone away and came back opens a fresh row, so it is raised again.
pub struct EventQueue<'a> {
	db: &'a Connection,
	now: Box<dyn Fn() -> i64 + 'a>,
}

impl<'a> EventQueue<'a> {
	/// The queue on `db`, with the real clock.
	pub fn new(db: &'a Connection) -> Self {
		Self { db, now: Box::new(now_ms) }
	}

	/// The queue on `db` with `now` as its clock (ms since the epoch).
	pub fn with_clock(db: &'a Connection, now: impl Fn() -> i64 + 'a) -> Self {
		Self { db, now: Box::new(now) }
	}

	/// Adds an event; returns its row when it is new, `None` when its source id is already open.
	pub fn add(&self, event: &NewEvent) -> Result<Option<EventRow>> {
		if let Some(old) = self.by_source(&event.source_id)? {
			if old.resolved_at.is_none() {
				return Ok(None);
			}
			self.db.execute("DELETE FROM events WHERE id = ?", params![old.id])?;
		}
		let row = self.db.query_row(
			"INSERT INTO events (source_id, source, kind, priority, text, url, project_id, thread_id, created_at)
			 VALUES (:source_id, :source, :kind, :priority, :text, :url, :project_id, :thread_id, :created_at) RETURNING *",
			named_params! {
				":source_id": event.source_id,
				":source": event.source.as_str(),
				":kind": event.kind,
				":priority": event.priority.as_str(),
				":text": event.text,
				":url": event.url,
				":project_id": event.project_id,
				":thread_id": event.thread_id,
				":created_at": iso((self.now)()),
			},
			EventRow::from_row,
		)?;
		Ok(Some(row))
	}

	pub fn by_source(&self, source_id: &str) -> Result<Option<EventRow>> {
		self.query("SELECT * FROM events WHERE source_id = ?", params![source_id]).map(|mut rows| rows.pop())
	}

	/// Open events, oldest first.
	pub fn open(&self) -> Result<Vec<EventRow>> {
		self.query("SELECT * FROM events WHERE resolved_at IS NULL ORDER BY id", params![])
	}

	pub fn mark_raised(&self, id: i64) -> Result<()> {
		self.db.execute("UPDATE events SET raised_at = ? WHERE id = ?", params![iso((self.now)()), id])?;
		Ok(())
	}

	pub fn mark_reminded(&self, id: i64) -> Result<()> {
		self.db.execute("UPDATE events SET reminded_at = ? WHERE id = ?", params![iso((self.now)()), id])?;
		Ok(())
	}

	pub fn resolve(&self, source_id: &str) -> Result<()> {
		self.db.execute(
			"UPDATE events SET resolved_at = ? WHERE source_id = ? AND resolved_at IS NULL",
			params![iso((self.now)()), source_id],
		)?;
		Ok(())
	}

	/// A poller's current set for one source and kinds: everything open of those kinds that isn't in `current` has gone
	/// away (a review done, a pipeline green again), but only inside `observed`, the scope the poller actually saw this
	/// round (SHR-8; `None` is everything): an event outside it (a listing that failed or was cut short) stays open.
	/// Returns the new events.
	pub fn sync(
		&self,
		source: EventSource,
		kinds: &[&str],
		current: &[NewEvent],
		observed: Option<&dyn Fn(&str) -> bool>,
	) -> Result<Vec<EventRow>> {
		let seen: HashSet<&str> = current.iter().map(|event| event.source_id.as_str()).collect();
		for row in self.open()? {
			if row.source != source || !kinds.contains(&row.kind.as_str()) || seen.contains(row.source_id.as_str()) {
				continue;
			}
			if observed.is_none_or(|observed| observed(&row.source_id)) {
				self.resolve(&row.source_id)?;
			}
		}
		let mut added = Vec::new();
		for event in current {
			added.extend(self.add(event)?);
		}
		Ok(added)
	}

	/// Raised, unresolved urgent and normal events 30 minutes old or more, not yet reminded (0303); `after_ms` is the age.
	pub fn due_reminders(&self, after_ms: i64) -> Result<Vec<EventRow>> {
		self.query(
			"SELECT * FROM events WHERE resolved_at IS NULL AND raised_at IS NOT NULL AND reminded_at IS NULL
			 AND priority IN ('urgent','normal') AND raised_at <= ? ORDER BY id",
			params![iso((self.now)() - after_ms)],
		)
	}

	fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<EventRow>> {
		let mut statement = self.db.prepare_cached(sql)?;
		let rows = statement.query_map(params, EventRow::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
		Ok(rows)
	}
}

#[cfg(test)]
mod tests {
	use std::cell::Cell;

	use chrono::TimeZone;

	use super::*;

	/// `catalog.db`'s events table, v5 (the TypeScript's `CATALOG_MIGRATIONS[4]`, `shared/schema.ts`).
	fn catalog() -> Connection {
		let db = Connection::open_in_memory().unwrap();
		db.execute_batch(
			"CREATE TABLE events (
  id          INTEGER PRIMARY KEY,
  source_id   TEXT NOT NULL UNIQUE,
  source      TEXT NOT NULL CHECK (source IN ('github','azure','calendar','thread','machine','inbox')),
  kind        TEXT NOT NULL,
  priority    TEXT NOT NULL CHECK (priority IN ('urgent','normal','low')),
  text        TEXT NOT NULL,
  url         TEXT,
  project_id  TEXT,
  thread_id   TEXT,
  created_at  TEXT NOT NULL,
  raised_at   TEXT,
  reminded_at TEXT,
  resolved_at TEXT
) STRICT;
CREATE INDEX events_open ON events(resolved_at, priority, id);",
		)
		.unwrap();
		db
	}

	fn review(n: u32) -> NewEvent {
		NewEvent {
			source_id: format!("github:review:acme/api#{n}"),
			source: EventSource::Github,
			kind: "review".to_owned(),
			priority: EventPriority::Normal,
			text: format!("review requested: change {n} (acme/api #{n})"),
			url: Some(format!("https://github.com/acme/api/pull/{n}")),
			project_id: None,
			thread_id: None,
		}
	}

	fn ids(rows: &[EventRow]) -> Vec<&str> {
		rows.iter().map(|row| row.source_id.as_str()).collect()
	}

	#[test]
	fn the_queue_deduplicates_by_source_id_a_pollers_set_resolves_what_went_away_a_cause_that_comes_back_is_new() {
		let db = catalog();
		let now = Cell::new(Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap().timestamp_millis());
		let queue = EventQueue::with_clock(&db, || now.get());
		let kinds = ["review", "pipeline"];
		assert!(queue.add(&review(1)).unwrap().is_some());
		assert_eq!(queue.add(&review(1)).unwrap(), None);
		let added = queue.sync(EventSource::Github, &kinds, &[review(1), review(2)], None).unwrap();
		assert_eq!(ids(&added), ["github:review:acme/api#2"]);
		// Review 1 is done: it leaves the set and is resolved.
		assert_eq!(queue.sync(EventSource::Github, &["review"], &[review(2)], None).unwrap(), vec![]);
		assert_eq!(ids(&queue.open().unwrap()), ["github:review:acme/api#2"]);
		assert!(queue.by_source("github:review:acme/api#1").unwrap().unwrap().resolved_at.is_some());
		// Requested again later: a fresh event, so it is raised again.
		assert_eq!(queue.sync(EventSource::Github, &["review"], &[review(1), review(2)], None).unwrap().len(), 1);

		// The one reminder (0303): raised, unresolved, 30 minutes old, not yet reminded.
		let id = queue.by_source("github:review:acme/api#2").unwrap().unwrap().id;
		queue.mark_raised(id).unwrap();
		assert_eq!(queue.due_reminders(30 * 60_000).unwrap(), vec![]);
		now.set(now.get() + 30 * 60_000);
		assert_eq!(queue.due_reminders(30 * 60_000).unwrap().iter().map(|row| row.id).collect::<Vec<_>>(), [id]);
		queue.mark_reminded(id).unwrap();
		assert_eq!(queue.due_reminders(30 * 60_000).unwrap(), vec![]);
	}

	#[test]
	fn a_pollers_set_resolves_only_what_went_away_inside_the_scope_it_observed() {
		let db = catalog();
		let queue = EventQueue::new(&db);
		for n in [1, 2, 3] {
			queue.add(&review(n)).unwrap();
		}
		// Review 1 lies outside this round's scope: it stays open though missing; 2 went away; 3 is still there.
		let observed = |source_id: &str| source_id != "github:review:acme/api#1";
		let added = queue.sync(EventSource::Github, &["review"], &[review(3)], Some(&observed)).unwrap();
		assert_eq!(added, vec![]);
		assert_eq!(ids(&queue.open().unwrap()), ["github:review:acme/api#1", "github:review:acme/api#3"]);
	}

	#[test]
	fn a_row_keeps_its_fields_and_only_the_sources_own_kinds_resolve() {
		let db = catalog();
		let now = Cell::new(Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap().timestamp_millis());
		let queue = EventQueue::with_clock(&db, || now.get());
		let row = queue.add(&review(7)).unwrap().unwrap();
		assert_eq!(
			(row.source, row.priority, row.url.as_deref(), row.project_id, row.created_at.as_str()),
			(
				EventSource::Github,
				EventPriority::Normal,
				Some("https://github.com/acme/api/pull/7"),
				None,
				"2026-10-08T09:00:00.000Z"
			)
		);
		// Another source's events, and kinds the poller doesn't own, are left alone.
		let mut machine = review(8);
		machine.source = EventSource::Machine;
		machine.source_id = "machine:disk".to_owned();
		queue.add(&machine).unwrap();
		queue.sync(EventSource::Github, &["pipeline"], &[], None).unwrap();
		assert_eq!(ids(&queue.open().unwrap()), ["github:review:acme/api#7", "machine:disk"]);
		queue.sync(EventSource::Github, &["review"], &[], None).unwrap();
		assert_eq!(ids(&queue.open().unwrap()), ["machine:disk"]);
	}

	#[test]
	fn a_new_event_is_json_with_camel_case_keys() {
		let event = serde_json::to_value(review(1)).unwrap();
		assert_eq!(event["sourceId"], "github:review:acme/api#1");
		assert_eq!(event["priority"], "normal");
		assert!(event.get("projectId").is_none());
	}
}
