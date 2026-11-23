use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{Value, json};

use super::*;
use crate::frontier::server::history::search_history;
use crate::shared::paths::data_dir;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

// Synthetic lines in omp 18.10.3's session shapes (see the header of watcher.rs).
const PROJECT: &str = "C:\\work\\demo";
const OUTSIDE: &str = "C:\\scratch";

fn now_ms() -> i64 {
	date_parse("2026-10-13T12:00:00Z").unwrap()
}

fn title_slot(title: &str) -> String {
	json!({ "type": "title", "v": 1, "title": title, "source": "auto", "updatedAt": "2026-10-13T10:00:00.000Z", "pad": "" })
		.to_string()
}

fn header(id: &str, cwd: &str) -> String {
	json!({ "type": "session", "version": 3, "id": id, "timestamp": "2026-10-13T10:00:00.000Z", "cwd": cwd, "title": "demo" })
		.to_string()
}

fn user(id: &str, text: &str) -> String {
	json!({
		"type": "message",
		"id": id,
		"parentId": null,
		"timestamp": "2026-10-13T10:01:00.000Z",
		"message": { "role": "user", "content": [{ "type": "text", "text": text }], "timestamp": now_ms() },
	})
	.to_string()
}

fn assistant(id: &str, content: Value, stop_reason: &str) -> String {
	json!({
		"type": "message",
		"id": id,
		"parentId": "u1",
		"timestamp": "2026-10-13T10:02:00.000Z",
		"message": {
			"role": "assistant",
			"content": content,
			"api": "anthropic-messages",
			"provider": "anthropic",
			"model": "model-a",
			"usage": { "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 2 },
			"stopReason": stop_reason,
			"timestamp": now_ms(),
		},
	})
	.to_string()
}

fn compaction(id: &str, summary: &str, method: Option<&str>, timestamp: &str) -> String {
	let mut entry = json!({
		"type": "compaction",
		"id": id,
		"parentId": "a1",
		"timestamp": timestamp,
		"summary": summary,
		"firstKeptEntryId": "u1",
		"tokensBefore": 120_000,
		"tokensAfter": 4000,
	});
	if let Some(method) = method {
		entry["method"] = json!(method);
	}
	entry["details"] = json!({ "readFiles": [], "modifiedFiles": [] });
	entry["fromExtension"] = json!(false);
	entry.to_string()
}

fn set_mtime(file: &Path, ms: i64) {
	let file = fs::OpenOptions::new().write(true).open(file).unwrap();
	file.set_modified(UNIX_EPOCH + Duration::from_millis(u64::try_from(ms).unwrap())).unwrap();
}

fn write_session(dir: &Path, stem: &str, lines: &[String], mtime: i64) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	let file = dir.join(format!("{stem}.jsonl"));
	fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
	set_mtime(&file, mtime);
	file
}

/// A session written a minute before `now`.
fn session(dir: &Path, stem: &str, lines: &[String]) -> PathBuf {
	write_session(dir, stem, lines, now_ms() - 60_000)
}

fn append(file: &Path, line: &str) {
	let mut handle = fs::OpenOptions::new().append(true).open(file).unwrap();
	writeln!(handle, "{line}").unwrap();
}

#[derive(Debug, PartialEq)]
struct Row {
	session_file: String,
	entry_id: String,
	kind: String,
	cwd: String,
	at: i64,
	summary: String,
	indexed: i64,
}

#[derive(Debug, PartialEq)]
struct Line {
	project_id: String,
	source: String,
	text: String,
	at: i64,
}

struct Setup {
	_j: TempJocasta,
	db: Arc<Mutex<Connection>>,
	sessions_dir: PathBuf,
	state_file: PathBuf,
	clock: Arc<AtomicI64>,
	watcher: Watcher,
}

impl Setup {
	fn with(project_for: ProjectFor) -> Setup {
		Setup::reading(|_| project_for)
	}

	/// A watcher whose `project_for` is made from its catalog, as the service's reads it through its places.
	fn reading(project_for: impl FnOnce(&Arc<Mutex<Connection>>) -> ProjectFor) -> Setup {
		let j = temp_jocasta();
		let db = Arc::new(Mutex::new(open_catalog().unwrap()));
		let project_for = project_for(&db);
		let sessions_dir = j.root().join("sessions");
		let state_file = data_dir().join("watcher.json");
		let clock = Arc::new(AtomicI64::new(now_ms()));
		let watcher = start_watcher(WatcherOptions {
			sessions_dir: sessions_dir.clone(),
			state_file: state_file.clone(),
			db: db.clone(),
			project_for,
			interval_ms: Some(3_600_000),
			now: Some({
				let clock = clock.clone();
				Arc::new(move || clock.load(Ordering::SeqCst))
			}),
			on_error: Some(Arc::new(|_| {})),
		});
		Setup { _j: j, db, sessions_dir, state_file, clock, watcher }
	}

	/// The watched project is `PROJECT` and what lies under it.
	fn new() -> Setup {
		Setup::with(Arc::new(|cwd| cwd.starts_with(PROJECT).then(|| "p-demo".to_owned())))
	}

	fn demo_dir(&self) -> PathBuf {
		self.sessions_dir.join("--C--work--demo--")
	}

	fn rows(&self) -> Vec<Row> {
		let db = lock(&self.db);
		let mut statement = db
			.prepare(
				"SELECT session_file, entry_id, kind, cwd, at, summary, indexed FROM summaries ORDER BY at, entry_id",
			)
			.unwrap();
		statement
			.query_map([], |r| {
				Ok(Row {
					session_file: r.get(0)?,
					entry_id: r.get(1)?,
					kind: r.get(2)?,
					cwd: r.get(3)?,
					at: r.get(4)?,
					summary: r.get(5)?,
					indexed: r.get(6)?,
				})
			})
			.unwrap()
			.map(Result::unwrap)
			.collect()
	}

	/// Everything the history holds, whatever its project.
	fn history(&self) -> Vec<Line> {
		let db = lock(&self.db);
		let mut statement = db.prepare("SELECT project_id, source, text, at FROM history ORDER BY at, text").unwrap();
		statement
			.query_map([], |r| Ok(Line { project_id: r.get(0)?, source: r.get(1)?, text: r.get(2)?, at: r.get(3)? }))
			.unwrap()
			.map(Result::unwrap)
			.collect()
	}

	fn entry_ids(&self) -> Vec<String> {
		let db = lock(&self.db);
		let mut statement = db.prepare("SELECT entry_id, cwd FROM summaries").unwrap();
		statement.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect()
	}

	fn state(&self) -> Value {
		serde_json::from_str(&fs::read_to_string(&self.state_file).unwrap()).unwrap()
	}

	fn offset(&self, file: &Path) -> Option<u64> {
		self.state()["files"][file.to_string_lossy().as_ref()]["offset"].as_u64()
	}

	fn tick_clock(&self, by: i64) -> i64 {
		self.clock.fetch_add(by, Ordering::SeqCst) + by
	}
}

fn at(timestamp: &str) -> i64 {
	date_parse(timestamp).unwrap()
}

fn size(file: &Path) -> u64 {
	fs::metadata(file).unwrap().len()
}

#[tokio::test]
async fn soft_and_handoff_compactions_with_text_become_rows_and_history_lines_shake_without_text_and_subagent_transcripts_dont()
 {
	let s = Setup::new();
	let cwd_dir = s.demo_dir();
	let stem = "2026-10-13T10-00-00-000Z_01a1";
	let file = session(
		&cwd_dir,
		stem,
		&[
			title_slot("demo"),
			header("s1", PROJECT),
			user("u1", "add a README line"),
			assistant("a1", json!([{ "type": "text", "text": "Done." }]), "stop"),
			compaction("c1", "Soft summary of the work done.\nShort.", Some("soft"), "2026-10-13T10:05:00.000Z"),
			compaction("c2", "", Some("shake"), "2026-10-13T10:06:00.000Z"),
			compaction("c3", "Handoff document for the next run.", Some("handoff"), "2026-10-13T10:07:00.000Z"),
			compaction("c4", "Legacy summary of the earlier work.", None, "2026-10-13T10:08:00.000Z"),
		],
	);
	// A subagent's transcript in `<stem>/<AgentId>.jsonl` and an advisor's beside it.
	session(
		&cwd_dir.join(stem),
		"0-Explore",
		&[
			header("s2", PROJECT),
			compaction("x1", "Subagent summary of its search.", Some("soft"), "2026-10-13T10:05:00.000Z"),
		],
	);
	session(
		&cwd_dir.join(stem),
		"__advisor",
		&[
			header("s3", PROJECT),
			compaction("x2", "Advisor summary of the whole run.", Some("soft"), "2026-10-13T10:05:00.000Z"),
		],
	);

	s.watcher.scan().await.unwrap();

	let file_name = file.to_string_lossy().into_owned();
	let row = |id: &str, kind: &str, summary: &str, time: &str| Row {
		session_file: file_name.clone(),
		entry_id: id.into(),
		kind: kind.into(),
		cwd: PROJECT.into(),
		at: at(time),
		summary: summary.into(),
		indexed: 1,
	};
	assert_eq!(
		s.rows(),
		[
			row("c1", "compact", "Soft summary of the work done.\nShort.", "2026-10-13T10:05:00.000Z"),
			row("c3", "handoff", "Handoff document for the next run.", "2026-10-13T10:07:00.000Z"),
			row("c4", "compact", "Legacy summary of the earlier work.", "2026-10-13T10:08:00.000Z"),
		]
	);
	// Only lines long enough to find: the 6-character "Short." stays out.
	let line = |source: String, text: &str, time: &str| Line {
		project_id: "p-demo".into(),
		source,
		text: text.into(),
		at: at(time),
	};
	assert_eq!(
		s.history(),
		[
			line(format!("compact {stem}"), "Soft summary of the work done.", "2026-10-13T10:05:00.000Z"),
			line(format!("handoff {stem}"), "Handoff document for the next run.", "2026-10-13T10:07:00.000Z"),
			line(format!("compact {stem}"), "Legacy summary of the earlier work.", "2026-10-13T10:08:00.000Z"),
		]
	);
	assert_eq!(s.state()["lastScan"], now_ms());

	// A second scan of the same file inserts nothing new and indexes nothing again.
	set_mtime(&file, now_ms());
	s.watcher.scan().await.unwrap();
	assert_eq!(s.rows().len(), 3);
	assert_eq!(s.history().len(), 3);
}

#[tokio::test]
async fn a_summary_is_found_by_its_words_in_its_projects_history_and_in_no_other_projects() {
	let s = Setup::new();
	session(
		&s.demo_dir(),
		"findable",
		&[
			header("s1", PROJECT),
			compaction(
				"c1",
				"The importer streams rows to avoid loading the file.",
				Some("soft"),
				"2026-10-13T10:01:00.000Z",
			),
		],
	);
	s.watcher.scan().await.unwrap();
	let db = lock(&s.db);
	let texts: Vec<String> =
		search_history(&db, "p-demo", "importer streams", 5).unwrap().into_iter().map(|hit| hit.text).collect();
	assert_eq!(texts, ["The importer streams rows to avoid loading the file."]);
	assert!(search_history(&db, "p-other", "importer streams", 5).unwrap().is_empty());
}

#[tokio::test]
async fn summaries_outside_watched_projects_are_stored_but_never_indexed() {
	let s = Setup::new();
	session(
		&s.sessions_dir.join("--C--scratch--"),
		"2026-10-13T10-00-00-000Z_01a2",
		&[
			title_slot("scratch"),
			header("s1", OUTSIDE),
			compaction("c1", "Scratch summary of an unrelated run.", Some("soft"), "2026-10-13T10:05:00.000Z"),
		],
	);
	s.watcher.scan().await.unwrap();
	s.watcher.scan().await.unwrap();
	let rows: Vec<_> = s.rows().into_iter().map(|r| (r.entry_id, r.cwd, r.indexed)).collect();
	assert_eq!(rows, [("c1".to_owned(), OUTSIDE.to_owned(), 0)]);
	assert!(s.history().is_empty());
}

/// The service names a project through its places, which read the catalog the watcher writes: a scan that held the
/// catalog meanwhile waited on itself for good, and every caller of the catalog with it (2026-11-23).
#[tokio::test]
async fn a_scan_names_projects_while_the_catalog_their_lookup_reads_is_free() {
	let s = Setup::reading(|db| {
		let db = db.clone();
		Arc::new(move |cwd: &str| {
			drop(db.try_lock().expect("the catalog is free while a project is named"));
			cwd.starts_with(PROJECT).then(|| "p-demo".to_owned())
		})
	});
	session(
		&s.demo_dir(),
		"watched",
		&[
			header("s1", PROJECT),
			compaction(
				"c1",
				"The importer streams rows to avoid loading the file.",
				Some("soft"),
				"2026-10-13T10:01:00.000Z",
			),
		],
	);
	session(
		&s.sessions_dir.join("--C--scratch--"),
		"outside",
		&[
			header("s2", OUTSIDE),
			compaction("c2", "Scratch summary of an unrelated run.", Some("soft"), "2026-10-13T10:02:00.000Z"),
		],
	);
	// The second scan names the outside row's project again: it is still waiting to be indexed.
	s.watcher.scan().await.unwrap();
	s.watcher.scan().await.unwrap();
	let rows: Vec<_> = s.rows().into_iter().map(|r| (r.entry_id, r.indexed)).collect();
	assert_eq!(rows, pairs(&[("c1", 1), ("c2", 0)]));
	let lines: Vec<_> = s.history().into_iter().map(|l| (l.project_id, l.text)).collect();
	assert_eq!(lines, [("p-demo".to_owned(), "The importer streams rows to avoid loading the file.".to_owned())]);
}

#[tokio::test]
async fn a_session_moved_to_a_sibling_file_gives_its_copied_compactions_once_its_own_new_ones_are_rows_0464() {
	let s = Setup::new();
	let cwd_dir = s.demo_dir();
	let old = session(
		&cwd_dir,
		"2026-10-13T10-00-00-000Z_old1",
		&[
			title_slot("demo"),
			header("old1", PROJECT),
			compaction("c1", "Soft summary before the move.", Some("soft"), "2026-10-13T10:05:00.000Z"),
		],
	);
	let moved = session(
		&cwd_dir,
		"2026-10-13T10-10-00-000Z_new2",
		&[
			title_slot("demo"),
			json!({ "type": "session", "version": 3, "id": "new2", "parentSession": "old1", "cwd": PROJECT })
				.to_string(),
			compaction("c1", "Soft summary before the move.", Some("soft"), "2026-10-13T10:05:00.000Z"),
			json!({ "type": "model_usage", "id": "u9", "usage": { "cost": { "total": 0.1 } } }).to_string(),
			json!({ "type": "memory_recall", "id": "m9", "text": "recalled" }).to_string(),
			compaction("c2", "Handoff after the move.", Some("handoff"), "2026-10-13T10:12:00.000Z"),
		],
	);
	s.watcher.scan().await.unwrap();
	let rows: Vec<_> = s.rows().into_iter().map(|r| (r.session_file, r.entry_id)).collect();
	let name = |file: &Path| file.to_string_lossy().into_owned();
	assert_eq!(rows, [(name(&old), "c1".to_owned()), (name(&moved), "c2".to_owned())]);
	let texts: Vec<_> = s.history().into_iter().map(|line| line.text).collect();
	assert_eq!(texts, ["Soft summary before the move."]);
}

fn id_and_indexed(s: &Setup) -> Vec<(String, i64)> {
	s.rows().into_iter().map(|r| (r.entry_id, r.indexed)).collect()
}

fn pairs(list: &[(&str, i64)]) -> Vec<(String, i64)> {
	list.iter().map(|(id, indexed)| ((*id).to_owned(), *indexed)).collect()
}

#[tokio::test]
async fn remote_and_snapcompact_summaries_are_stored_but_not_indexed_0400() {
	let s = Setup::new();
	session(
		&s.demo_dir(),
		"methods",
		&[
			header("s1", PROJECT),
			compaction("c1", "Remote summary of the whole run.", Some("remote"), "2026-10-13T10:01:00.000Z"),
			compaction("c2", "Snap summary of the whole run.", Some("snapcompact"), "2026-10-13T10:02:00.000Z"),
			compaction("c3", "Soft summary of the whole run.", Some("soft"), "2026-10-13T10:03:00.000Z"),
		],
	);
	s.watcher.scan().await.unwrap();
	assert_eq!(id_and_indexed(&s), pairs(&[("c1", 1), ("c2", 1), ("c3", 1)]));
	let texts: Vec<_> = s.history().into_iter().map(|line| line.text).collect();
	assert_eq!(texts, ["Soft summary of the whole run."]);
}

/// A synthetic snapcompact transcript archive (omp's `snapcompact-summary.md` shape): a preamble, a scope legend naming
/// an archived reasoning scope, a file list and a HISTORY section.
fn archive() -> String {
	[
		"Resume prior conversation. Earlier turns archived under HISTORY below, oldest→newest.",
		"",
		"Archived transcript scopes:",
		"- `¶user:`, `¶think:`, `¶ai:`, `¶call:`: user, assistant reasoning, assistant reply, tool call.",
		"",
		"FILES",
		"===================",
		"README.md (Read)",
		"",
		"HISTORY",
		"===================",
	]
	.join("\n")
}

#[tokio::test]
async fn snapcompact_transcript_archives_never_reach_the_history_whatever_their_method() {
	let s = Setup::new();
	session(
		&s.demo_dir(),
		"archives",
		&[
			header("s1", PROJECT),
			compaction("c1", &archive(), None, "2026-10-13T10:01:00.000Z"),
			compaction("c2", &archive(), Some("soft"), "2026-10-13T10:02:00.000Z"),
			compaction("c3", "Soft summary of the whole run.", Some("soft"), "2026-10-13T10:03:00.000Z"),
		],
	);
	s.watcher.scan().await.unwrap();
	assert_eq!(id_and_indexed(&s), pairs(&[("c1", 1), ("c2", 1), ("c3", 1)]));
	let texts: Vec<_> = s.history().into_iter().map(|line| line.text).collect();
	assert_eq!(texts, ["Soft summary of the whole run."]);
}

/// A `temp_jocasta` is held by `Setup`, so the seed goes in through one of its own first and `Setup` shares its roots by
/// being made inside it: the roots are the same variables, so the file the seed wrote is the one the watcher opens.
/// A summary row to seed: session file, entry id, cwd, time, text and project.
type Seeded<'a> = (&'a str, &'a str, &'a str, i64, &'a str, Option<&'a str>);

fn setup_seeded(rows: &[Seeded]) -> Setup {
	let s = Setup::new();
	{
		let db = lock(&s.db);
		for (file, id, cwd, at, summary, project) in rows {
			db.execute(
				"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, indexed, project_id) VALUES (?, ?, 'compact', ?, ?, ?, 0, ?)",
				rusqlite::params![file, id, cwd, at, summary, project],
			)
			.unwrap();
		}
	}
	s
}

#[tokio::test]
async fn an_archive_already_waiting_in_catalog_db_is_marked_indexed_without_lines_and_no_longer_holds_the_queue() {
	let archive = archive();
	let now = now_ms();
	let s = setup_seeded(&[
		("old.jsonl", "a1", PROJECT, now - 7_200_000, &archive, None),
		("old.jsonl", "a2", PROJECT, now - 3_600_000, "Older soft summary of the run.", None),
	]);
	s.watcher.scan().await.unwrap();
	assert_eq!(id_and_indexed(&s), pairs(&[("a1", 1), ("a2", 1)]));
	let texts: Vec<_> = s.history().into_iter().map(|line| line.text).collect();
	assert_eq!(texts, ["Older soft summary of the run."]);
}

#[tokio::test]
async fn a_modified_file_is_read_from_where_the_last_scan_stopped_cr_03() {
	let s = Setup::with(Arc::new(|_| None));
	let file = session(
		&s.demo_dir(),
		"grow",
		&[header("s1", PROJECT), compaction("c1", "First.", Some("soft"), "2026-10-13T10:01:00.000Z")],
	);
	s.watcher.scan().await.unwrap();
	assert_eq!(s.offset(&file), Some(size(&file)));
	// The rows already read are gone from the table; only the appended line comes back.
	lock(&s.db).execute("DELETE FROM summaries", []).unwrap();
	append(&file, &compaction("c2", "Second.", Some("soft"), "2026-10-13T10:02:00.000Z"));
	let clock = s.tick_clock(60_000);
	set_mtime(&file, clock);
	s.watcher.scan().await.unwrap();
	let db = lock(&s.db);
	let mut statement = db.prepare("SELECT entry_id, cwd FROM summaries").unwrap();
	let rows: Vec<(String, String)> =
		statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
	assert_eq!(rows, [("c2".to_owned(), PROJECT.to_owned())]);
}

#[tokio::test]
async fn files_not_modified_since_last_scan_are_not_reread_modified_ones_are() {
	let j = temp_jocasta();
	let db = Arc::new(Mutex::new(open_catalog().unwrap()));
	let sessions_dir = j.root().join("sessions");
	let state_file = data_dir().join("watcher.json");
	fs::create_dir_all(data_dir()).unwrap();
	fs::write(&state_file, format!("{}\n", json!({ "lastScan": now_ms() - 30_000 }))).unwrap();
	let dir = sessions_dir.join("--C--work--demo--");
	write_session(
		&dir,
		"old",
		&[header("s1", PROJECT), compaction("c1", "Old summary.", Some("soft"), "2026-10-13T09:00:00.000Z")],
		now_ms() - 60_000,
	);
	write_session(
		&dir,
		"new",
		&[header("s2", PROJECT), compaction("c2", "New summary.", Some("soft"), "2026-10-13T11:59:50.000Z")],
		now_ms() - 10_000,
	);
	let watcher = start_watcher(WatcherOptions {
		sessions_dir,
		state_file,
		db: db.clone(),
		project_for: Arc::new(|cwd| cwd.starts_with(PROJECT).then(|| "p-demo".to_owned())),
		interval_ms: Some(3_600_000),
		now: Some(Arc::new(now_ms)),
		on_error: Some(Arc::new(|_| {})),
	});
	watcher.scan().await.unwrap();
	let db = lock(&db);
	let mut statement = db.prepare("SELECT summary FROM summaries ORDER BY at, entry_id").unwrap();
	let summaries: Vec<String> = statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
	assert_eq!(summaries, ["New summary."]);
}

#[tokio::test]
async fn a_summary_is_indexed_under_the_project_stored_with_it_once_its_cwd_folder_is_gone_and_one_from_before_projects_were_stored_under_the_project_its_cwd_names_0440_4()
 {
	let now = now_ms();
	let s = setup_seeded(&[
		("kept.jsonl", "a1", "C:\\gone\\folder", now - 7_200_000, "Kept summary of a removed folder.", Some("p-demo")),
		("old.jsonl", "a2", PROJECT, now - 3_600_000, "Older summary from before projects were stored.", None),
	]);
	s.watcher.scan().await.unwrap();
	let lines: Vec<_> = s.history().into_iter().map(|line| (line.project_id, line.text)).collect();
	assert_eq!(
		lines,
		[
			("p-demo".to_owned(), "Kept summary of a removed folder.".to_owned()),
			("p-demo".to_owned(), "Older summary from before projects were stored.".to_owned()),
		]
	);
}

#[tokio::test]
async fn scans_that_change_nothing_leave_watcher_json_alone() {
	let s = Setup::new();
	session(
		&s.demo_dir(),
		"still",
		&[header("s1", PROJECT), compaction("c1", "Summary.", Some("soft"), "2026-10-13T10:01:00.000Z")],
	);
	s.watcher.scan().await.unwrap();
	let offsets = fs::read_to_string(&s.state_file).unwrap();
	let old = now_ms() - 3_600_000;
	set_mtime(&s.state_file, old);
	let before = fs::metadata(&s.state_file).unwrap().modified().unwrap();
	s.watcher.scan().await.unwrap();
	s.watcher.scan().await.unwrap();
	assert_eq!(fs::metadata(&s.state_file).unwrap().modified().unwrap(), before);
	assert_eq!(fs::read_to_string(&s.state_file).unwrap(), offsets);
}

#[tokio::test]
async fn a_session_omp_rewrote_through_a_rename_no_shorter_than_before_is_read_again_from_its_start_srv_b_3() {
	let s = Setup::with(Arc::new(|_| Some("p-demo".to_owned())));
	let dir = s.demo_dir();
	let long_turn = assistant("a1", json!([{ "type": "text", "text": "x".repeat(5000) }]), "length");
	let file = session(&dir, "rewritten", &[header("s1", PROJECT), long_turn]);
	s.watcher.scan().await.unwrap();
	// omp's discard: the long turn dropped (re-journaled small), then the recovery's compaction; the whole file written
	// to a temp file and renamed over it, longer than the offset the last scan kept.
	let discarded = json!({
		"type": "model_usage",
		"id": "m9",
		"parentId": null,
		"timestamp": "2026-10-13T10:02:00.000Z",
		"purpose": "discarded-turn",
		"usage": { "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "cost": { "total": 0.01 } },
	})
	.to_string();
	let temp = session(
		&dir,
		"rewritten.tmp",
		&[
			header("s1", PROJECT),
			discarded,
			compaction(
				"c1",
				&format!("## Goal\n{}", "The task so far. ".repeat(400)),
				Some("soft"),
				"2026-10-13T10:03:00.000Z",
			),
		],
	);
	assert!(size(&temp) > size(&file));
	fs::rename(&temp, &file).unwrap();
	let clock = s.tick_clock(60_000);
	set_mtime(&file, clock);
	s.watcher.scan().await.unwrap();
	assert_eq!(s.entry_ids(), ["c1"]);
}

#[tokio::test]
async fn watcher_json_is_written_when_its_files_change_at_most_every_5_minutes_while_only_offsets_move_and_at_stop_perf_3()
 {
	let s = Setup::with(Arc::new(|_| None));
	let dir = s.demo_dir();
	let file = session(&dir, "busy", &[header("s1", PROJECT)]);
	s.watcher.scan().await.unwrap();
	assert_eq!(s.offset(&file), Some(size(&file)));
	// An offset moved a minute later: kept in memory only.
	append(&file, &compaction("c1", "First.", Some("soft"), "2026-10-13T10:01:00.000Z"));
	let clock = s.tick_clock(60_000);
	set_mtime(&file, clock);
	s.watcher.scan().await.unwrap();
	assert!(s.offset(&file).unwrap() < size(&file));
	// A new file is written at once.
	let other = write_session(&dir, "new", &[header("s2", PROJECT)], clock);
	s.tick_clock(1_000);
	s.watcher.scan().await.unwrap();
	assert_eq!(s.offset(&file), Some(size(&file)));
	assert!(!s.state()["files"][other.to_string_lossy().as_ref()].is_null());
	// Moved offsets wait for 5 minutes, or the stop.
	append(&file, &compaction("c2", "Second.", Some("soft"), "2026-10-13T10:02:00.000Z"));
	let clock = s.tick_clock(60_000);
	set_mtime(&file, clock);
	s.watcher.scan().await.unwrap();
	assert!(s.offset(&file).unwrap() < size(&file));
	s.watcher.stop();
	assert_eq!(s.offset(&file), Some(size(&file)));
}

#[test]
fn watcher_json_is_written_as_json_stringify_writes_it() {
	let mut files = Files::default();
	files.set(
		"C:\\s\\a.jsonl".to_owned(),
		FileState {
			offset: 12,
			cwd: Some("C:\\work".to_owned()),
			ino: Some(77),
			birthtime_ms: Some(1_760_000_000_000.0),
		},
	);
	files.set("b".to_owned(), FileState { offset: 0, cwd: None, ino: None, birthtime_ms: Some(1.5) });
	let state = WatcherState { last_scan: 1_760_000_000_123.0, files };
	assert_eq!(
		state_json(&state),
		"{\"lastScan\":1760000000123,\"files\":{\"C:\\\\s\\\\a.jsonl\":{\"offset\":12,\"cwd\":\"C:\\\\work\",\"ino\":77,\"birthtimeMs\":1760000000000},\"b\":{\"offset\":0,\"birthtimeMs\":1.5}}}\n"
	);
}

#[tokio::test]
async fn a_mark_that_fails_after_the_lines_leaves_no_lines_behind() {
	let now = now_ms();
	let s = setup_seeded(&[("old.jsonl", "a1", PROJECT, now - 3_600_000, "Older soft summary of the run.", None)]);
	// Stop the watcher's first scan, which may have indexed the row already; start from an unindexed row again.
	s.watcher.scan().await.unwrap();
	{
		let db = lock(&s.db);
		db.execute_batch(
			"DELETE FROM history; UPDATE summaries SET indexed = 0;
			 CREATE TRIGGER no_mark BEFORE UPDATE ON summaries BEGIN SELECT RAISE(ABORT, 'no mark'); END;",
		)
		.unwrap();
	}
	assert!(s.watcher.scan().await.is_err());
	assert!(s.history().is_empty());
	assert_eq!(id_and_indexed(&s), pairs(&[("a1", 0)]));
	lock(&s.db).execute_batch("DROP TRIGGER no_mark").unwrap();
	s.watcher.scan().await.unwrap();
	assert_eq!(s.history().len(), 1);
	assert_eq!(id_and_indexed(&s), pairs(&[("a1", 1)]));
}
