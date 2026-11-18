//! A thread's task file (0540, K2): `<data>/projects/<pid>/threads/<tid>/task.md`, written by Jocasta only and kept
//! current as the thread moves, rendered from the thread's record `thread.json` beside it (which also holds its
//! `report_result`, 0539). `prompt.md` beside them is what each omp launch of the thread appends to its system prompt
//! (`--append-system-prompt`): the panes' shipped prompt (Ponytail, 0427), then the file's Brief and Done-when sections.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::frontier::server::hunk::ReviewProof;
use crate::frontier::server::report::{ThreadReport, report_line};
use crate::frontier::server::sessions::TestsResult;
use crate::frontier::server::wording::WHEN_YOU_FINISH;
use crate::shared::clock::now_ms;
use crate::shared::day::hhmm;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, write_atomically};
use crate::shared::pathkey::path_key;
use crate::shared::paths::project_dir;
use crate::shared::prompt::without_leading_comment;

/// One check of a review (0453, 0540): the command and its exit code in the review turn; `None` when it didn't run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckExit {
	pub command: String,
	pub exit: Option<i32>,
}

/// A [`TestsResult`] as `thread.json` spells it: `pass`, `fail`, `none` or `unknown`.
mod tests_result {
	use serde::{Deserialize, Deserializer, Serializer};

	use crate::frontier::server::sessions::TestsResult;

	pub fn serialize<S: Serializer>(value: &Option<TestsResult>, serializer: S) -> Result<S::Ok, S::Error> {
		match value {
			Some(TestsResult::Pass) => serializer.serialize_str("pass"),
			Some(TestsResult::Fail) => serializer.serialize_str("fail"),
			Some(TestsResult::None) => serializer.serialize_str("none"),
			Some(TestsResult::Unknown) => serializer.serialize_str("unknown"),
			None => serializer.serialize_none(),
		}
	}

	pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<TestsResult>, D::Error> {
		match Option::<String>::deserialize(deserializer)?.as_deref() {
			None => Ok(None),
			Some("pass") => Ok(Some(TestsResult::Pass)),
			Some("fail") => Ok(Some(TestsResult::Fail)),
			Some("none") => Ok(Some(TestsResult::None)),
			Some("unknown") => Ok(Some(TestsResult::Unknown)),
			Some(other) => Err(serde::de::Error::custom(format!("not a tests result: {other}"))),
		}
	}
}

/// A number as `JSON.stringify` writes it: a whole value has no fraction (`1`, not `1.0`).
fn js_number<S: serde::Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
	match value {
		Some(number) if number.fract() == 0.0 && number.abs() < 1e15 => serializer.serialize_i64(*number as i64),
		Some(number) => serializer.serialize_f64(*number),
		None => serializer.serialize_none(),
	}
}

/// One review as the task file lists it, with the outcome a restart gives the thread back (its notes, blocking notes,
/// tests and diff proof; absent in records written before they were kept).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskReview {
	pub at: i64,
	pub passed: bool,
	/// The project's check (when known) and each `done_when` command, in that order.
	pub checks: Vec<CheckExit>,
	/// What the review found: `2 notes (1 blocking), tests pass`, and `no report` when the thread handed in none.
	pub said: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub notes: Option<usize>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub blocking: Option<usize>,
	#[serde(default, skip_serializing_if = "Option::is_none", with = "tests_result")]
	pub tests: Option<TestsResult>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub proof: Option<ReviewProof>,
}

/// How a thread closed (0621): squash-merged, opened as a PR, or its worktree removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseHow {
	Merged,
	Pr,
	Removed,
}

/// When and how a thread closed, and its commit (0621): the squash commit for a merge, the pushed head for a PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskClose {
	pub at: i64,
	pub how: CloseHow,
	/// The full commit id; `None` for a removed thread. Nothing is added to the commit's message.
	pub commit: Option<String>,
}

/// A [`TaskClose`] without its time: what [`close_task`] is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFacts {
	pub how: CloseHow,
	pub commit: Option<String>,
}

/// The approved plan: when, its title and its file (`local://<slug>-plan.md` or a path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlan {
	pub approved_at: i64,
	pub title: String,
	pub file: Option<String>,
}

/// The thread's `report_result` and when it came (0539).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskReport {
	pub at: i64,
	pub value: ThreadReport,
}

/// `thread.json`: what the task file is rendered from. A key missing from an older record reads as empty (`closed`
/// false, no plan, no reviews). Fields are in the order the TypeScript's `newTask` writes
/// them, so a record written here reads byte for byte as one written there.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskRecord {
	/// The worktree; empty until it is made.
	pub path: String,
	pub branch: String,
	/// Trunk's commit the thread started from, short.
	pub base: Option<String>,
	/// The thread's omp session file, once known.
	pub session: Option<String>,
	/// The thread this one continues; no lineage yet.
	pub parent: Option<String>,
	pub created_at: i64,
	/// The state as `thread_status` says it.
	pub state: String,
	/// USD, from its session; `None` before one is read.
	#[serde(serialize_with = "js_number")]
	pub spend: Option<f64>,
	pub plan: Option<TaskPlan>,
	pub report: Option<TaskReport>,
	pub reviews: Vec<TaskReview>,
	/// The thread was finished or removed: its worktree folder may hold a new thread.
	pub closed: bool,
	pub project_id: String,
	pub project: String,
	pub title: String,
	/// The boss's brief, or his own from New thread or `jocasta thread new`; empty when not recorded.
	pub brief: String,
	/// Commands that must pass (0540), run by the review beside the project's check.
	pub done_when: Vec<String>,
	/// The boss started it heavy (0590): every launch of its omp loads `config/heavy.yml` after the project's overlay, so
	/// it runs on his `slow` role. Absent: not heavy (records written before it was kept read the same).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub heavy: Option<bool>,
	/// `t-` and 4 hex digits, unique in the project.
	pub id: String,
	/// Absent while it is open, and on a record closed before 0621.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub close: Option<TaskClose>,
	/// Keys of `thread.json` this build doesn't know (a newer or older build wrote them): kept through a save.
	#[serde(flatten)]
	pub extra: serde_json::Map<String, serde_json::Value>,
}

/// What [`new_task`] is given: the TypeScript's required fields and its optional ones a caller sets.
#[derive(Debug, Clone, Default)]
pub struct NewTask {
	pub project_id: String,
	pub project: String,
	pub title: String,
	pub brief: String,
	pub done_when: Vec<String>,
	pub path: Option<String>,
	pub branch: Option<String>,
	pub base: Option<String>,
	pub session: Option<String>,
	pub parent: Option<String>,
	pub heavy: Option<bool>,
}

/// `<data>/projects/<pid>/threads`.
fn threads_dir(project_id: &str) -> PathBuf {
	project_dir(project_id).join("threads")
}

/// The record's folder.
pub fn task_dir(project_id: &str, id: &str) -> PathBuf {
	threads_dir(project_id).join(id)
}

/// The task file's path.
pub fn task_file(project_id: &str, id: &str) -> PathBuf {
	task_dir(project_id, id).join("task.md")
}

/// A new record with a fresh id; nothing is written until [`save_task`]. `now` defaults to the clock.
pub fn new_task(fields: NewTask, now: Option<i64>) -> TaskRecord {
	let id = loop {
		let mut bytes = [0_u8; 2];
		getrandom::fill(&mut bytes).expect("the OS has random bytes");
		let id = format!("t-{:02x}{:02x}", bytes[0], bytes[1]);
		if !threads_dir(&fields.project_id).join(&id).exists() {
			break id;
		}
	};
	TaskRecord {
		path: fields.path.unwrap_or_default(),
		branch: fields.branch.unwrap_or_default(),
		base: fields.base,
		session: fields.session,
		parent: fields.parent,
		created_at: now.unwrap_or_else(now_ms),
		state: "starting".to_owned(),
		spend: None,
		plan: None,
		report: None,
		reviews: Vec::new(),
		closed: false,
		project_id: fields.project_id,
		project: fields.project,
		title: fields.title,
		brief: fields.brief,
		done_when: fields.done_when,
		heavy: fields.heavy,
		id,
		close: None,
		extra: serde_json::Map::new(),
	}
}

/// Every record of the project, open and closed, in no order; an unreadable one is left out.
pub fn project_tasks(project_id: &str) -> Vec<TaskRecord> {
	let Ok(entries) = fs::read_dir(threads_dir(project_id)) else { return Vec::new() };
	let mut records = Vec::new();
	for entry in entries.flatten() {
		if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
			continue;
		}
		let Some(object) = read_json_object(&entry.path().join("thread.json")) else { continue };
		if let Ok(record) = serde_json::from_value(serde_json::Value::Object(object)) {
			records.push(record);
		}
	}
	records
}

/// The open record of the thread in worktree `path`, the newest when several claim it; `None` when none.
pub fn find_task(project_id: &str, path: &Path) -> Option<TaskRecord> {
	let key = path_key(path);
	let mut found: Option<TaskRecord> = None;
	for record in project_tasks(project_id) {
		if record.closed || record.path.is_empty() || path_key(Path::new(&record.path)) != key {
			continue;
		}
		if found.as_ref().is_none_or(|best| record.created_at > best.created_at) {
			found = Some(record);
		}
	}
	found
}

/// The fields of a record a launch's prompt and the task file's Brief and Done-when sections read.
#[derive(Debug, Clone, Copy)]
pub struct PromptFields<'a> {
	pub title: &'a str,
	pub project: &'a str,
	pub brief: &'a str,
	pub done_when: &'a [String],
}

impl<'a> From<&'a TaskRecord> for PromptFields<'a> {
	fn from(record: &'a TaskRecord) -> Self {
		Self { title: &record.title, project: &record.project, brief: &record.brief, done_when: &record.done_when }
	}
}

/// What `String.prototype.trim` strips: white space and the byte order mark.
fn js_trim(text: &str) -> &str {
	text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// A file's text as Node reads it (`utf8`: bad bytes become U+FFFD); `None` when it can't be read.
fn read_text(file: &Path) -> Option<String> {
	fs::read(file).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The Brief section, as the task file and each launch's appended prompt have it.
fn brief_section(brief: &str) -> Vec<String> {
	let brief = js_trim(brief);
	vec!["## Brief".to_owned(), if brief.is_empty() { "Not recorded." } else { brief }.to_owned()]
}

/// The Done-when section (0540).
fn done_when_section(done_when: &[String]) -> Vec<String> {
	let mut lines = vec!["## Done when".to_owned()];
	if done_when.is_empty() {
		lines.push("- No commands given; the review's project check decides.".to_owned());
	} else {
		lines.extend(done_when.iter().map(|command| format!("- `{command}` passes")));
	}
	lines
}

/// The Brief and Done-when sections.
fn brief_sections(fields: &PromptFields) -> Vec<String> {
	let mut lines = brief_section(fields.brief);
	lines.push(String::new());
	lines.extend(done_when_section(fields.done_when));
	lines
}

/// `file` without its `.jsonl` ending.
fn without_jsonl(file: &str) -> &str {
	file.strip_suffix(".jsonl").unwrap_or(file)
}

/// The approved plan's text from its file, or its path when the file can't be read. omp names it `local://<slug>-plan.md`,
/// which lives in the session's artifacts dir (`<session stem>/local/`).
fn plan_text(file: Option<&str>, session: Option<&str>) -> String {
	let Some(file) = file.filter(|file| !file.is_empty()) else { return "The plan's file isn't recorded.".to_owned() };
	// `/^local:\/\/(.+)$/`: `.` stops at a line break.
	let local = file
		.strip_prefix("local://")
		.filter(|rest| !rest.is_empty() && !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}']));
	let path = match local {
		Some(local) => session.filter(|s| !s.is_empty()).map(|s| Path::new(without_jsonl(s)).join("local").join(local)),
		None => Some(PathBuf::from(file)),
	};
	if let Some(text) = path.and_then(|path| read_text(&path)) {
		return js_trim(&text).to_owned();
	}
	format!("In {file}.")
}

/// `number.toFixed(2)`: a tie rounds away from zero where Rust's formatter rounds to even.
fn fixed2(number: f64) -> String {
	let eighths = number * 8.0;
	if eighths.fract() == 0.0 && eighths.abs() % 2.0 == 1.0 {
		let cents = (number.abs() * 100.0).round();
		return format!("{}{:.2}", if number < 0.0 { "-" } else { "" }, cents / 100.0);
	}
	format!("{number:.2}")
}

/// The task file's text (0540's format).
fn task_markdown(record: &TaskRecord) -> String {
	let session = match record.session.as_deref().filter(|s| !s.is_empty()) {
		Some(session) => {
			let name = Path::new(session).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
			without_jsonl(&name).to_owned()
		}
		None => "—".to_owned(),
	};
	let dash = "—";
	let mut lines = vec![
		format!("# {} ({})", record.title, record.project),
		String::new(),
		format!(
			"- Thread: {} · session {session} · parent: {} · base {}",
			record.id,
			record.parent.as_deref().unwrap_or(dash),
			record.base.as_deref().unwrap_or(dash)
		),
		format!(
			"- State: {} · spend {}",
			record.state,
			record.spend.map_or_else(|| dash.to_owned(), |spend| format!("${}", fixed2(spend)))
		),
		String::new(),
	];
	lines.extend(brief_sections(&PromptFields::from(record)));
	lines.push(String::new());
	match &record.plan {
		Some(plan) => {
			lines.push(format!("## Plan (approved {})", hhmm(plan.approved_at)));
			lines.push(plan_text(plan.file.as_deref(), record.session.as_deref()));
		}
		None => lines.extend(["## Plan".to_owned(), "Not approved yet.".to_owned()]),
	}
	lines.extend([
		String::new(),
		"## Report".to_owned(),
		record.report.as_ref().map_or_else(|| "None yet.".to_owned(), |report| report_line(&report.value)),
		String::new(),
		"## Reviews".to_owned(),
	]);
	if record.reviews.is_empty() {
		lines.push("None yet.".to_owned());
	}
	for review in &record.reviews {
		let mut parts = vec![format!("- {} {}", hhmm(review.at), if review.passed { "pass" } else { "not a pass" })];
		parts.extend(review.checks.iter().map(|check| {
			let exit = check.exit.map_or_else(|| "not run".to_owned(), |code| format!("exit {code}"));
			format!("`{}` {exit}", check.command)
		}));
		parts.push(format!("notes: {}", review.said));
		lines.push(parts.join(" · "));
	}
	format!("{}\n", lines.join("\n"))
}

/// `thread.json`'s text: `JSON.stringify(record, null, "\t")` and a newline.
fn record_json(record: &TaskRecord) -> Result<String> {
	let mut out = Vec::new();
	let mut serializer =
		serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"\t"));
	Serialize::serialize(record, &mut serializer)?;
	out.push(b'\n');
	String::from_utf8(out).map_err(|error| Error::other(error.to_string()))
}

/// Writes `thread.json` and `task.md` when they changed.
pub fn save_task(record: &TaskRecord) -> Result<()> {
	let dir = task_dir(&record.project_id, &record.id);
	write_if_changed(&dir.join("thread.json"), &record_json(record)?)?;
	write_if_changed(&dir.join("task.md"), &task_markdown(record))
}

fn write_if_changed(file: &Path, text: &str) -> Result<()> {
	if read_text(file).is_some_and(|current| current == text) {
		return Ok(());
	}
	write_atomically(file, text, None)
}

/// Marks the thread closed, with when, how and its commit (0621), so its folder may hold a new thread; its state says
/// `finished`, or `worktree removed` for a removed one. A record already closed keeps its first close: a merged
/// thread whose folder is removed later stays merged. A failed write goes to `failed`. `now` defaults to the clock.
pub fn close_task(record: &mut TaskRecord, close: CloseFacts, failed: impl FnOnce(Error), now: Option<i64>) {
	if record.close.is_some() {
		return;
	}
	record.closed = true;
	record.state = if close.how == CloseHow::Removed { "worktree removed" } else { "finished" }.to_owned();
	record.close = Some(TaskClose { at: now.unwrap_or_else(now_ms), how: close.how, commit: close.commit });
	if let Err(error) = save_task(record) {
		failed(error);
	}
}

/// The text a thread's omp appends to its system prompt (0540, 0577): the panes' shipped prompt when there is one,
/// without its leading `<!-- … -->` comment (its license and file notes, PRM-6), then the Brief, Done-when and When you
/// finish sections. With the layer off (`layer` false, 0576's test switch) only the Brief follows the title: Done when
/// and When you finish are the layer's parts. The benchmark's A6 layer appends the same text (0543).
pub fn task_prompt(record: &PromptFields, pane_prompt: Option<&Path>, layer: bool) -> String {
	let shipped = pane_prompt
		.and_then(read_text)
		.map(|text| js_trim(&without_leading_comment(&text)).to_owned())
		.unwrap_or_default();
	let sections = if layer {
		let mut sections = brief_sections(record);
		sections.push(String::new());
		sections.extend(WHEN_YOU_FINISH.iter().map(|line| (*line).to_owned()));
		sections
	} else {
		brief_section(record.brief)
	};
	let mut lines = vec![format!("# Your task: {} ({})", record.title, record.project), String::new()];
	lines.extend(sections);
	let task = lines.join("\n");
	let lead = if shipped.is_empty() { String::new() } else { format!("{shipped}\n\n") };
	format!("{lead}{task}\n")
}

/// `prompt.md`, what this launch of the thread's omp appends to its system prompt (0540), [`task_prompt`]'s text.
/// omp 18.18 takes one `--append-system-prompt`, the last one given (text, or a file's contents when the value names a
/// file), so the two go in one file. Resolves its path.
pub fn launch_prompt(record: &TaskRecord, pane_prompt: Option<&Path>, layer: bool) -> Result<PathBuf> {
	let file = task_dir(&record.project_id, &record.id).join("prompt.md");
	write_if_changed(&file, &task_prompt(&PromptFields::from(record), pane_prompt, layer))?;
	Ok(file)
}

#[cfg(test)]
#[path = "task_file_tests.rs"]
mod tests;
