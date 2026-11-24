//! The journal (0688): one append-only file of the orchestrator's events, `<data>/journal.jsonl`, a JSON line each:
//! `{"at": <ms>, "kind": …, "project": <id>, "thread": <id, when one>, …fields}`. The weekly numbers read from it.
//! No prompt text and no private text goes in: the fields are counts, names of gates, outcomes and ids. A write that
//! fails is logged and never fails the caller.

use std::io::Write;
use std::sync::{Mutex, PoisonError};

use serde_json::{Map, Value};

use crate::frontier::server::thread::Thread;
use crate::shared::clock::now_ms;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;

/// The journal's file name in the data folder.
pub const JOURNAL_FILE: &str = "journal.jsonl";

/// Who an event is about: the project, and the thread when there is one.
#[derive(Debug, Clone, Copy, Default)]
pub struct About<'a> {
	pub project: &'a str,
	pub thread: Option<&'a str>,
}

impl<'a> About<'a> {
	/// An event about the project alone.
	pub fn project(project: &'a str) -> Self {
		Self { project, thread: None }
	}
}

/// The thread's project id and task id, as the journal names them.
pub fn about_thread(thread: &Thread) -> (String, Option<String>) {
	(thread.project.id.clone(), thread.task.as_ref().map(|task| task.id.clone()))
}

/// Appends events; one writer at a time, so two lines never interleave.
#[derive(Default)]
pub struct Journal {
	writing: Mutex<()>,
}

impl Journal {
	/// Appends one line of `kind` about `about`, with `fields` (a JSON object; anything else adds nothing). A write
	/// failure is logged, never returned.
	pub fn append(&self, kind: &str, about: About<'_>, fields: Value) {
		let mut line = Map::new();
		line.insert("at".to_owned(), Value::from(now_ms()));
		line.insert("kind".to_owned(), Value::from(kind));
		line.insert("project".to_owned(), Value::from(about.project));
		if let Some(thread) = about.thread {
			line.insert("thread".to_owned(), Value::from(thread));
		}
		if let Value::Object(fields) = fields {
			line.extend(fields);
		}
		let text = format!("{}\n", Value::Object(line));
		let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
		let written = std::fs::create_dir_all(data_dir())
			.and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join(JOURNAL_FILE)))
			.and_then(|mut file| file.write_all(text.as_bytes()));
		if let Err(error) = written {
			log(LogLevel::Warn, "journal", &format!("a {kind} line wasn't written: {error}"), None);
		}
	}

	/// [`Journal::append`] about `thread`.
	pub fn thread(&self, kind: &str, thread: &Thread, fields: Value) {
		let (project, task) = about_thread(thread);
		self.append(kind, About { project: &project, thread: task.as_deref() }, fields);
	}
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
