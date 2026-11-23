//! Jocasta's check runner (0542, 0546, 0559): the review's checks, `done_when`, the check on idle and the thread's
//! `check` tool all run here, keyed by the git tree they ran on. The Rust port of `checks.ts`.
//!
//! - **Key:** the tree git would commit from the worktree as it is (a copy of its index, `git add -A`, `git write-tree`,
//!   the real index untouched), plus the tool versions (`mise current`) and the check's command. A key that already has
//!   a result answers from it, PASS and FAIL alike, so a check on an unchanged tree costs the key computation only.
//! - **Store:** `<data>/checks/<tree 12>/<name>.json` (`{command, versions, exit, ms, at, gates}`) beside `<name>.log`
//!   (stdout and stderr as they interleaved); a failing mise task's own lines also go to `<task>.log` in the same
//!   folder. A name's `/`, `\` and `:` become `-` in its file names. Folders untouched for 14 days are removed once a
//!   day (0582 PERF-11); `hits.log` stays.
//! - **Single flight:** one run per key, across processes: `<name>.lock` taken exclusively with the runner's pid and
//!   start time in it; a caller finding it waits for the result (joins), and a lock whose process is gone (or whose
//!   pid now names a process started at another time), or held past the check's time plus a grace, is taken over. In
//!   one process a second caller joins the first's run. A run's checks run one after another.
//! - **Queue (0671):** with an [`Admission`], a check that has to run (no stored result, no run to join) waits for its
//!   turn, by what it cost on earlier runs (`check_costs.rs`, `<data>/checks/costs.json`). The timeout starts once it
//!   is admitted. A waiter in another process isn't queued here: its own deadline (the check's time plus a grace)
//!   may pass while the holder is queued; all Jocasta checks run in this process.
//! - **mise:** the project's check runs as `mise run -c <task>`, so a failing task doesn't abort its siblings; mise's
//!   own artifact cache replays unchanged tasks underneath only when the project's `mise.toml` switches it on. Jocasta
//!   never writes a project's files.
//! - **Gates (0546):** each check (each mise task the run names) is one PASS/FAIL line; a failure gets its failures,
//!   its log and the narrowest rerun: `bun test <the failing files>` when bun's test runner named them, else the
//!   check's own command. Every gate passing says nothing.
//!
//! Shared hits (a result reused, or a run joined, rather than run here) are counted for H8 (K11): one line per hit in
//! `<data>/checks/hits.log`, which [`read_check_hits`] counts.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Weak};
use std::time::{Duration, UNIX_EPOCH};

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use process_wrap::tokio::ChildWrapper;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::frontier::protocol::ChecksLoad;
use crate::frontier::server::check_costs::{CheckCosts, Sampler};
use crate::shared::clock::now_ms;
use crate::shared::day::local_day;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, remove_tree_async, write_atomically};
use crate::shared::git::{GitOptions, git_async};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::pollers::machine::available_memory_bytes;
use crate::shared::process::{CREATE_NO_WINDOW, RunOptions, on_path, run_program, wrap_async};
use crate::shared::proctree::{pid_alive, process_start};
use crate::shared::settings::read_check_reserve_gb;

/// One check to run: its gate name (`verify`, `done_when`, `done_when-2`) and its shell command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckSpec {
	pub name: String,
	pub command: String,
	/// Run through mise (`mise run -c <task>`): its output names each task's lines and failures.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub mise: bool,
}

/// One failing test as the runner named it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestFailure {
	pub file: String,
	pub line: Option<u32>,
	pub test: String,
}

/// One PASS/FAIL line of a gate reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
	pub name: String,
	pub command: String,
	pub pass: bool,
	/// The failing tests the runner named; empty when it named none.
	pub failures: Vec<TestFailure>,
	/// The log to read; set on a failure.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub log: Option<PathBuf>,
	/// The narrowest command that runs the failure again; set on a failure.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rerun: Option<String>,
}

/// How a check's result was had: `Ran` here, `Hit` a stored result, `Joined` another caller's run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
	Ran,
	Hit,
	Joined,
}

/// One check's result for a key, and how it was had.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
	pub spec: CheckSpec,
	/// The command's exit code; `None` when it couldn't run or ran out of time (never stored).
	pub exit: Option<i32>,
	pub log: PathBuf,
	pub ms: i64,
	pub how: How,
	pub gates: Vec<Gate>,
}

/// The checks of one tree.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckRun {
	pub tree: String,
	pub results: Vec<CheckResult>,
	pub passed: bool,
}

/// What a runner's versions lookup is: `mise current` in a folder with an environment, or the tests' own.
pub type VersionsFn = Arc<dyn Fn(PathBuf, HashMap<String, String>) -> BoxFuture<'static, String> + Send + Sync>;

#[derive(Default)]
pub struct CheckRunnerOptions {
	/// `<data>/checks`.
	pub dir: PathBuf,
	/// The tool versions in the key: `mise current` in `cwd` by default; "" without mise.
	pub versions: Option<VersionsFn>,
	/// How long one check may run before it is killed (and nothing stored); 30 minutes.
	pub timeout_ms: Option<i64>,
	/// How often a waiter looks for the running check's result; 250 ms.
	pub poll_ms: Option<i64>,
	/// The check queue (0671); `None` runs every check at once.
	pub admission: Option<Admission>,
}

const CHECK_TIMEOUT_MS: i64 = 30 * 60_000;
const POLL_MS: i64 = 250;
/// How long past the check's own time a waiter gives a live lock holder before taking its lock over.
const LOCK_GRACE_MS: i64 = 60_000;
/// How long one git call of the tree key may take.
const GIT_TIMEOUT_MS: i64 = 30_000;
const DAY_MS: i64 = 24 * 60 * 60_000;
/// Days a stored result is kept after its folder was last touched (0582 PERF-11).
const RESULT_DAYS: i64 = 14;
const PRUNE_EVERY_MS: u64 = 24 * 60 * 60_000;
/// The file in the checks folder holding the local day the last prune pass ran (once a day, 0582 PERF-11).
const PRUNED_ON: &str = "pruned-on";
/// Index copies in `tmp` older than this are a crash's leftovers.
const TMP_MAX_AGE_MS: i64 = 60 * 60_000;
const STOPPED: &str = "the check runner has stopped";

/// Stored with each result.
struct Stored {
	exit: i32,
	ms: i64,
	/// The result's gates; absent from results stored before they were kept, whose log is read instead.
	gates: Option<Vec<Gate>>,
}

/// The `Stored` JSON: `{command, versions, exit, ms, at, gates}`.
#[derive(Serialize)]
struct StoredOut<'a> {
	command: &'a str,
	versions: &'a str,
	exit: i32,
	ms: i64,
	at: i64,
	gates: &'a [Gate],
}

/// A result folder's name: the tree's first 12 hex digits.
fn is_result_folder(name: &str) -> bool {
	name.len() == 12 && name.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The process's environment (and the test overlay) as a map.
fn process_env() -> HashMap<String, String> {
	env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect()
}

/// The command that runs one shell command line: cmd.exe on Windows (`/d /s /c "<line>"`, verbatim), else `sh -c`.
fn shell_command(command: &str) -> tokio::process::Command {
	#[cfg(windows)]
	{
		let mut shell = tokio::process::Command::new(env::var("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned()));
		shell.as_std_mut().raw_arg(format!("/d /s /c \"{command}\""));
		shell
	}
	#[cfg(not(windows))]
	{
		let mut shell = tokio::process::Command::new("sh");
		shell.arg("-c").arg(command);
		shell
	}
}

#[cfg(windows)]
use std::os::windows::process::CommandExt as _;

/// `mise current` in `cwd` (the resolved tool versions), "" when mise isn't there or fails.
async fn mise_current(cwd: PathBuf, env: HashMap<String, String>) -> String {
	let Some(mise) = on_path("mise", Some(&env)) else { return String::new() };
	let argv = [mise.into_os_string(), "current".into()];
	let options = RunOptions { cwd: Some(cwd), env: Some(env), timeout_ms: Some(15_000), ..RunOptions::default() };
	match run_program(&argv, options).await {
		Ok(run) if run.code == Some(0) => run.stdout.trim().to_owned(),
		_ => String::new(),
	}
}

/// The escape character is what an ANSI color code starts with.
static ANSI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("the ANSI pattern"));

/// A mise task's output prefix: `[test] …`.
static TASK_PREFIX: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\[([A-Za-z0-9_:./-]+)\] ?(.*)$").expect("the task prefix pattern"));

static EXITED_WITH: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"\[([A-Za-z0-9_:./-]+)\] exited with status [0-9]+").expect("the exit status pattern")
});

static TASK_FAILED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^ERROR\b.*\bfailed\b").expect("the task failure pattern"));

static TEST_FILE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^(\S.*\.(?:test|spec|_test|_spec)\.[cm]?[jt]sx?):$").expect("the test file pattern"));

static STACK_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"([^\s()]+):([0-9]+):[0-9]+\)?\s*$").expect("the stack line pattern"));

static FAILED_TEST: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*\(fail\) (.+?)(?: \[[0-9.]+m?s\])?\s*$").expect("the failed test pattern"));

static PASSED_TEST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\(pass\) ").expect("the passed test pattern"));

/// A log's lines, split at `\n` or `\r\n`.
fn lines_of(text: &str) -> impl Iterator<Item = &str> {
	text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line))
}

/// Notes task `name`: new ones are added; a known one changes only when `only_new` is not set.
fn set_task(tasks: &mut Vec<MiseTask>, name: &str, failed: bool, only_new: bool) {
	match tasks.iter_mut().find(|known| known.name == name) {
		Some(known) if !only_new => known.failed = failed,
		Some(_) => {}
		None => tasks.push(MiseTask { name: name.to_owned(), failed }),
	}
}

/// One task a mise log names, and whether it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiseTask {
	pub name: String,
	pub failed: bool,
}

/// The tasks a `mise run` log names, in order, and which failed: a task's lines start `[<task>] `; mise ends a failed
/// one with `[<task>] ERROR task failed` and sums them up as `[<task>] exited with status <n>`.
pub fn mise_tasks(text: &str) -> Vec<MiseTask> {
	let mut tasks: Vec<MiseTask> = Vec::new();
	for raw in lines_of(text) {
		let line = ANSI.replace_all(raw, "");
		for found in EXITED_WITH.captures_iter(&line) {
			set_task(&mut tasks, &found[1], true, false);
		}
		let Some(prefixed) = TASK_PREFIX.captures(&line) else { continue };
		set_task(&mut tasks, &prefixed[1], false, true);
		if TASK_FAILED.is_match(&prefixed[2]) {
			set_task(&mut tasks, &prefixed[1], true, false);
		}
	}
	tasks
}

/// A task's own lines of a mise log, their prefix cut.
fn task_lines(text: &str, task: &str) -> String {
	let mut out: Vec<String> = Vec::new();
	for raw in lines_of(text) {
		let line = ANSI.replace_all(raw, "");
		if let Some(prefixed) = TASK_PREFIX.captures(&line)
			&& &prefixed[1] == task
		{
			out.push(prefixed[2].to_owned());
		}
	}
	format!("{}\n", out.join("\n"))
}

/// A path as a test runner prints it, made relative to `cwd` with forward slashes.
fn relative_to(cwd: &Path, path: &str) -> String {
	let slashed = path.replace('\\', "/");
	let root = format!("{}/", cwd.to_string_lossy().replace('\\', "/").trim_end_matches('/'));
	match slashed.get(..root.len()) {
		Some(head) if head.to_lowercase() == root.to_lowercase() => slashed[root.len()..].to_owned(),
		_ => slashed,
	}
}

/// The failing tests bun's test runner names in `text`: each file's results follow a `<file>:` line, a failure is
/// `(fail) <name>`, and its error's stack, printed before it, points at the test file's line. Lines a mise task
/// prefixed are read without the prefix.
pub fn bun_failures(text: &str, cwd: &Path) -> Vec<TestFailure> {
	let mut failures = Vec::new();
	let mut file: Option<String> = None;
	let mut line: Option<u32> = None;
	for raw in lines_of(text) {
		let stripped = ANSI.replace_all(raw, "");
		let row = match TASK_PREFIX.captures(&stripped) {
			Some(prefixed) => prefixed[2].to_owned(),
			None => stripped.into_owned(),
		};
		if let Some(header) = TEST_FILE.captures(row.trim()) {
			file = Some(relative_to(cwd, &header[1]));
			line = None;
			continue;
		}
		if let Some(file) = &file {
			let name = file.rsplit('/').next().unwrap_or("");
			for at in STACK_LINE.captures_iter(&row) {
				if at[1].replace('\\', "/").ends_with(name) {
					line = at[2].parse().ok();
				}
			}
		}
		let failed = FAILED_TEST.captures(&row);
		match (failed, &file) {
			(Some(failed), Some(file)) => {
				failures.push(TestFailure { file: file.clone(), line, test: failed[1].to_owned() });
				line = None;
			}
			_ if PASSED_TEST.is_match(&row) => line = None,
			_ => {}
		}
	}
	failures
}

/// One gate of a log: a failing one gets its failures, the log to read and the narrowest rerun.
fn gate_of((name, command): (&str, &str), pass: bool, lines: &str, log_path: &Path, cwd: &Path) -> Gate {
	let failures = if pass { Vec::new() } else { bun_failures(lines, cwd) };
	let mut files: Vec<&str> = Vec::new();
	for failure in &failures {
		if !files.contains(&failure.file.as_str()) {
			files.push(&failure.file);
		}
	}
	let rerun = files
		.iter()
		.map(|file| if file.contains(' ') { format!("\"{file}\"") } else { (*file).to_owned() })
		.collect::<Vec<_>>()
		.join(" ");
	let (log, rerun) = if pass {
		(None, None)
	} else {
		(
			Some(log_path.to_path_buf()),
			Some(if files.is_empty() { command.to_owned() } else { format!("bun test {rerun}") }),
		)
	};
	Gate { name: name.to_owned(), command: command.to_owned(), pass, failures, log, rerun }
}

/// The gates of one check's log: one per mise task it names, else the check itself.
pub fn gates_of(spec: &CheckSpec, exit: Option<i32>, text: &str, log_file: &Path, cwd: &Path) -> Vec<Gate> {
	let tasks = if spec.mise { mise_tasks(text) } else { Vec::new() };
	let named: Vec<&MiseTask> = tasks.iter().filter(|task| task.name != spec.name || tasks.len() == 1).collect();
	if named.len() > 1 || (named.len() == 1 && named[0].name != spec.name) {
		let any_failed = named.iter().any(|task| task.failed);
		return named
			.iter()
			.map(|MiseTask { name, failed }| {
				// A failing run whose log names no failed task: every task it named is said as failing.
				let pass = exit == Some(0) || (any_failed && !failed);
				let task_log = log_file.parent().unwrap_or(Path::new("")).join(format!("{}.log", file_name(name)));
				let log_path = if pass { log_file } else { &task_log };
				gate_of((name, &format!("mise run {name}")), pass, text, log_path, cwd)
			})
			.collect();
	}
	vec![gate_of((&spec.name, &spec.command), exit == Some(0), text, log_file, cwd)]
}

/// `3f9a1c` of a tree.
pub fn short_tree(tree: &str) -> String {
	tree.chars().take(6).collect()
}

/// 0546's gate reply for a run; "" when every gate passes. Lines: `checks: FAIL (tree 3f9a1c)`, then per gate
/// `  PASS  <name padded>  <command>`; a failing gate adds its failures, `log:` and `rerun:` lines under it.
pub fn gate_reply(run: &CheckRun) -> String {
	let gates: Vec<&Gate> = run.results.iter().flat_map(|result| &result.gates).collect();
	if run.passed {
		return String::new();
	}
	let width = gates.iter().map(|gate| gate.name.chars().count()).max().unwrap_or(0);
	let mut lines = vec![format!("checks: FAIL (tree {})", short_tree(&run.tree))];
	for gate in gates {
		let state = if gate.pass { "PASS" } else { "FAIL" };
		lines.push(format!("  {state}  {:<width$}  {}", gate.name, gate.command));
		if gate.pass {
			continue;
		}
		let pad = " ".repeat(8);
		if !gate.failures.is_empty() {
			let said: Vec<String> = gate
				.failures
				.iter()
				.map(|f| {
					format!("{}{} \"{}\"", f.file, f.line.map_or_else(String::new, |line| format!(":{line}")), f.test)
				})
				.collect();
			lines.push(format!("{pad}{} failed: {}", gate.failures.len(), said.join(", ")));
		}
		if let Some(log) = &gate.log {
			lines.push(format!("{pad}log: {}", log.display()));
		}
		if let Some(rerun) = &gate.rerun {
			lines.push(format!("{pad}rerun: {rerun}"));
		}
	}
	lines.join("\n")
}

/// How many shared check hits `<data>/checks/hits.log` holds (H8, K11).
pub fn read_check_hits(checks_dir: &Path) -> usize {
	std::fs::read_to_string(checks_dir.join("hits.log"))
		.map_or(0, |text| text.split('\n').filter(|l| !l.is_empty()).count())
}

/// The checks' folder under a data dir.
pub fn checks_dir(data: &Path) -> PathBuf {
	data.join("checks")
}

/// How long a waiting check goes without looking at the queue again when nothing in it changed: free memory moves on
/// its own.
const RECHECK_MS: u64 = 2_000;

struct Running {
	id: u64,
	bytes: u64,
	cores: f64,
	/// A check with no record: it runs alone.
	alone: bool,
}

#[derive(Default)]
struct Queue {
	last: u64,
	running: Vec<Running>,
	waiting: VecDeque<u64>,
	closed: bool,
}

enum Turn {
	Admitted,
	Wait,
	Closed,
}

/// The check queue (0671): which checks may run now, from what each has cost before.
///
/// - A check with a record is admitted when its recorded peak plus the peaks of the checks running fit in the free
///   memory minus the reserve, and its cores plus theirs fit in the machine's cores. Otherwise it waits, first come
///   first served: a later check never passes an earlier one.
/// - A check with no record runs alone, once: it waits for the running checks to end and nothing starts while it
///   runs; its measurement becomes the record.
/// - Nothing running admits the head of the queue whatever it costs, so a check bigger than the machine still runs.
/// - Free memory is read as it is when a waiter looks (on every change in the queue, and every [`RECHECK_MS`]), and
///   the running checks' peaks are counted whole although part of them is already gone from it: the queue errs tight.
pub struct Admission {
	free_bytes: Arc<dyn Fn() -> u64 + Send + Sync>,
	cores: usize,
	reserve_bytes: u64,
	costs: CheckCosts,
	queue: Mutex<Queue>,
	/// Bumped at every change of the queue, so a waiter looks again.
	changed: watch::Sender<u64>,
}

/// A check's place in the queue while it waits; leaves it when the wait is dropped.
struct Ticket<'a> {
	admission: &'a Admission,
	id: u64,
	queued: bool,
}

impl Drop for Ticket<'_> {
	fn drop(&mut self) {
		if self.queued {
			locked(&self.admission.queue).waiting.retain(|id| *id != self.id);
			self.admission.changed.send_modify(|n| *n += 1);
		}
	}
}

/// A check's right to run; ends when dropped.
pub struct Permit {
	admission: Arc<Admission>,
	id: u64,
	key: String,
}

impl Permit {
	/// Notes what the run cost; the next check admitted reads it.
	pub fn record(&self, peak_rss_bytes: u64, avg_cores: f64) {
		self.admission.costs.record(&self.key, peak_rss_bytes, avg_cores, now_ms());
	}
}

impl Drop for Permit {
	fn drop(&mut self) {
		locked(&self.admission.queue).running.retain(|running| running.id != self.id);
		self.admission.changed.send_modify(|n| *n += 1);
	}
}

impl Admission {
	/// The queue as the service runs it: the machine's free memory and cores, `checks.memoryReserveGb` kept free, and
	/// the records in `<data>/checks/costs.json`. Where the platform can't tell the free memory, memory never holds a
	/// check.
	pub fn from_settings() -> Self {
		let reserve = (read_check_reserve_gb() * 1_073_741_824.0) as u64;
		let free: Arc<dyn Fn() -> u64 + Send + Sync> = Arc::new(|| available_memory_bytes().unwrap_or(u64::MAX));
		let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
		Self::with(free, cores, reserve, CheckCosts::load(checks_dir(&data_dir()).join("costs.json")))
	}

	/// A queue with its numbers given (the tests').
	pub fn with(
		free_bytes: Arc<dyn Fn() -> u64 + Send + Sync>,
		cores: usize,
		reserve_bytes: u64,
		costs: CheckCosts,
	) -> Self {
		Self {
			free_bytes,
			cores,
			reserve_bytes,
			costs,
			queue: Mutex::new(Queue::default()),
			changed: watch::channel(0).0,
		}
	}

	/// Checks running and waiting now.
	pub fn load(&self) -> ChecksLoad {
		let queue = locked(&self.queue);
		ChecksLoad { running: queue.running.len() as u32, waiting: queue.waiting.len() as u32 }
	}

	/// Releases every waiter (they get `None`) and admits no one after.
	pub fn close(&self) {
		locked(&self.queue).closed = true;
		self.changed.send_modify(|n| *n += 1);
	}

	fn turn(&self, id: u64, key: &str) -> Turn {
		let mut queue = locked(&self.queue);
		if queue.closed {
			return Turn::Closed;
		}
		if queue.waiting.front() != Some(&id) {
			return Turn::Wait;
		}
		let cost = self.costs.get(key);
		let fits = queue.running.is_empty()
			|| cost.is_some_and(|cost| {
				let held: u64 = queue.running.iter().map(|running| running.bytes).sum();
				let busy: f64 = queue.running.iter().map(|running| running.cores).sum();
				let room = (self.free_bytes)().saturating_sub(self.reserve_bytes);
				!queue.running.iter().any(|running| running.alone)
					&& held.saturating_add(cost.peak_rss_bytes) <= room
					&& busy + cost.avg_cores <= self.cores as f64
			});
		if !fits {
			return Turn::Wait;
		}
		queue.waiting.pop_front();
		queue.running.push(Running {
			id,
			bytes: cost.map_or(0, |cost| cost.peak_rss_bytes),
			cores: cost.map_or(0.0, |cost| cost.avg_cores),
			alone: cost.is_none(),
		});
		drop(queue);
		// The next in line may fit beside this one.
		self.changed.send_modify(|n| *n += 1);
		Turn::Admitted
	}

	/// Waits for `command`'s turn in `project`; `None` when the queue was closed. Dropping the wait leaves the queue.
	pub async fn admit(self: &Arc<Self>, project: &Path, command: &str) -> Option<Permit> {
		let key = CheckCosts::key(project, command);
		let mut changes = self.changed.subscribe();
		let id = {
			let mut queue = locked(&self.queue);
			if queue.closed {
				return None;
			}
			queue.last += 1;
			let id = queue.last;
			queue.waiting.push_back(id);
			id
		};
		let mut ticket = Ticket { admission: self, id, queued: true };
		let mut said = false;
		loop {
			match self.turn(id, &key) {
				Turn::Admitted => {
					ticket.queued = false;
					return Some(Permit { admission: Arc::clone(self), id, key });
				}
				Turn::Closed => return None,
				Turn::Wait => {}
			}
			if !said {
				said = true;
				let load = self.load();
				let (running, waiting) = (load.running, load.waiting);
				let line = format!("`{command}` waits in the check queue ({running} running, {waiting} waiting)");
				log(LogLevel::Info, "checks", &line, None);
			}
			tokio::select! {
				_ = changes.changed() => {}
				() = tokio::time::sleep(Duration::from_millis(RECHECK_MS)) => {}
			}
		}
	}
}

/// The folder of the project a check's folder belongs to: the folder holding the repository's `.git` (a worktree's
/// own folder is the project's), so the project's worktrees share one record. `cwd` itself outside git.
async fn project_of(cwd: &Path) -> PathBuf {
	let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
	let Ok(out) = git_async(cwd, &args, GIT_TIMEOUT_MS, GitOptions::default()).await else { return cwd.to_path_buf() };
	let common = PathBuf::from(out.stdout.trim());
	if out.code != Some(0) || !common.is_absolute() {
		return cwd.to_path_buf();
	}
	let in_git_dir = common.file_name().is_some_and(|name| name == ".git");
	match common.parent() {
		Some(parent) if in_git_dir => parent.to_path_buf(),
		_ => common,
	}
}

type Flight = Shared<BoxFuture<'static, std::result::Result<CheckResult, String>>>;

/// What one keyed run needs, owned so the run can outlive its first caller.
struct Job {
	cwd: PathBuf,
	tree: String,
	versions: String,
	spec: CheckSpec,
	env: HashMap<String, String>,
}

struct Inner {
	dir: PathBuf,
	timeout_ms: i64,
	poll_ms: i64,
	versions: VersionsFn,
	hits: AtomicUsize,
	runs: AtomicUsize,
	in_flight: Mutex<HashMap<String, Flight>>,
	/// Set by `stop`: every check running here ends its own tree (its job or group, 0641).
	halt: watch::Sender<bool>,
	stopped: AtomicBool,
	/// The check queue; `None` runs every check at once.
	admission: Option<Arc<Admission>>,
}

/// The check runner: one per service (or per test), runs checks keyed by git tree.
pub struct CheckRunner {
	inner: Arc<Inner>,
	/// The day's prune pass, started with the runner unless one already ran today (0582 PERF-11).
	pruning: Mutex<Option<JoinHandle<()>>>,
	prune_timer: JoinHandle<()>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl CheckRunner {
	/// Starts a runner; the day's prune pass and its daily timer run on the current tokio runtime.
	pub fn new(options: CheckRunnerOptions) -> Self {
		let versions: VersionsFn =
			options.versions.unwrap_or_else(|| Arc::new(|cwd, env| mise_current(cwd, env).boxed()));
		let inner = Arc::new(Inner {
			dir: options.dir,
			timeout_ms: options.timeout_ms.unwrap_or(CHECK_TIMEOUT_MS),
			poll_ms: options.poll_ms.unwrap_or(POLL_MS),
			versions,
			hits: AtomicUsize::new(0),
			runs: AtomicUsize::new(0),
			in_flight: Mutex::new(HashMap::new()),
			halt: watch::channel(false).0,
			stopped: AtomicBool::new(false),
			admission: options.admission.map(Arc::new),
		});
		// Stored results age out (0582 PERF-11): once a day, at the service's first start that day and daily while it runs.
		let first = Arc::clone(&inner);
		let pruning = tokio::spawn(async move { first.prune_daily().await });
		let weak: Weak<Inner> = Arc::downgrade(&inner);
		let prune_timer = tokio::spawn(async move {
			loop {
				tokio::time::sleep(Duration::from_millis(PRUNE_EVERY_MS)).await;
				let Some(inner) = weak.upgrade() else { break };
				inner.prune_daily().await;
			}
		});
		Self { inner, pruning: Mutex::new(Some(pruning)), prune_timer }
	}

	/// Results reused or runs joined, rather than run here (K11's shared check hits).
	pub fn hits(&self) -> usize {
		self.inner.hits.load(Ordering::SeqCst)
	}

	/// Checks run here.
	pub fn runs(&self) -> usize {
		self.inner.runs.load(Ordering::SeqCst)
	}

	/// The checks running and waiting in the queue; none without one.
	pub fn load(&self) -> ChecksLoad {
		self.inner.admission.as_ref().map_or(ChecksLoad { running: 0, waiting: 0 }, |admission| admission.load())
	}

	/// Resolves once the day's prune pass the runner started is done.
	pub async fn pruning(&self) {
		let handle = locked(&self.pruning).take();
		if let Some(handle) = handle {
			let _ = handle.await;
		}
	}

	/// Ends every check running here (their results aren't stored); later runs answer from stored results only.
	/// Checks waiting in the queue are released.
	pub async fn stop(&self) {
		self.inner.stopped.store(true, Ordering::SeqCst);
		self.prune_timer.abort();
		self.inner.halt.send_replace(true);
		if let Some(admission) = &self.inner.admission {
			admission.close();
		}
		let flights: Vec<Flight> = locked(&self.inner.in_flight).values().cloned().collect();
		futures_util::future::join_all(flights).await;
	}

	/// Removes the result folders untouched for 14 days (the newest mtime of the folder and its files) and `tmp`
	/// entries older than an hour; `hits.log` stays (H8, K11). One log line when something was removed (0582 PERF-11).
	/// Never fails: a failure is logged. `now` defaults to the current time.
	pub async fn prune(&self, now: Option<i64>) {
		self.inner.prune(now.unwrap_or_else(now_ms)).await;
	}

	/// The tree git would commit from `cwd` as it is now: tracked and untracked files not ignored, edits included,
	/// written through a copy of the worktree's index so the real one is never touched. `None` outside git, or when
	/// git fails or runs past its time.
	pub async fn tree_key(&self, cwd: &Path) -> Result<Option<String>> {
		self.inner.tree_key(cwd).await
	}

	/// Runs `specs` on `cwd`'s tree with `env`, each answered from its stored result when the key has one; `None`
	/// outside git or without specs. `tree` is the caller's own [`CheckRunner::tree_key`] of `cwd`, when it has one.
	/// The specs run one after another: a project's tests and a `done_when` in one worktree may share ports, temp and
	/// build files, and a clash would be stored as the tree's FAIL (0559).
	pub async fn run(
		&self,
		cwd: &Path,
		specs: &[CheckSpec],
		env: &HashMap<String, String>,
		tree: Option<&str>,
	) -> Result<Option<CheckRun>> {
		if specs.is_empty() {
			return Ok(None);
		}
		let key = match tree {
			Some(tree) => tree.to_owned(),
			None => match self.inner.tree_key(cwd).await? {
				Some(key) => key,
				None => return Ok(None),
			},
		};
		let versions = (self.inner.versions)(cwd.to_path_buf(), env.clone()).await;
		let mut results = Vec::new();
		for spec in specs {
			let job = Job {
				cwd: cwd.to_path_buf(),
				tree: key.clone(),
				versions: versions.clone(),
				spec: spec.clone(),
				env: env.clone(),
			};
			results.push(self.inner.one(job).await?);
		}
		let passed = results.iter().all(|result| result.gates.iter().all(|gate| gate.pass));
		Ok(Some(CheckRun { tree: key, results, passed }))
	}
}

impl Drop for CheckRunner {
	fn drop(&mut self) {
		self.prune_timer.abort();
	}
}

impl Inner {
	fn stopped(&self) -> bool {
		self.stopped.load(Ordering::SeqCst)
	}

	/// `prune`, unless a pass already ran today; the day is kept in `<dir>/pruned-on`.
	async fn prune_daily(&self) {
		let stamp = self.dir.join(PRUNED_ON);
		let today = local_day(now_ms());
		if std::fs::read_to_string(&stamp).is_ok_and(|text| text.trim() == today) {
			return;
		}
		self.prune(now_ms()).await;
		if self.stopped() {
			return;
		}
		// No checks folder yet means nothing was stored to prune.
		if let Err(error) = std::fs::write(&stamp, format!("{today}\n"))
			&& error.kind() != std::io::ErrorKind::NotFound
		{
			log(LogLevel::Warn, "checks", &format!("couldn't note today's prune: {error}"), None);
		}
	}

	async fn prune(&self, now: i64) {
		let names = match std::fs::read_dir(&self.dir) {
			Ok(entries) => entries.filter_map(|entry| entry.ok()?.file_name().into_string().ok()).collect::<Vec<_>>(),
			Err(error) => {
				if error.kind() != std::io::ErrorKind::NotFound {
					log(LogLevel::Warn, "checks", &format!("couldn't prune stored results: {error}"), None);
				}
				return;
			}
		};
		let mut removed = 0;
		let mut bytes = 0;
		for name in names {
			if self.stopped() {
				return;
			}
			if !is_result_folder(&name) {
				continue;
			}
			let folder = self.dir.join(&name);
			match remove_if_old(&folder, now).await {
				Ok(Some(size)) => {
					removed += 1;
					bytes += size;
				}
				Ok(None) => {}
				Err(error) => {
					let said = format!("couldn't remove stored results {}: {error}", folder.display());
					log(LogLevel::Warn, "checks", &said, None);
				}
			}
		}
		self.prune_tmp(now).await;
		if removed > 0 {
			let said =
				format!("removed {removed} stored results older than {RESULT_DAYS} days ({})", size_said(bytes as f64));
			log(LogLevel::Info, "checks", &said, None);
		}
	}

	/// Removes `tmp` entries (index copies a crash left) older than an hour.
	async fn prune_tmp(&self, now: i64) {
		let Ok(entries) = std::fs::read_dir(self.dir.join("tmp")) else { return };
		for entry in entries.filter_map(|entry| entry.ok()) {
			let path = entry.path();
			let old = std::fs::metadata(&path).map(|meta| now - mtime_ms(&meta) >= TMP_MAX_AGE_MS);
			let removed = match old {
				Ok(true) => remove_tree_async(&path).await,
				Ok(false) => Ok(()),
				Err(error) => Err(Error::from(error)),
			};
			if let Err(error) = removed {
				let gone = matches!(&error, Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound);
				if !gone {
					log(LogLevel::Warn, "checks", &format!("couldn't remove {}: {error}", path.display()), None);
				}
			}
		}
	}

	async fn tree_key(&self, cwd: &Path) -> Result<Option<String>> {
		let index_path =
			git_async(cwd, &["rev-parse", "--git-path", "index"], GIT_TIMEOUT_MS, GitOptions::default()).await?;
		if index_path.code != Some(0) {
			return Ok(None);
		}
		let index = PathBuf::from(index_path.stdout.trim());
		let real = if index.is_absolute() { index } else { cwd.join(index) };
		let tmp_dir = self.dir.join("tmp");
		std::fs::create_dir_all(&tmp_dir)?;
		let short = uuid::Uuid::new_v4().simple().to_string();
		let temp = tmp_dir.join(format!("index-{}-{}", std::process::id(), &short[..8]));
		let tree = tree_of(cwd, &real, &temp).await;
		let _ = std::fs::remove_file(&temp);
		tree
	}

	fn hit(&self) {
		self.hits.fetch_add(1, Ordering::SeqCst);
		let counted = std::fs::create_dir_all(&self.dir).and_then(|()| {
			let mut file = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("hits.log"))?;
			writeln!(file, "{}", now_ms())
		});
		if let Err(error) = counted {
			log(LogLevel::Warn, "checks", &format!("couldn't count a shared hit: {error}"), None);
		}
	}

	/// One check of a run: a second caller of a key already running here joins its run.
	async fn one(self: &Arc<Self>, job: Job) -> Result<CheckResult> {
		let key = format!("{}\0{}\0{}\0{}", job.tree, job.spec.name, job.spec.command, job.versions);
		let (flight, joined) = {
			let mut flights = locked(&self.in_flight);
			if let Some(running) = flights.get(&key) {
				(running.clone(), true)
			} else {
				let inner = Arc::clone(self);
				let task_key = key.clone();
				let task = tokio::spawn(async move {
					let result = inner.keyed(job).await;
					locked(&inner.in_flight).remove(&task_key);
					result.map_err(|error| error.to_string())
				});
				let flight: Flight = async move { task.await.map_err(|error| error.to_string())? }.boxed().shared();
				flights.insert(key, flight.clone());
				(flight, false)
			}
		};
		if joined {
			self.hit();
		}
		let mut result = flight.await.map_err(Error::other)?;
		if joined {
			result.how = How::Joined;
		}
		Ok(result)
	}

	fn stored(&self, file: &Path, spec: &CheckSpec, versions: &str) -> Option<Stored> {
		let value = read_json_object(file)?;
		if value.get("command").and_then(Value::as_str) != Some(spec.command.as_str())
			|| value.get("versions").and_then(Value::as_str) != Some(versions)
		{
			return None;
		}
		let exit = value.get("exit")?.as_f64()? as i32;
		let ms = value.get("ms").and_then(Value::as_f64).unwrap_or(0.0) as i64;
		let gates = value
			.get("gates")
			.filter(|gates| gates.is_array())
			.and_then(|gates| serde_json::from_value(gates.clone()).ok());
		Some(Stored { exit, ms, gates })
	}

	/// A stored result as this caller's; its gates as stored, or read from its log when stored without them.
	fn answer(job: &Job, folder: &Path, stored: Stored, how: How) -> CheckResult {
		let log_file = folder.join(format!("{}.log", file_name(&job.spec.name)));
		let gates = stored.gates.unwrap_or_else(|| {
			let text = read_text(&log_file).unwrap_or_default();
			gates_of(&job.spec, Some(stored.exit), &text, &log_file, &job.cwd)
		});
		CheckResult { spec: job.spec.clone(), exit: Some(stored.exit), log: log_file, ms: stored.ms, how, gates }
	}

	async fn keyed(&self, job: Job) -> Result<CheckResult> {
		let folder = self.dir.join(job.tree.chars().take(12).collect::<String>());
		std::fs::create_dir_all(&folder)?;
		let name = file_name(&job.spec.name);
		let result_file = folder.join(format!("{name}.json"));
		let lock_file = folder.join(format!("{name}.lock"));
		loop {
			if let Some(done) = self.stored(&result_file, &job.spec, &job.versions) {
				self.hit();
				return Ok(Self::answer(&job, &folder, done, How::Hit));
			}
			if self.stopped() {
				return Err(Error::other(STOPPED));
			}
			let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(&lock_file) else {
				// Someone runs this key: wait for its result, or take the lock over once its process is gone.
				if let Some(done) = self.wait_on_holder(&job, &folder, &result_file, &lock_file).await {
					return Ok(done);
				}
				continue;
			};
			let own = lock_token();
			let ran = match file.write_all(own.as_bytes()) {
				Ok(()) => {
					drop(file);
					self.execute(&job, &folder).await
				}
				Err(error) => Err(Error::from(error)),
			};
			// Only our own lock: one taken over past its time is another runner's now.
			if read_text(&lock_file).as_deref() == Some(own.as_str()) {
				let _ = std::fs::remove_file(&lock_file);
			}
			return ran;
		}
	}

	/// A lock is held: takes it over when its process is gone, else waits for the result. The joined result, or
	/// `None` when the caller looks again.
	async fn wait_on_holder(
		&self,
		job: &Job,
		folder: &Path,
		result_file: &Path,
		lock_file: &Path,
	) -> Option<CheckResult> {
		let spec_name = &job.spec.name;
		let tree = short_tree(&job.tree);
		let held = read_text(lock_file);
		if let Some(held) = held.as_deref().filter(|held| holder_gone(held)) {
			let pid = lock_holder(held).0;
			log(
				LogLevel::Info,
				"checks",
				&format!("taking over {spec_name}'s lock on tree {tree}: process {pid} is gone"),
				None,
			);
			take_over(lock_file, held);
			return None;
		}
		let waited = self.wait_for(result_file, lock_file, &job.spec, &job.versions).await;
		if let Some(stored) = waited.stored {
			self.hit();
			return Some(Self::answer(job, folder, stored, How::Joined));
		}
		if let Some(overdue) = waited.overdue {
			let pid = lock_holder(&overdue).0;
			let said =
				format!("taking over {spec_name}'s lock on tree {tree}: process {pid} held it past the check's time");
			log(LogLevel::Info, "checks", &said, None);
			take_over(lock_file, &overdue);
		}
		None
	}

	/// Waits while the lock is held by a live process. `stored` once the result is there (or the lock went without
	/// one: none); `overdue` with the lock's text once it was held past the check's time plus a grace; neither once the
	/// holder is gone or this runner stopped.
	async fn wait_for(&self, result_file: &Path, lock_file: &Path, spec: &CheckSpec, versions: &str) -> Waited {
		let deadline = now_ms() + self.timeout_ms + LOCK_GRACE_MS;
		loop {
			if let Some(done) = self.stored(result_file, spec, versions) {
				return Waited { stored: Some(done), overdue: None };
			}
			if self.stopped() {
				return Waited::default();
			}
			let Some(held) = read_text(lock_file) else {
				return Waited { stored: self.stored(result_file, spec, versions), overdue: None };
			};
			if holder_gone(&held) {
				return Waited::default();
			}
			if now_ms() >= deadline {
				return Waited { stored: None, overdue: Some(held) };
			}
			tokio::time::sleep(Duration::from_millis(self.poll_ms.max(0) as u64)).await;
		}
	}

	/// Spawns one check's shell with its output in `log_file`, in its own job or group (0641). `Err` when it can't
	/// start.
	fn start(&self, job: &Job, out: &std::fs::File) -> std::io::Result<Box<dyn ChildWrapper>> {
		let mut command = shell_command(&job.spec.command);
		command.current_dir(&job.cwd).env_clear().envs(&job.env).env("NO_COLOR", "1").env("CLICOLOR", "0");
		env::apply_to_tokio(&mut command);
		command.stdin(Stdio::null()).stdout(out.try_clone()?).stderr(out.try_clone()?);
		wrap_async(command, CREATE_NO_WINDOW).spawn()
	}

	/// Waits for the check's shell, ending its tree (the whole job or group, orphans too) past `timeout_ms` or when
	/// `stop` ran; `(exit, timed_out)`. The wait is for the shell, not the job: a check that left a background
	/// process running has still ended.
	async fn finish(&self, mut child: Box<dyn ChildWrapper>) -> std::io::Result<(Option<i32>, bool)> {
		let mut halted = self.halt.subscribe();
		let limit = Duration::from_millis(self.timeout_ms.max(0) as u64);
		let mut timed_out = false;
		let status = tokio::select! {
			status = child.inner_mut().wait() => status,
			() = tokio::time::sleep(limit) => {
				timed_out = true;
				let _ = child.start_kill();
				child.inner_mut().wait().await
			}
			() = async { let _ = halted.wait_for(|halt| *halt).await; } => {
				let _ = child.start_kill();
				child.inner_mut().wait().await
			}
		};
		let status = status?;
		// Ended by `stop`: not the check's own result.
		Ok((exit_code(&status), timed_out || self.stopped()))
	}

	async fn execute(&self, job: &Job, folder: &Path) -> Result<CheckResult> {
		self.runs.fetch_add(1, Ordering::SeqCst);
		if self.stopped() {
			return Err(Error::other(STOPPED));
		}
		// Waiting in the queue is not part of the check's time: the timeout and `ms` start once it is admitted.
		let permit = match &self.admission {
			Some(admission) => {
				let project = project_of(&job.cwd).await;
				Some(admission.admit(&project, &job.spec.command).await.ok_or_else(|| Error::other(STOPPED))?)
			}
			None => None,
		};
		if self.stopped() {
			return Err(Error::other(STOPPED));
		}
		let spec = &job.spec;
		let log_file = folder.join(format!("{}.log", file_name(&spec.name)));
		let mut out = std::fs::File::create(&log_file)?;
		let started = now_ms();
		let mut exit = None;
		let mut timed_out = false;
		match self.start(job, &out) {
			Ok(child) => {
				let sampler = permit.as_ref().and(child.id()).map(Sampler::start);
				let ended = self.finish(child).await;
				let measured = sampler.and_then(Sampler::stop);
				match ended {
					Ok((code, ended)) => {
						(exit, timed_out) = (code, ended);
						// Only a check that ran to its own end says what it costs.
						if let (Some(permit), Some(measured), Some(_), false) = (&permit, measured, code, ended) {
							permit.record(measured.peak_rss_bytes, measured.avg_cores);
						}
					}
					Err(error) => write!(out, "\njocasta: couldn't run `{}`: {error}\n", spec.command)?,
				}
			}
			Err(error) => write!(out, "\njocasta: couldn't run `{}`: {error}\n", spec.command)?,
		}
		drop(permit);
		drop(out);
		let ms = now_ms() - started;
		if timed_out {
			let why = if self.stopped() {
				"Jocasta stopped".to_owned()
			} else {
				format!("{} minutes", (self.timeout_ms as f64 / 60_000.0).round())
			};
			std::fs::OpenOptions::new()
				.append(true)
				.open(&log_file)?
				.write_all(format!("\njocasta: killed ({why})\n").as_bytes())?;
			exit = None;
		}
		let text = String::from_utf8_lossy(&std::fs::read(&log_file)?).into_owned();
		if spec.mise {
			for MiseTask { name, failed } in mise_tasks(&text) {
				if failed && name != spec.name {
					std::fs::write(folder.join(format!("{}.log", file_name(&name))), task_lines(&text, &name))?;
				}
			}
		}
		let gates = gates_of(spec, exit, &text, &log_file, &job.cwd);
		if let Some(exit) = exit {
			// The gates go with the result, so a hit answers without reading the log again.
			let stored =
				StoredOut { command: &spec.command, versions: &job.versions, exit, ms, at: now_ms(), gates: &gates };
			let json = format!("{}\n", serde_json::to_string(&stored)?);
			write_atomically(&folder.join(format!("{}.json", file_name(&spec.name))), json, None)?;
		}
		Ok(CheckResult { spec: spec.clone(), exit, log: log_file, ms, how: How::Ran, gates })
	}
}

/// What waiting on a lock came to.
#[derive(Default)]
struct Waited {
	stored: Option<Stored>,
	overdue: Option<String>,
}

/// `git add -A` and `git write-tree` of `cwd` through the copy `temp` of its index `real`.
async fn tree_of(cwd: &Path, real: &Path, temp: &Path) -> Result<Option<String>> {
	if real.exists() {
		std::fs::copy(real, temp)?;
	}
	let mut with_index = process_env();
	with_index.insert("GIT_INDEX_FILE".to_owned(), temp.to_string_lossy().into_owned());
	let options = || GitOptions { env: Some(with_index.clone()), stdin: None };
	let add = git_async(cwd, &["add", "-A"], GIT_TIMEOUT_MS, options()).await?;
	if add.code != Some(0) {
		let why = if add.stderr.is_empty() {
			format!("exit {}", add.code.map_or_else(|| "null".to_owned(), |c| c.to_string()))
		} else {
			add.stderr
		};
		log(LogLevel::Warn, "checks", &format!("git add -A for the tree key failed in {}: {why}", cwd.display()), None);
		return Ok(None);
	}
	let write = git_async(cwd, &["write-tree"], GIT_TIMEOUT_MS, options()).await?;
	let tree = write.stdout.trim();
	let hex = (40..=64).contains(&tree.len()) && tree.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
	Ok((write.code == Some(0) && hex).then(|| tree.to_owned()))
}

/// The exit code of a finished shell; on POSIX a signal reads as 128 plus its number, as Bun's `proc.exited` gives it.
fn exit_code(status: &std::process::ExitStatus) -> Option<i32> {
	#[cfg(unix)]
	{
		use std::os::unix::process::ExitStatusExt;
		status.code().or_else(|| status.signal().map(|signal| 128 + signal))
	}
	#[cfg(not(unix))]
	{
		status.code()
	}
}

/// This process's lock text: its pid and start time (`process_start`), so a reused pid doesn't pass for it.
fn lock_token() -> String {
	let pid = std::process::id();
	match process_start(pid) {
		Some(start) => format!("{pid} {start}"),
		None => pid.to_string(),
	}
}

/// The holder a lock's text names: `<pid>` or `<pid> <start>`; a pid that isn't a number is 0 (`parseInt`'s NaN).
fn lock_holder(text: &str) -> (i64, Option<f64>) {
	let mut parts = text.split_whitespace();
	let first = parts.next().unwrap_or("");
	let digits = first.strip_prefix(['-', '+']).unwrap_or(first);
	let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
	let pid = match (&digits[..end], first.starts_with('-')) {
		("", _) => 0,
		(number, negative) => number.parse::<i64>().map_or(0, |pid| if negative { -pid } else { pid }),
	};
	let start = parts.next().and_then(|start| start.parse::<f64>().ok()).filter(|start| start.is_finite());
	(pid, start)
}

/// Whether a lock's holder is gone: its process isn't running, or the pid now belongs to a process that started at
/// another time. A lock still being written (no pid yet) has a holder.
fn holder_gone(text: &str) -> bool {
	let (pid, start) = lock_holder(text);
	let Ok(pid) = u32::try_from(pid) else { return false };
	if pid == 0 {
		return false;
	}
	if !pid_alive(pid) {
		return true;
	}
	start.is_some_and(|start| process_start(pid) != Some(start))
}

/// Takes a stale lock over: renames it aside (only one of two takers can) and removes it. When what was renamed isn't
/// the lock judged stale (another taker's new lock in between), it is put back unless a lock is there again.
fn take_over(lock_file: &Path, stale: &str) {
	let short = uuid::Uuid::new_v4().simple().to_string();
	let mut aside = lock_file.as_os_str().to_owned();
	aside.push(format!(".{}-{}.stale", std::process::id(), &short[..8]));
	let aside = PathBuf::from(aside);
	// Gone already, or another taker had it first: the caller looks again.
	if std::fs::rename(lock_file, &aside).is_err() {
		return;
	}
	if read_text(&aside).as_deref() != Some(stale) {
		// A new lock is there already: the link then fails.
		let _ = std::fs::hard_link(&aside, lock_file);
	}
	let _ = std::fs::remove_file(&aside);
}

/// A name as a file name: `/`, `\` and `:` (a mise task's `ci:lint`, `test/unit`) become `-`.
fn file_name(name: &str) -> String {
	name.replace(['/', '\\', ':'], "-")
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
	let since = meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok());
	since.map_or(0, |since| since.as_millis() as i64)
}

/// Removes `folder` when nothing in it was touched for 14 days; its size when removed.
async fn remove_if_old(folder: &Path, now: i64) -> Result<Option<u64>> {
	let Some((newest, bytes)) = folder_age(folder)? else { return Ok(None) };
	if now - newest < RESULT_DAYS * DAY_MS {
		return Ok(None);
	}
	remove_tree_async(folder).await?;
	Ok(Some(bytes))
}

/// The newest mtime of `folder` and its files, and their total size; `None` when it isn't a folder.
fn folder_age(folder: &Path) -> Result<Option<(i64, u64)>> {
	let own = std::fs::metadata(folder)?;
	if !own.is_dir() {
		return Ok(None);
	}
	let mut newest = mtime_ms(&own);
	let mut bytes = 0;
	for entry in std::fs::read_dir(folder)? {
		let meta = std::fs::metadata(entry?.path())?;
		newest = newest.max(mtime_ms(&meta));
		bytes += meta.len();
	}
	Ok(Some((newest, bytes)))
}

/// A byte count as `512 B`, `4.2 KB`, `380 MB`: whole numbers from 10 up, one decimal below.
pub fn size_said(bytes: f64) -> String {
	let units = ["B", "KB", "MB", "GB"];
	let mut value = bytes;
	let mut unit = 0;
	while value >= 1024.0 && unit < units.len() - 1 {
		value /= 1024.0;
		unit += 1;
	}
	let said = if unit == 0 || value >= 10.0 {
		format!("{}", value.round())
	} else {
		format!("{:.1}", (value * 10.0).round() / 10.0)
	};
	format!("{said} {}", units[unit])
}

fn read_text(file: &Path) -> Option<String> {
	std::fs::read(file).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
