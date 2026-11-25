//! The session watcher (0389, 0400): saves the compaction and handoff summaries of his omp sessions to `catalog.db`
//! `summaries` and puts the lines of those from sessions under a watched project into the project's history (0603).
//!
//! Each session file is read from where the last scan stopped (CR-03): `<data>/watcher.json` keeps `lastScan` and, per
//! file, the byte offset after its last complete line, its cwd and its identity (`ino`, `birthtimeMs`): omp ≥ 18.10.4
//! rewrites a session whole through a temp file and a rename (discards, retry marks, shake), possibly no shorter than
//! the offset, so a file whose identity changed is read again from its start, and the summaries' key absorbs the
//! re-read (SRV-B-3). The history gets the summaries of `handoff` and `soft` compactions (and legacy ones without a
//! method), as 0400 decided; `remote` and `snapcompact` ones stay in `summaries` only (stored as already indexed). A
//! snapcompact transcript archive ([`is_transcript_archive`]) never goes into the history whatever its method, and one
//! already waiting in `summaries` is marked indexed without lines. A summary keeps the watched project its cwd
//! resolved to (`project_id`, 0440 (4)), so it is indexed even when that folder is gone; one whose project is still
//! unknown waits. A scan lets the service's other work run every few milliseconds (PERF-3). The state stays in memory
//! after the first read and is written when the set of files changed, else at most every 5 minutes while offsets move,
//! and at stop: a lost offset only re-reads a tail, which INSERT OR IGNORE absorbs.
//!
//! The scan is polling (an interval, 30 s by default), as the TypeScript's: nothing watches the folder.
//!
//! The omp shapes this relies on (checked against omp 19.2.0's `pi-coding-agent/src/session/`, 0663, 0690):
//! - layout: a session is `<sessions>/<cwd dir>/<stem>.jsonl` (`session-paths.ts:223` `sessionDirForCwd`); its subagents
//!   live in `<sessions>/<cwd dir>/<stem>/<AgentId>.jsonl` beside `__advisor*.jsonl` (`sub-sessions.ts:4-7`), so only
//!   files directly in a cwd dir are sessions.
//! - header: `{type: "session", version, id, timestamp, cwd, …}` (`session-entries.ts:68-102`); since the title slot
//!   (`{type: "title", v: 1, …}`, `session-entries.ts:30-38`) it is the file's second line, not its first.
//! - compaction: `{type: "compaction", id, parentId, timestamp, summary, shortSummary?, firstKeptEntryId, tokensBefore,
//!   method?, …}` (`session-entries.ts:255-279`); the text is `summary`. `method` is one of `remote`, `snapcompact`,
//!   `handoff`, `soft`, `shake` (`compaction-methods.ts:11-39`), absent on legacy and extension compactions; `/handoff`
//!   writes one with `method: "handoff"` into the same session (`compact(focus, {mode: "handoff"})`, `compact-modes.ts:62`,
//!   `session-maintenance.ts:1818`; 19.1.0 replaced `session.handoff`).
//! - message: `{type: "message", id, parentId, timestamp, message}` (`session-entries.ts:140-143`); an assistant message
//!   has `role: "assistant"`, `content` parts (`text` / `thinking` / `toolCall`) and `stopReason` `stop | length |
//!   toolUse | error | aborted` (pi-ai `types.ts:1114`), and is written once, at `message_end` (`agent-session.ts:4743`
//!   `#persistMessageEnd`).
//! - moved session (18.12.6, 0464): when another process wrote a session first, omp goes on in a sibling file whose
//!   header names the old session as `parentSession` and publishes the whole transcript there once
//!   (`session-manager.ts` `#moveOffSessionFile`); its compactions already stored from the old file are skipped. The
//!   service follows the boss's session there (`SessionIndex.moved`). `model_usage` and `memory_recall` entries hold
//!   no summary and are passed over.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};
use tokio::task::JoinHandle;

use crate::frontier::protocol::js_number_text;
use crate::frontier::server::history::{HistoryEntry, add_history};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, write_atomically};
use crate::shared::log::{LogLevel, log};
use crate::shared::omp_session::{
	FileStat, ReadMark, SessionTail, SessionWalker, TailVisitor, date_parse, read_header, replaced, rewritten,
};

/// A scan holds the runtime at most about this long before it lets other work run (PERF-3).
const SLICE: Duration = Duration::from_millis(8);

/// watcher.json is rewritten at most this often while only offsets move.
const STATE_WRITE_MS: i64 = 5 * 60_000;

/// The scan interval when none is given.
const DEFAULT_INTERVAL_MS: u64 = 30_000;

/// Clock, in epoch ms.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// What the watcher does with a scan's failure.
pub type OnError = Arc<dyn Fn(&Error) + Send + Sync>;

/// The watched project holding `cwd`, if any: its id.
pub type ProjectFor = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct WatcherOptions {
	/// omp's sessions dir, `~/.omp/agent/sessions`.
	pub sessions_dir: PathBuf,
	/// `<data>/watcher.json`: `{lastScan, files: {<session file>: {offset, cwd, ino, birthtimeMs}}}`.
	pub state_file: PathBuf,
	/// `catalog.db` at v9 or later.
	pub db: Arc<Mutex<Connection>>,
	/// The watched project holding `cwd`, if any (its id).
	pub project_for: ProjectFor,
	/// Scan interval, default 30 s.
	pub interval_ms: Option<u64>,
	pub now: Option<Clock>,
	/// Default: a `warn` line in the log under `watcher`.
	pub on_error: Option<OnError>,
}

/// One session file's place: the offset after its last complete line, its cwd and its identity when read.
#[derive(Debug, Clone, PartialEq)]
struct FileState {
	offset: u64,
	cwd: Option<String>,
	ino: Option<u64>,
	birthtime_ms: Option<f64>,
}

impl FileState {
	fn mark(&self) -> ReadMark {
		ReadMark { offset: self.offset, ino: self.ino, birthtime_ms: self.birthtime_ms }
	}
}

/// The files of a state, in the order `watcher.json` lists them.
#[derive(Debug, Default)]
struct Files {
	list: Vec<(String, FileState)>,
	index: HashMap<String, usize>,
}

impl Files {
	fn get(&self, name: &str) -> Option<&FileState> {
		self.index.get(name).map(|&at| &self.list[at].1)
	}

	fn set(&mut self, name: String, state: FileState) {
		match self.index.get(&name) {
			Some(&at) => self.list[at].1 = state,
			None => {
				self.index.insert(name.clone(), self.list.len());
				self.list.push((name, state));
			}
		}
	}

	/// Whether two states hold the same files.
	fn same_names(&self, other: &Files) -> bool {
		self.list.len() == other.list.len() && self.list.iter().all(|(name, _)| other.index.contains_key(name))
	}

	/// Whether two states hold the same files with the same offsets, cwds and identities.
	fn same_files(&self, other: &Files) -> bool {
		self.same_names(other) && self.list.iter().all(|(name, mine)| other.get(name) == Some(mine))
	}
}

/// What `watcher.json` holds.
#[derive(Debug, Default)]
struct WatcherState {
	last_scan: f64,
	files: Files,
}

/// A compaction entry with summary text, as found in a session file.
struct Found {
	entry_id: String,
	kind: &'static str,
	cwd: String,
	at: SqlValue,
	summary: String,
	indexable: bool,
}

/// A summary waiting to be indexed (`summaries` without its text).
struct Pending {
	session_file: String,
	entry_id: String,
	kind: String,
	cwd: String,
	at: i64,
	project_id: Option<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// JavaScript's `trim`: white space and line terminators, with the byte order mark and without U+0085.
fn js_trim(text: &str) -> &str {
	text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// Whether a compaction's text is a snapcompact transcript archive (omp's `snapcompact/src/prompts/snapcompact-summary.md`:
/// "Resume prior conversation. Earlier turns archived under HISTORY …", a scope legend, a file list and a `HISTORY`
/// section whose turns are mostly images) rather than a written summary: it holds no lines worth searching. Rows of
/// this shape were stored before 0400's method filter (CR-23).
fn is_transcript_archive(summary: &str) -> bool {
	summary.starts_with("Resume prior conversation. Earlier turns archived under HISTORY")
		&& (summary.contains("\nHISTORY\n") || summary.contains("\nHISTORY\r\n"))
}

/// `stat`, as the TypeScript's number: an inode past 2^53 is the double nearest to it, in memory and in `watcher.json`
/// alike.
fn stat_of(path: &Path) -> Option<FileStat> {
	let mut stat = FileStat::of(path)?;
	stat.ino = (stat.ino as f64) as u64;
	Some(stat)
}

/// `JSON.stringify(state)` and a newline.
fn state_json(state: &WatcherState) -> String {
	let quoted = |text: &str| serde_json::to_string(text).unwrap_or_default();
	let mut out = format!("{{\"lastScan\":{},\"files\":{{", js_number_text(state.last_scan));
	for (at, (name, file)) in state.files.list.iter().enumerate() {
		if at > 0 {
			out.push(',');
		}
		out.push_str(&format!("{}:{{\"offset\":{}", quoted(name), file.offset));
		if let Some(cwd) = &file.cwd {
			out.push_str(&format!(",\"cwd\":{}", quoted(cwd)));
		}
		if let Some(ino) = file.ino {
			out.push_str(&format!(",\"ino\":{}", js_number_text(ino as f64)));
		}
		if let Some(birthtime) = file.birthtime_ms {
			out.push_str(&format!(",\"birthtimeMs\":{}", js_number_text(birthtime)));
		}
		out.push('}');
	}
	out.push_str("}}\n");
	out
}

/// The state file; a missing or unreadable one means scan everything.
fn read_state(state_file: &Path) -> WatcherState {
	let value = read_json_object(state_file);
	let last_scan = value.as_ref().and_then(|v| v.get("lastScan")).and_then(Value::as_f64).filter(|v| v.is_finite());
	let mut files = Files::default();
	let listed = value.as_ref().and_then(|v| v.get("files")).and_then(Value::as_object);
	for (file, entry) in listed.into_iter().flatten() {
		let Some(entry) = entry.as_object() else { continue };
		let Some(offset) = entry.get("offset").and_then(Value::as_f64).filter(|offset| *offset >= 0.0) else {
			continue;
		};
		let ino = entry.get("ino").and_then(Value::as_f64);
		let birthtime_ms = entry.get("birthtimeMs").and_then(Value::as_f64);
		let identity = ino.zip(birthtime_ms);
		files.set(
			file.clone(),
			FileState {
				offset: offset as u64,
				cwd: entry.get("cwd").and_then(Value::as_str).map(str::to_owned),
				ino: identity.map(|(ino, _)| ino as u64),
				birthtime_ms: identity.map(|(_, birthtime)| birthtime),
			},
		);
	}
	WatcherState { last_scan: last_scan.unwrap_or(0.0), files }
}

/// The state a scan records for `file` when it need not read it, or `None` when it must (read from `known`'s
/// offset, or from the start). A file untouched since the last scan keeps its offset, with its identity refreshed; one
/// unknown to a state that predates offsets was read whole then, so it is followed from its end.
fn unread_state(
	file: &Path,
	stat: &FileStat,
	previous: &WatcherState,
	known: Option<&FileState>,
) -> Result<Option<FileState>> {
	// Another file took its name (a rewrite through a rename); a state from before identities were kept can't say.
	if stat.mtime_ms >= previous.last_scan || known.is_some_and(|known| replaced(stat, &known.mark())) {
		return Ok(None);
	}
	let (ino, birthtime_ms) = (Some(stat.ino), Some(stat.birthtime_ms));
	if let Some(known) = known {
		if rewritten(stat, &known.mark()) {
			return Ok(None);
		}
		return Ok(Some(FileState { ino, birthtime_ms, ..known.clone() }));
	}
	if previous.last_scan > 0.0 {
		// Read whole by a scan before offsets were kept: it is in `summaries`; follow it from its end.
		let cwd = read_header(file)?.map(|header| header.cwd).filter(|cwd| !cwd.is_empty());
		return Ok(Some(FileState { offset: stat.size, cwd, ino, birthtime_ms }));
	}
	Ok(None)
}

/// A cwd's project, resolved once per scan.
struct Projects {
	resolve: ProjectFor,
	seen: HashMap<String, Option<String>>,
}

impl Projects {
	fn of(&mut self, cwd: &str) -> Option<String> {
		if let Some(id) = self.seen.get(cwd) {
			return id.clone();
		}
		let id = (self.resolve)(cwd);
		self.seen.insert(cwd.to_owned(), id.clone());
		id
	}
}

/// A JavaScript number as SQLite gets it from Bun: an integer when it is one, else a float (a STRICT `INTEGER` column
/// refuses the float, as in the TypeScript).
fn js_sql_number(value: f64) -> SqlValue {
	if value.fract() == 0.0 && value.abs() < 9.0e15 { SqlValue::Integer(value as i64) } else { SqlValue::Real(value) }
}

/// Collects the compaction entries with text of one file range.
struct Collector {
	cwd: Option<String>,
	rows: Vec<Found>,
	moved: bool,
	mtime_ms: f64,
}

impl TailVisitor for Collector {
	fn entry(&mut self, entry: Map<String, Value>) {
		let kind = entry.get("type").and_then(Value::as_str);
		let Some(cwd) = &self.cwd else {
			if kind == Some("session")
				&& let Some(cwd) = entry.get("cwd").and_then(Value::as_str)
			{
				self.cwd = Some(cwd.to_owned());
				self.moved = entry.get("parentSession").is_some_and(Value::is_string);
			}
			return;
		};
		let Some(id) = entry.get("id").and_then(Value::as_str).filter(|_| kind == Some("compaction")) else { return };
		let summary = js_trim(entry.get("summary").and_then(Value::as_str).unwrap_or(""));
		if summary.is_empty() {
			return; // shake, and snapcompact or remote without text
		}
		let at = entry.get("timestamp").and_then(Value::as_str).and_then(date_parse);
		let method = entry.get("method").and_then(Value::as_str);
		self.rows.push(Found {
			entry_id: id.to_owned(),
			kind: if method == Some("handoff") { "handoff" } else { "compact" },
			cwd: cwd.clone(),
			at: at.map_or_else(|| js_sql_number(self.mtime_ms), SqlValue::Integer),
			summary: summary.to_owned(),
			// Compaction methods whose summaries are indexed (0400); legacy entries carry none.
			indexable: matches!(method, None | Some("handoff" | "soft")) && !is_transcript_archive(summary),
		});
	}

	fn reset(&mut self) {
		// Read once from `from`, which the scan judged against a rewrite already.
	}
}

/// What reading a file's range found.
struct Read {
	rows: Vec<Found>,
	offset: u64,
	cwd: Option<String>,
	moved: bool,
}

/// The compaction entries with summary text in `file` between byte `from` and `to`, each with whether its lines go into
/// the history; the offset after the last complete line; the session's cwd (from its header, or `known_cwd` when the
/// header was read before); whether its header (read in this range) names a `parentSession`.
fn summaries_in(file: &Path, from: u64, to: u64, known_cwd: Option<&String>, mtime_ms: f64) -> Result<Read> {
	let collector = Collector { cwd: known_cwd.cloned(), rows: Vec::new(), moved: false, mtime_ms };
	let mut tail = SessionTail::new(file.to_path_buf(), collector);
	tail.mark.offset = from;
	// A line still being written isn't complete; the next scan reads it.
	let offset = tail.read_to(to)?;
	let Collector { rows, cwd, moved, .. } = tail.visitor;
	Ok(Read { rows, offset, cwd, moved })
}

struct Inner {
	sessions: Mutex<SessionWalker>,
	state_file: PathBuf,
	db: Arc<Mutex<Connection>>,
	project_for: ProjectFor,
	now: Clock,
	on_error: OnError,
	stopped: AtomicBool,
	/// One scan at a time; a call during a scan runs another after it, so it sees files written before the call.
	scanning: Arc<tokio::sync::Mutex<()>>,
	memory: Mutex<Memory>,
}

/// `watcher.json` as last read or written, with whether it holds offsets the file doesn't yet, and when it was last
/// written (`now()`).
#[derive(Default)]
struct Memory {
	state: Option<Arc<WatcherState>>,
	unsaved: bool,
	saved_at: Option<i64>,
}

impl Inner {
	fn save(&self, memory: &mut Memory) -> Result<()> {
		let Some(state) = memory.state.clone().filter(|_| memory.unsaved) else { return Ok(()) };
		write_atomically(&self.state_file, state_json(&state), None)?;
		memory.unsaved = false;
		memory.saved_at = Some((self.now)());
		Ok(())
	}

	/// Reads `file` from where `known` left off (from its start when it was rewritten), stores the summaries found, and
	/// returns the file's new state. A moved session's copies of compactions the old file gave are dropped.
	fn read_and_store(
		&self,
		path: &Path,
		stat: &FileStat,
		known: Option<&FileState>,
		projects: &mut Projects,
	) -> Result<FileState> {
		// A file that was rewritten (replaced, or shorter than the offset) is read again from its start.
		let from = known.filter(|known| !rewritten(stat, &known.mark()));
		let read = summaries_in(
			path,
			from.map_or(0, |from| from.offset),
			stat.size,
			from.and_then(|from| from.cwd.as_ref()),
			stat.mtime_ms,
		)?;
		let file = path.to_string_lossy().into_owned();
		// Projects before the lock: the service names them through its places, which read this same catalog, and a
		// std mutex taken twice on one thread waits on itself (the hang of 2026-11-23).
		let found: Vec<(Found, Option<String>)> = read
			.rows
			.into_iter()
			.map(|found| {
				let project = projects.of(&found.cwd);
				(found, project)
			})
			.collect();
		let db = lock(&self.db);
		let mut rows = Vec::new();
		for (found, project) in found {
			if read.moved && stored_elsewhere(&db, &found, &file)? {
				continue;
			}
			rows.push((found, project));
		}
		if !rows.is_empty() {
			let tx = db.unchecked_transaction()?;
			{
				let mut insert = tx.prepare_cached(
					"INSERT OR IGNORE INTO summaries (session_file, entry_id, kind, cwd, at, summary, indexed, project_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
				)?;
				for (found, project) in rows {
					insert.execute(rusqlite::params![
						file,
						found.entry_id,
						found.kind,
						found.cwd,
						found.at,
						found.summary,
						i64::from(!found.indexable),
						project
					])?;
				}
			}
			tx.commit()?;
		}
		Ok(FileState { offset: read.offset, cwd: read.cwd, ino: Some(stat.ino), birthtime_ms: Some(stat.birthtime_ms) })
	}

	/// The state a scan records for one file, or `None` when it is gone.
	fn scan_file(
		&self,
		path: &Path,
		previous: &WatcherState,
		known: Option<&FileState>,
		projects: &mut Projects,
	) -> Result<Option<FileState>> {
		let Some(stat) = stat_of(path) else { return Ok(None) };
		if let Some(unread) = unread_state(path, &stat, previous, known)? {
			return Ok(Some(unread));
		}
		self.read_and_store(path, &stat, known, projects).map(Some)
	}

	/// Puts the lines of each stored summary whose project is known into the history, once (0603).
	/// Projects are named between the catalog's locks, for the reason `read_and_store` gives.
	fn index_pending(&self, projects: &mut Projects) -> Result<()> {
		// Without `summary`: a backlog outside any watched project is listed every scan, its text is loaded only to index.
		let pending = lock(&self.db)
			.prepare_cached(
				"SELECT session_file, entry_id, kind, cwd, at, project_id FROM summaries WHERE indexed = 0 ORDER BY at, session_file, entry_id",
			)?
			.query_map([], |row| {
				Ok(Pending {
					session_file: row.get(0)?,
					entry_id: row.get(1)?,
					kind: row.get(2)?,
					cwd: row.get(3)?,
					at: row.get(4)?,
					project_id: row.get(5)?,
				})
			})?
			.collect::<std::result::Result<Vec<_>, _>>()?;
		for row in pending {
			let project_id = row.project_id.clone().or_else(|| projects.of(&row.cwd));
			let Some(project_id) = project_id.filter(|id| !id.is_empty()) else { continue };
			let db = lock(&self.db);
			let summary: Option<String> = db
				.prepare_cached("SELECT summary FROM summaries WHERE session_file = ? AND entry_id = ?")?
				.query_row([&row.session_file, &row.entry_id], |found| found.get(0))
				.optional()?;
			let Some(summary) = summary else { continue };
			index_one(&db, &row, &project_id, &summary)?;
		}
		Ok(())
	}

	/// The state as last read or written; read at the first scan.
	fn previous_state(&self) -> Arc<WatcherState> {
		let mut memory = lock(&self.memory);
		memory.state.get_or_insert_with(|| Arc::new(read_state(&self.state_file))).clone()
	}

	/// Every session file once, then the index and the state. `Ok` also when stopped midway.
	async fn run_scan(&self) -> Result<()> {
		let started = (self.now)();
		let previous = self.previous_state();
		let mut files = Files::default();
		let mut projects = Projects { resolve: self.project_for.clone(), seen: HashMap::new() };
		let listed = lock(&self.sessions).files();
		let mut slice = Instant::now();
		for path in listed {
			if self.stopped.load(Ordering::SeqCst) {
				return Ok(());
			}
			// Let the service's other work run every few milliseconds, unchanged files included (a first scan reads every
			// session once; a steady one stats them all).
			if slice.elapsed() >= SLICE {
				tokio::task::yield_now().await;
				slice = Instant::now();
				if self.stopped.load(Ordering::SeqCst) {
					return Ok(());
				}
			}
			let file = path.to_string_lossy().into_owned();
			let known = previous.files.get(&file);
			match self.scan_file(&path, &previous, known, &mut projects) {
				Ok(Some(state)) => files.set(file, state),
				Ok(None) => {}
				Err(error) => {
					if let Some(known) = known {
						files.set(file, known.clone());
					}
					(self.on_error)(&error);
				}
			}
		}
		self.index_pending(&mut projects)?;
		let state = Arc::new(WatcherState { last_scan: started as f64, files });
		let mut memory = lock(&self.memory);
		memory.state = Some(state.clone());
		if !previous.files.same_files(&state.files) {
			memory.unsaved = true;
		}
		// A new or gone file is written at once; moved offsets at most every 5 minutes (and at stop).
		let now = (self.now)();
		let due = memory.saved_at.is_none_or(|saved| now - saved >= STATE_WRITE_MS || now < saved);
		if memory.unsaved && (!previous.files.same_names(&state.files) || due) {
			self.save(&mut memory)?;
		}
		Ok(())
	}

	async fn scan(&self) -> Result<()> {
		let _scanning = self.scanning.lock().await;
		self.run_scan().await
	}
}

/// Whether a moved session's compaction was given already by another file (same entry id, same cwd).
fn stored_elsewhere(db: &Connection, found: &Found, file: &str) -> Result<bool> {
	let hit: Option<i64> = db
		.prepare_cached("SELECT 1 AS one FROM summaries WHERE entry_id = ? AND cwd = ? AND session_file <> ? LIMIT 1")?
		.query_row([&found.entry_id, &found.cwd, file], |row| row.get(0))
		.optional()?;
	Ok(hit.is_some())
}

/// Starts a scan unless one is running or the watcher is stopped; the scan holds the scan lock from this call on, so one
/// asked for right after runs behind it.
fn tick(inner: &Arc<Inner>) {
	if inner.stopped.load(Ordering::SeqCst) {
		return;
	}
	let Ok(guard) = inner.scanning.clone().try_lock_owned() else { return };
	let inner = inner.clone();
	tokio::spawn(async move {
		let _scanning = guard;
		if let Err(error) = inner.run_scan().await {
			(inner.on_error)(&error);
		}
	});
}

/// The running watcher: [`Watcher::scan`] and [`Watcher::stop`].
pub struct Watcher {
	inner: Arc<Inner>,
	timer: Mutex<Option<JoinHandle<()>>>,
}

impl Watcher {
	/// One scan now, after any running one: a call during a scan runs another after it.
	pub async fn scan(&self) -> Result<()> {
		self.inner.scan().await
	}

	/// Stops the timer and any scan in flight at its next file, and writes the state if offsets moved.
	pub fn stop(&self) {
		self.inner.stopped.store(true, Ordering::SeqCst);
		if let Some(timer) = lock(&self.timer).take() {
			timer.abort();
		}
		let mut memory = lock(&self.inner.memory);
		if let Err(error) = self.inner.save(&mut memory) {
			(self.inner.on_error)(&error);
		}
	}
}

impl Drop for Watcher {
	fn drop(&mut self) {
		if let Some(timer) = lock(&self.timer).take() {
			timer.abort();
		}
	}
}

/// Starts the watcher: a first scan now and one every `interval_ms`. Needs a tokio runtime.
pub fn start_watcher(opts: WatcherOptions) -> Watcher {
	let on_error = opts
		.on_error
		.unwrap_or_else(|| Arc::new(|error: &Error| log(LogLevel::Warn, "watcher", &error.to_string(), None)));
	let inner = Arc::new(Inner {
		sessions: Mutex::new(SessionWalker::new(opts.sessions_dir)),
		state_file: opts.state_file,
		db: opts.db,
		project_for: opts.project_for,
		now: opts.now.unwrap_or_else(|| Arc::new(|| chrono::Utc::now().timestamp_millis())),
		on_error,
		stopped: AtomicBool::new(false),
		scanning: Arc::new(tokio::sync::Mutex::new(())),
		memory: Mutex::new(Memory::default()),
	});
	tick(&inner);
	let period = Duration::from_millis(opts.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS).max(1));
	let timer = tokio::spawn({
		let inner = inner.clone();
		async move {
			let mut every = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
			every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
			loop {
				every.tick().await;
				tick(&inner);
			}
		}
	});
	Watcher { inner, timer: Mutex::new(Some(timer)) }
}

/// One stored summary's lines and its indexed mark, in one transaction: both or neither (0603).
fn index_one(db: &Connection, row: &Pending, project_id: &str, summary: &str) -> Result<()> {
	let tx = db.unchecked_transaction()?;
	if !is_transcript_archive(summary) {
		let stem = Path::new(&row.session_file).file_name().map_or_else(String::new, |name| {
			let name = name.to_string_lossy();
			name.strip_suffix(".jsonl").unwrap_or(&name).to_owned()
		});
		let source = format!("{} {stem}", row.kind);
		add_history(&tx, project_id, &HistoryEntry { source: &source, at: row.at, text: summary })?;
	}
	// The project is kept with the mark, so a summary is indexed under it after its cwd folder is gone (0440 (4)).
	tx.prepare_cached("UPDATE summaries SET indexed = 1, project_id = ? WHERE session_file = ? AND entry_id = ?")?
		.execute([project_id, &row.session_file, &row.entry_id])?;
	tx.commit()?;
	Ok(())
}
#[cfg(test)]
#[path = "watcher_tests.rs"]
mod tests;
