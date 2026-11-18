//! omp's session files as every reader sees them (SRV-B-10): the product's session index, the cost ledger, the
//! watcher and today's spend share these rules, so an omp format change (0464 re-checks every bump) is made once.
//!
//! - Layout: a session is `<sessions>/<cwd dir>/<stem>.jsonl` (omp `session-paths.ts` `sessionDirForCwd`); its subagent,
//!   advisor and fork transcripts sit under its artifacts dir `<stem>/`, nested deeper for a subagent's own
//!   (`sub-sessions.ts`), so only files directly in a cwd dir are sessions ([`SessionWalker`], [`transcripts_of`]).
//! - Header: `{type: "session", version, id, timestamp, cwd, parentSession?}` (`session-entries.ts`); since the title
//!   slot (`{type: "title", v: 1, …}`, 256 bytes rewritten in place) it is the file's second line ([`read_header`]).
//! - Rewrites: omp ≥ 18.10.4 rewrites a session whole through a temp file and a rename (discards, retry marks, shake),
//!   possibly no shorter than what was read, so an incremental reader checks the file's identity as well as its size
//!   ([`rewritten`], [`SessionTail`], SRV-B-3).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::shared::clock::now_ms;
use crate::shared::errors::Result;
use crate::shared::jsonl::{parse_line, read_lines};

/// Scratch copies `omp gc --compress` leaves beside transcripts; never transcripts of their own (omp-stats).
const COMPRESS_SCRATCH: &str = ".omp-gc-compress-";

/// A session file's header.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionHeader {
	pub file: PathBuf,
	pub cwd: String,
	pub title: Option<String>,
	/// The header's session id, and the session it moved from or forked off (`parentSession`).
	pub id: Option<String>,
	pub parent_session: Option<String>,
	/// When the session began (the header's `timestamp`), epoch ms.
	pub started: Option<i64>,
}

/// What a reader sees of a file when it stats it: the TypeScript's `Stats` (size, mtime, inode, birth time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileStat {
	pub size: u64,
	/// Epoch ms, fractional as Node's `mtimeMs`.
	pub mtime_ms: f64,
	/// The file's identity on its volume: the inode, or the NTFS file index.
	pub ino: u64,
	/// Epoch ms; 0 where the file system keeps no creation time.
	pub birthtime_ms: f64,
}

fn epoch_ms(time: std::io::Result<SystemTime>) -> f64 {
	match time {
		Ok(time) => match time.duration_since(UNIX_EPOCH) {
			Ok(after) => after.as_nanos() as f64 / 1e6,
			Err(before) => -(before.duration().as_nanos() as f64) / 1e6,
		},
		Err(_) => 0.0,
	}
}

#[cfg(unix)]
fn file_id(_path: &Path, meta: &fs::Metadata) -> u64 {
	std::os::unix::fs::MetadataExt::ino(meta)
}

/// The NTFS file index, read through a handle opened for attributes only (as libuv does for `stat`); 0 when it can't be.
#[cfg(windows)]
fn file_id(path: &Path, _meta: &fs::Metadata) -> u64 {
	use std::os::windows::fs::OpenOptionsExt;
	use std::os::windows::io::AsRawHandle;

	use windows_sys::Win32::Storage::FileSystem::{
		BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
	};
	let mut options = fs::OpenOptions::new();
	options.access_mode(0).custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
	let Ok(file) = options.open(path) else { return 0 };
	// SAFETY: a zeroed BY_HANDLE_FILE_INFORMATION is a valid value, and the handle lives as long as `file`.
	let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
	if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
		return 0;
	}
	(u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow)
}

impl FileStat {
	/// `path`'s stat, following links; `None` when it can't be read (the TypeScript's `throwIfNoEntry: false`).
	pub fn of(path: &Path) -> Option<FileStat> {
		let meta = fs::metadata(path).ok()?;
		Some(FileStat {
			size: meta.len(),
			mtime_ms: epoch_ms(meta.modified()),
			ino: file_id(path, &meta),
			birthtime_ms: epoch_ms(meta.created()),
		})
	}
}

/// `Date.parse` of the timestamps omp and the other harnesses write: RFC 3339, a date alone (UTC), or a date and time
/// without an offset (local time); `None` otherwise.
pub fn date_parse(text: &str) -> Option<i64> {
	use chrono::{NaiveDate, NaiveDateTime, TimeZone};
	let text = text.trim();
	if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
		return Some(at.timestamp_millis());
	}
	if let Ok(day) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
		return Some(day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
	}
	["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f"].iter().find_map(|format| {
		let naive = NaiveDateTime::parse_from_str(text, format).ok()?;
		Some(chrono::Local.from_local_datetime(&naive).earliest()?.timestamp_millis())
	})
}

/// The first `bytes` of a file: enough for the title slot (256 bytes) and the session header.
fn head(file: &Path, bytes: u64) -> Result<String> {
	let mut buffer = Vec::new();
	fs::File::open(file)?.take(bytes).read_to_end(&mut buffer)?;
	Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// A session file's header: its cwd, title, id, `parentSession` and start; `None` when it has no `session` line yet.
/// Fails when the file can't be opened.
pub fn read_header(file: &Path) -> Result<Option<SessionHeader>> {
	let mut title = None;
	for line in head(file, 8192)?.split('\n') {
		let Some(entry) = parse_line(line) else { continue };
		let kind = entry.get("type").and_then(Value::as_str);
		if kind == Some("title")
			&& let Some(text) = entry.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty())
		{
			title = Some(text.to_owned());
		}
		if kind != Some("session") {
			continue;
		}
		let Some(cwd) = entry.get("cwd").and_then(Value::as_str) else { return Ok(None) };
		let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_owned);
		return Ok(Some(SessionHeader {
			file: file.to_path_buf(),
			cwd: cwd.to_owned(),
			title,
			id: text("id"),
			parent_session: text("parentSession"),
			started: entry.get("timestamp").and_then(Value::as_str).and_then(date_parse),
		}));
	}
	Ok(None)
}

/// A session file's artifacts dir: its path without `.jsonl`.
pub(crate) fn artifacts_dir(session_file: &Path) -> PathBuf {
	let text = session_file.to_string_lossy();
	PathBuf::from(text.strip_suffix(".jsonl").unwrap_or(&text))
}

/// Every `*.jsonl` anywhere under `dir`, leaving out the `omp gc --compress` scratch files and folders.
fn collect_transcripts(dir: &Path, found: &mut Vec<PathBuf>) {
	let Ok(entries) = fs::read_dir(dir) else { return };
	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		if name.contains(COMPRESS_SCRATCH) {
			continue;
		}
		let Ok(kind) = entry.file_type() else { continue };
		if kind.is_dir() {
			collect_transcripts(&entry.path(), found);
		} else if kind.is_file() && name.ends_with(".jsonl") {
			found.push(entry.path());
		}
	}
}

/// The transcripts (`*.jsonl`) anywhere under `session_file`'s artifacts dir (the path without `.jsonl`): subagents,
/// the advisor, forks; sorted. `omp gc --compress` scratch copies are left out, a file or a folder whose name holds the
/// marker alike (the ledger's tested rule). Empty when the dir doesn't exist (yet).
pub fn transcripts_of(session_file: &Path) -> Vec<PathBuf> {
	let mut found = Vec::new();
	collect_transcripts(&artifacts_dir(session_file), &mut found);
	found.sort_by_cached_key(|path| path.to_string_lossy().into_owned());
	found
}

/// Where a reader stopped in a file: the offset after its last complete line, and the file's identity then.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReadMark {
	pub offset: u64,
	/// `None` when not known (a state kept before identities were): the file then counts as the same one.
	pub ino: Option<u64>,
	pub birthtime_ms: Option<f64>,
}

/// Whether another file took the name of the one `mark` was read from (a rewrite through a rename).
pub fn replaced(stat: &FileStat, mark: &ReadMark) -> bool {
	mark.ino.is_some() && (Some(stat.ino) != mark.ino || Some(stat.birthtime_ms) != mark.birthtime_ms)
}

/// Whether the file was rewritten since `mark`: replaced, or now shorter than the offset; it is read again from 0.
pub fn rewritten(stat: &FileStat, mark: &ReadMark) -> bool {
	replaced(stat, mark) || stat.size < mark.offset
}

/// What a [`SessionTail`] hands its entries to.
pub trait TailVisitor {
	/// One complete JSON object line, in file order.
	fn entry(&mut self, entry: Map<String, Value>);
	/// The file was rewritten: everything handed over so far is void; it is read again from its start.
	fn reset(&mut self);
}

/// One JSONL file followed incrementally (CR-03): its offset, its identity and its size and mtime when last read; only
/// lines appended since are parsed, a line still being written waits (`jsonl.rs`), and a rewritten file
/// ([`rewritten`]) resets the visitor and is read again from its start.
pub struct SessionTail<V: TailVisitor> {
	pub file: PathBuf,
	pub visitor: V,
	pub mark: ReadMark,
	/// Size when last read to the end; `None` before.
	pub size: Option<u64>,
	/// Mtime when last read to the end; `None` before.
	pub mtime_ms: Option<f64>,
}

impl<V: TailVisitor> SessionTail<V> {
	pub fn new(file: PathBuf, visitor: V) -> Self {
		SessionTail { file, visitor, mark: ReadMark::default(), size: None, mtime_ms: None }
	}

	/// The offset after the last complete line read.
	pub fn offset(&self) -> u64 {
		self.mark.offset
	}

	/// Whether the file behind `stat` changed since it was last read; a rewritten one resets the tail first. Keeps the
	/// file's identity.
	pub fn changed(&mut self, stat: &FileStat) -> bool {
		let again = rewritten(stat, &self.mark);
		if !again && Some(stat.size) == self.size && Some(stat.mtime_ms) == self.mtime_ms {
			return false;
		}
		if again {
			self.mark.offset = 0;
			self.visitor.reset();
		}
		self.mark.ino = Some(stat.ino);
		self.mark.birthtime_ms = Some(stat.birthtime_ms);
		true
	}

	/// Hands the complete lines between the offset and byte `to` to the visitor and moves the offset past them.
	pub fn read_to(&mut self, to: u64) -> Result<u64> {
		let visitor = &mut self.visitor;
		self.mark.offset = read_lines(&self.file, self.mark.offset, to, |line| {
			if let Some(entry) = parse_line(line) {
				visitor.entry(entry);
			}
		})?;
		Ok(self.mark.offset)
	}

	/// Brings the tail up to the file on disk (`stat`, when the caller has it); whether anything changed. A missing file
	/// leaves it as it was.
	pub fn advance(&mut self, stat: Option<FileStat>) -> Result<bool> {
		let Some(stat) = stat.or_else(|| FileStat::of(&self.file)) else { return Ok(false) };
		if !self.changed(&stat) {
			return Ok(false);
		}
		self.read_to(stat.size)?;
		self.size = Some(stat.size);
		self.mtime_ms = Some(stat.mtime_ms);
		Ok(true)
	}

	/// As [`SessionTail::advance`], `step` bytes at a time with the runtime let run between steps; stops early when
	/// `stopped` says so, and `advance` then reads the rest when asked.
	pub async fn advance_in_steps(&mut self, step: u64, stopped: impl Fn() -> bool) -> Result<()> {
		let Some(stat) = FileStat::of(&self.file) else { return Ok(()) };
		if !self.changed(&stat) {
			return Ok(());
		}
		while self.mark.offset < stat.size && !stopped() {
			// A line longer than a step: the window doubles until the line is whole, then stepping goes on (PM-18).
			let from = self.mark.offset;
			let mut window = step;
			loop {
				let to = stat.size.min(from + window);
				if self.read_to(to)? != from || to == stat.size {
					break;
				}
				window *= 2;
			}
			if self.mark.offset == from {
				break; // only a line still being written is left
			}
			tokio::task::yield_now().await;
		}
		if self.mark.offset < stat.size && stopped() {
			return Ok(());
		}
		self.size = Some(stat.size);
		self.mtime_ms = Some(stat.mtime_ms);
		Ok(())
	}
}

/// A cwd dir listed this soon after its mtime is listed again next walk: a file made within the file system's clock
/// tick of the listing leaves the dir's mtime as it was (NTFS gives a create right after another the same time).
const RACY_MS: f64 = 2_000.0;

/// One cwd dir as last listed.
struct Listing {
	mtime_ms: f64,
	listed_at: i64,
	files: Vec<PathBuf>,
}

/// Lists omp's session files: `*.jsonl` directly inside each cwd dir of `sessions_dir` (subagent and advisor
/// transcripts sit one level deeper). A cwd dir whose mtime hasn't changed since this walker last listed it is answered
/// from that listing (SRV-B-6): a new, removed or renamed session file changes its dir's mtime; an append doesn't, and
/// a reader stats the files it follows anyway.
pub struct SessionWalker {
	pub sessions_dir: PathBuf,
	dirs: HashMap<PathBuf, Listing>,
}

impl SessionWalker {
	pub fn new(sessions_dir: PathBuf) -> Self {
		SessionWalker { sessions_dir, dirs: HashMap::new() }
	}

	/// The `*.jsonl` files directly inside `path`; `None` when it can't be listed (removed since).
	fn list(path: &Path) -> Option<Vec<PathBuf>> {
		let names = fs::read_dir(path).ok()?;
		Some(
			names
				.flatten()
				.filter(|e| {
					e.file_type().is_ok_and(|kind| kind.is_file())
						&& e.file_name().to_string_lossy().ends_with(".jsonl")
				})
				.map(|e| e.path())
				.collect(),
		)
	}

	/// Every session file under the sessions dir, each cwd dir answered from its last listing while its mtime holds.
	pub fn files(&mut self) -> Vec<PathBuf> {
		let Ok(cwd_dirs) = fs::read_dir(&self.sessions_dir) else {
			self.dirs.clear();
			return Vec::new();
		};
		let mut files = Vec::new();
		let mut seen = HashSet::new();
		for cwd_dir in cwd_dirs.flatten() {
			if !cwd_dir.file_type().is_ok_and(|kind| kind.is_dir()) {
				continue;
			}
			let path = self.sessions_dir.join(cwd_dir.file_name());
			seen.insert(path.clone());
			let Some(stat) = FileStat::of(&path) else { continue }; // removed since the listing
			if let Some(known) = self.dirs.get(&path)
				&& known.mtime_ms == stat.mtime_ms
				&& known.listed_at as f64 - stat.mtime_ms >= RACY_MS
			{
				files.extend(known.files.iter().cloned());
				continue;
			}
			let listed_at = now_ms();
			let Some(own) = Self::list(&path) else {
				self.dirs.remove(&path);
				continue; // removed since the listing
			};
			files.extend(own.iter().cloned());
			self.dirs.insert(path, Listing { mtime_ms: stat.mtime_ms, listed_at, files: own });
		}
		self.dirs.retain(|path, _| seen.contains(path));
		files
	}
}

/// Sets a file's or folder's mtime (what `utimesSync` does in the TypeScript tests).
#[cfg(test)]
pub(crate) fn set_mtime(path: &Path, ms: i64) {
	let mut options = fs::OpenOptions::new();
	options.write(true);
	#[cfg(windows)]
	std::os::windows::fs::OpenOptionsExt::custom_flags(&mut options, 0x0200_0000); // FILE_FLAG_BACKUP_SEMANTICS
	let file = options.open(path).or_else(|_| fs::File::open(path)).expect("opening for its times");
	let time = UNIX_EPOCH + std::time::Duration::from_millis(u64::try_from(ms).expect("a time after the epoch"));
	file.set_modified(time).expect("setting the mtime");
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;
	use crate::shared::fsx::remove_tree;
	use crate::shared::testing::temp_dir;

	fn title() -> String {
		json!({"type": "title", "v": 1, "title": "  demo  ", "pad": ""}).to_string()
	}

	fn header(extra: Value) -> String {
		let mut entry = json!({
			"type": "session",
			"version": 3,
			"id": "s1",
			"timestamp": "2026-11-09T10:00:00.000Z",
			"cwd": "C:\\work",
		});
		if let (Some(entry), Some(extra)) = (entry.as_object_mut(), extra.as_object()) {
			entry.extend(extra.clone());
		}
		entry.to_string()
	}

	fn plain_header() -> String {
		header(json!({}))
	}

	fn write(file: &Path, lines: &[String]) -> PathBuf {
		fs::create_dir_all(file.parent().unwrap()).unwrap();
		fs::write(file, format!("{}\n", lines.join("\n"))).unwrap();
		file.to_path_buf()
	}

	fn entry(id: &str) -> String {
		json!({ "id": id }).to_string()
	}

	/// A visitor that records the ids it was handed and how often it was reset.
	#[derive(Default)]
	struct Recorder {
		ids: Vec<String>,
		resets: usize,
	}

	impl TailVisitor for Recorder {
		fn entry(&mut self, entry: Map<String, Value>) {
			self.ids.push(entry["id"].as_str().unwrap().to_owned());
		}

		fn reset(&mut self) {
			self.ids.clear();
			self.resets += 1;
		}
	}

	#[test]
	fn read_header_finds_the_session_line_after_the_title_slot() {
		let dir = temp_dir("omp-session");
		let file = write(&dir.path().join("a.jsonl"), &[title(), header(json!({"parentSession": "s0"})), entry("e1")]);
		let started = chrono::DateTime::parse_from_rfc3339("2026-11-09T10:00:00.000Z").unwrap().timestamp_millis();
		assert_eq!(
			read_header(&file).unwrap(),
			Some(SessionHeader {
				file: file.clone(),
				cwd: "C:\\work".to_owned(),
				title: Some("demo".to_owned()),
				id: Some("s1".to_owned()),
				parent_session: Some("s0".to_owned()),
				started: Some(started),
			})
		);
		assert_eq!(read_header(&write(&dir.path().join("b.jsonl"), &[title()])).unwrap(), None);
		assert_eq!(read_header(&write(&dir.path().join("c.jsonl"), &[header(json!({"cwd": 1}))])).unwrap(), None);
	}

	#[test]
	fn transcripts_of_lists_a_sessions_nested_transcripts_sorted_gc_compress_scratch_left_out() {
		let dir = temp_dir("omp-session");
		let root = dir.path();
		let session = write(&root.join("s.jsonl"), &[plain_header()]);
		let artifacts = root.join("s");
		for name in ["1-Task.jsonl", "0-Explore.jsonl", "__advisor.jsonl", "0-Explore.omp-gc-compress-123.jsonl"] {
			write(&artifacts.join(name), &[plain_header()]);
		}
		write(&artifacts.join("0-Explore").join("0-Deep.jsonl"), &[plain_header()]);
		write(&artifacts.join(".omp-gc-compress-9").join("copy.jsonl"), &[plain_header()]);
		fs::write(artifacts.join("notes.md"), "x").unwrap();
		let mut expected = vec![
			artifacts.join("0-Explore").join("0-Deep.jsonl"),
			artifacts.join("0-Explore.jsonl"),
			artifacts.join("1-Task.jsonl"),
			artifacts.join("__advisor.jsonl"),
		];
		expected.sort_by_key(|path| path.to_string_lossy().into_owned());
		assert_eq!(transcripts_of(&session), expected);
		assert!(transcripts_of(&root.join("none.jsonl")).is_empty());
	}

	#[test]
	fn a_session_tail_reads_appended_lines_once_waits_for_a_torn_last_line_and_reads_a_rewrite_from_its_start() {
		let dir = temp_dir("omp-session");
		let root = dir.path();
		let file = write(&root.join("s.jsonl"), &[plain_header(), entry("e1")]);
		let mut tail = SessionTail::new(file.clone(), Recorder::default());
		assert!(tail.advance(None).unwrap());
		assert_eq!(tail.visitor.ids, ["s1", "e1"]);
		// Unchanged: nothing is read again.
		assert!(!tail.advance(None).unwrap());
		// A line still being written waits until it is whole.
		let append = |text: String| {
			let mut handle = fs::OpenOptions::new().append(true).open(&file).unwrap();
			std::io::Write::write_all(&mut handle, text.as_bytes()).unwrap();
		};
		append(format!("{}\n{{\"id\":\"e3", entry("e2")));
		tail.advance(None).unwrap();
		assert_eq!(tail.visitor.ids, ["s1", "e1", "e2"]);
		append("\"}\n".to_owned());
		tail.advance(None).unwrap();
		assert_eq!(tail.visitor.ids, ["s1", "e1", "e2", "e3"]);
		assert_eq!(tail.visitor.resets, 0);
		// omp's rewrite through a temp file and a rename, no shorter than what was read: read again from the start.
		let temp = write(&root.join("s.jsonl.tmp"), &[plain_header(), entry("e1"), entry("r1"), "x".repeat(60)]);
		fs::rename(temp, &file).unwrap();
		assert!(tail.advance(None).unwrap());
		assert_eq!(tail.visitor.resets, 1);
		assert_eq!(tail.visitor.ids, ["s1", "e1", "r1"]);
		// Rewritten in place, shorter than the offset: read again too.
		fs::write(&file, format!("{}\n", entry("short"))).unwrap();
		tail.advance(None).unwrap();
		assert_eq!(tail.visitor.resets, 2);
		assert_eq!(tail.visitor.ids, ["short"]);
		// A missing file leaves the tail as it was.
		remove_tree(&file).unwrap();
		assert!(!tail.advance(None).unwrap());
		assert_eq!(tail.visitor.ids, ["short"]);
	}

	#[tokio::test]
	async fn a_session_tail_read_in_steps_reads_a_line_longer_than_a_step_whole() {
		let dir = temp_dir("omp-session");
		let long = json!({"id": "long", "text": "y".repeat(5000)}).to_string();
		let file = write(&dir.path().join("s.jsonl"), &[plain_header(), long, entry("e2")]);
		let mut tail = SessionTail::new(file.clone(), Recorder::default());
		tail.advance_in_steps(64, || false).await.unwrap();
		assert_eq!(tail.visitor.ids, ["s1", "long", "e2"]);
		assert_eq!(tail.offset(), fs::metadata(&file).unwrap().len());
		assert!(!tail.advance(None).unwrap());
	}

	#[test]
	fn rewritten_another_file_under_the_name_or_one_shorter_than_the_offset_a_mark_without_identity_cant_say() {
		let dir = temp_dir("omp-session");
		let file = write(&dir.path().join("s.jsonl"), &[plain_header()]);
		let stat = FileStat::of(&file).unwrap();
		let mark = |offset, birth: f64| ReadMark {
			offset,
			ino: Some(stat.ino),
			birthtime_ms: Some(stat.birthtime_ms + birth),
		};
		assert!(!rewritten(&stat, &mark(stat.size, 0.0)));
		assert!(rewritten(&stat, &mark(stat.size + 1, 0.0)));
		assert!(rewritten(&stat, &mark(0, 1.0)));
		assert!(!rewritten(&stat, &ReadMark { offset: stat.size, ino: None, birthtime_ms: None }));
	}

	#[test]
	fn a_session_walker_lists_the_session_files_of_each_cwd_dir_and_lists_a_dir_again_only_when_its_mtime_changed() {
		let dir = temp_dir("omp-session");
		let sessions = dir.path();
		let cwd_dir = sessions.join("--C--work--");
		let first = write(&cwd_dir.join("a.jsonl"), &[plain_header()]);
		write(&cwd_dir.join("a").join("0-Task.jsonl"), &[plain_header()]);
		fs::write(sessions.join("stray.jsonl"), "").unwrap();
		let old = now_ms() - 60_000;
		set_mtime(&cwd_dir, old);
		let mut walker = SessionWalker::new(sessions.to_path_buf());
		assert_eq!(walker.files(), std::slice::from_ref(&first));
		// A file added with the dir's mtime set back is not seen: the dir is answered from the last listing.
		let second = write(&cwd_dir.join("b.jsonl"), &[plain_header()]);
		set_mtime(&cwd_dir, old);
		assert_eq!(walker.files(), std::slice::from_ref(&first));
		// Its mtime changed: listed again.
		set_mtime(&cwd_dir, now_ms() - 30_000);
		let sorted = |mut files: Vec<PathBuf>| {
			files.sort();
			files
		};
		let both = sorted(vec![first.clone(), second]);
		assert_eq!(sorted(walker.files()), both);
		// A fresh dir is listed every walk while its mtime is within the clock's tick of the listing.
		let other = sessions.join("--C--other--");
		let third = write(&other.join("c.jsonl"), &[plain_header()]);
		assert!(walker.files().contains(&third));
		let fourth = write(&other.join("d.jsonl"), &[plain_header()]);
		assert!(walker.files().contains(&fourth));
		remove_tree(&other).unwrap();
		assert_eq!(sorted(walker.files()), both);
		assert!(SessionWalker::new(sessions.join("missing")).files().is_empty());
	}
}
