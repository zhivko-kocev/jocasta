//! The task benchmark of 0482, as 0543 amends it: `jocasta-bench evals bench --tasks <tasks.jsonl> --cache <dir>
//! --out <runDir> [--study main|followup] [--arms a0,a1,a2,a3,a4,a6,a7] [--reps 3] [--set main] [--only id,id]
//! [--pilot] [--workers 2] [--seed 482] [--timeout 90] [--profile <omp profile>] [--work <dir>] [--validate]
//! [--dry-run]`. Trials (tasks × arms × reps; `--study followup` is the follow-up study's 10 tasks × 2 reps × A6, A1 and
//! six comparison arms) run in a seeded random order through the eval runner's `run_trial`, each on its own blocking
//! thread (`--workers` at once, never two of one task), the quota pacer awaited before each, their roots in `--work`
//! (default the OS temp folder). Results go to `<runDir>/results.jsonl` (with each trial's ledger totals), the ledger's
//! rows to `ledger.jsonl`, each attempt's priced spend to `spend.jsonl`, the harnesses' own records to `sessions/`,
//! infra errors to `errors.jsonl` (retried, at most twice more), setup and grading output to `logs/`, the run's
//! conditions to `conditions.json`; a rerun with the same `--out` and the same conditions resumes, and
//! `<runDir>/run.lock` keeps a second bench out while one runs. `--validate` checks each task without a model: its
//! hidden tests fail on the base commit and pass three times with the gold patch (logs in `validation/`). Real trials
//! bill model usage: they need `JOCASTA_EVALS_REAL=1` and never run in CI (0169). Exits 1 when a trial or task is left
//! failing or an arm stopped on a new harness version, 2 on a bad  or an omp folder that would give the omp arms
//! MCP servers (0587), 3 after three sign-in failures in a row, 4 when A7's spend limit stopped its trials.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::future::{BoxFuture, join_all};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::Notify;

use super::grading::{GRADING_LIMIT_MS, SETUP_LIMIT_MS};
use super::pacer::Pause;
use super::report::{js_text, to_fixed};
use super::runner::{EvalCase, ResultRow, TrialOptions, run_trial};
use super::tasks::{
	BenchRow, BenchTask, Infra, InfraOutcome, InfraRow, LedgerSummary, MAX_ATTEMPTS, Trial, classify_infra,
	pending_trials, plan_trials, prepare_task, read_resume_state, scan_leaks, task_case, trial_key,
};
use super::variants::{Harness, Variant};
use super::watch::wait_while_paused;
use crate::lib::args::BenchUsageError;
use crate::lib::ledger::{bench_pricing, ledger_of_dir};
use crate::lib::output::{append_row, conditions, iso_now, json_text};
use crate::lib::prices::{PriceTable, load_price_table};
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::git::git as git_read;
use jocasta::shared::jsonl::read_jsonl_file;
use jocasta::shared::ledger::{LedgerHarness, TrialLedger};
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};
use jocasta::shared::proctree::{pid_alive, process_started_at};

/// One arm as the run uses it: its variant, the command that starts its harness, and that harness's version.
#[derive(Clone)]
pub struct Arm {
	pub name: String,
	pub variant: Variant,
	pub launcher: Vec<String>,
	pub harness_version: String,
}

/// What a worker runs: one trial through `run_trial`.
pub struct TrialJob {
	pub eval_case: EvalCase,
	pub rep: u64,
	pub harness_version: String,
	pub options: TrialOptions,
}

/// A trial's result as the worker returns it: the row, or the error it threw, and the harness's stderr.
pub struct TrialOutcome {
	pub row: Option<ResultRow>,
	pub error: Option<String>,
	pub stderr: String,
}

/// Runs one trial; the default runs it on its own blocking thread.
pub type Execute = Arc<dyn Fn(TrialJob) -> BoxFuture<'static, TrialOutcome> + Send + Sync>;

/// A line to the run's log.
pub type LogFn = Arc<dyn Fn(&str) + Send + Sync>;

/// Reads an arm's harness version again.
pub type VersionOf = Arc<dyn Fn(&Arm) -> Result<String> + Send + Sync>;

pub struct BenchOptions {
	pub tasks: Vec<BenchTask>,
	pub cache_dir: PathBuf,
	pub out_dir: PathBuf,
	pub arms: Vec<Arm>,
	pub reps: u64,
	pub seed: u32,
	pub workers: usize,
	pub timeout_ms: Option<u64>,
	/// The setup-before-the-harness limit; [`SETUP_LIMIT_MS`] when unset (tests shorten it). Held for the run.
	pub setup_timeout_ms: Option<u64>,
	pub profile: Option<String>,
	/// The bench herdr session for plan-review trials' terminals (`--pty-session`, 0567).
	pub pty_session: Option<String>,
	/// Recorded in `conditions.json`.
	pub pilot: bool,
	pub pace: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
	/// Whether the pacer's last reading had a full account: a rate limit then is an infra error (0543).
	pub account_full: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
	/// The ledger's price table; the committed one by default.
	pub prices: Option<&'static PriceTable>,
	/// A7's spend limit in dollars at the price table (0537); [`A7_SPEND_LIMIT_USD`] by default.
	pub a7_spend_limit_usd: Option<f64>,
	/// Where trial roots go (`--work`); the OS temp folder when unset. Recorded in `conditions.json`.
	pub work_dir: Option<PathBuf>,
	/// Runs one trial; the default runs it on a blocking thread.
	pub execute: Option<Execute>,
	/// Reads an arm's harness version again after each of its trials (0574); a changed version stops that arm's
	/// remaining trials. No re-check when unset.
	pub version_of: Option<VersionOf>,
	/// The host's conditions for `conditions.json` (an object); the default asks each tool for its version.
	pub describe_host: Option<Box<dyn Fn() -> Map<String, Value>>>,
	pub log: Option<LogFn>,
	/// Pauses the pacer took, for `conditions.json`.
	pub pauses: Option<Arc<Mutex<Vec<Pause>>>>,
}

impl BenchOptions {
	/// The options a run has when only its plan is given.
	pub fn new(tasks: Vec<BenchTask>, cache_dir: PathBuf, out_dir: PathBuf, arms: Vec<Arm>, reps: u64) -> Self {
		Self {
			tasks,
			cache_dir,
			out_dir,
			arms,
			reps,
			seed: 482,
			workers: 2,
			timeout_ms: None,
			setup_timeout_ms: None,
			profile: None,
			pty_session: None,
			pilot: false,
			pace: None,
			account_full: None,
			prices: None,
			a7_spend_limit_usd: None,
			work_dir: None,
			execute: None,
			version_of: None,
			describe_host: None,
			log: None,
			pauses: None,
		}
	}
}

/// The tools whose versions `conditions.json` records (0482, 0537).
const VERSION_TOOLS: [&str; 8] = ["omp", "claude", "opencode", "git", "bun", "node", "python", "go"];

/// A7's spend limit (0537): once the priced spend of A7's attempts (`spend.jsonl`, the pinned price table) reaches it,
/// no further A7 trial starts. The key's own limit at Anthropic is the backstop for a trial already running.
pub const A7_SPEND_LIMIT_USD: f64 = 150.0;

/// One `spend.jsonl` row: an attempt's priced cost, whether it counted or was an infra error.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpendRow {
	task: String,
	arm: String,
	rep: u64,
	attempt: u64,
	harness: String,
	cost: f64,
	#[serde(default)]
	unpriced_models: Vec<String>,
}

/// Long-path support as the trials see it: git's effective `core.longpaths` and, on Windows, `LongPathsEnabled`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LongPaths {
	git_core_longpaths: Option<String>,
	/// `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem` `LongPathsEnabled`; null off Windows or when unset.
	windows_long_paths_enabled: Option<u64>,
}

/// Reads [`LongPaths`] without changing anything.
fn read_long_paths() -> LongPaths {
	let git_core_longpaths = git_read(&std::env::temp_dir(), &["config", "--get", "core.longpaths"]).ok().flatten();
	let mut windows_long_paths_enabled = None;
	if cfg!(windows) {
		let options = RunOptions { timeout_ms: Some(30_000), ..RunOptions::default() };
		let query = ["reg", "query", "HKLM\\SYSTEM\\CurrentControlSet\\Control\\FileSystem", "/v", "LongPathsEnabled"];
		if let Ok(reg) = run_program_sync(&query, options) {
			let pattern = Regex::new(r"(?i)LongPathsEnabled\s+REG_DWORD\s+0x([0-9a-f]+)").expect("a fixed pattern");
			windows_long_paths_enabled =
				pattern.captures(&reg.stdout).and_then(|found| u64::from_str_radix(&found[1], 16).ok());
		}
	}
	LongPaths { git_core_longpaths, windows_long_paths_enabled }
}

/// The host, the tools' versions (Go's from `go version`, not `--version`) and long-path support.
fn host_conditions() -> Map<String, Value> {
	let tools: Vec<&str> = VERSION_TOOLS.iter().copied().filter(|tool| *tool != "go").collect();
	let mut host = conditions(&tools);
	let version = on_path("go", None).and_then(|go| {
		let options = RunOptions { timeout_ms: Some(30_000), ..RunOptions::default() };
		run_program_sync(&[go.to_string_lossy().into_owned(), "version".to_owned()], options).ok()
	});
	host.versions.insert(
		"go".to_owned(),
		match version {
			Some(run) if run.code == Some(0) => Value::String(run.stdout.trim().to_owned()),
			_ => Value::Null,
		},
	);
	let mut object = match serde_json::to_value(&host) {
		Ok(Value::Object(object)) => object,
		_ => Map::new(),
	};
	object.insert("longPaths".to_owned(), serde_json::to_value(read_long_paths()).unwrap_or(Value::Null));
	object
}

/// Runs one trial on its own blocking thread, so a setup's or a check's blocking spawn never stalls another trial's
/// harness.
fn execute_on_thread(job: TrialJob) -> BoxFuture<'static, TrialOutcome> {
	Box::pin(async move {
		let stderr: Arc<Mutex<Vec<String>>> = Arc::default();
		let mut options = job.options;
		let sink = stderr.clone();
		options.on_harness_run = Some(Arc::new(move |run| {
			sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(run.stderr.clone());
		}));
		let handle = tokio::runtime::Handle::current();
		let finished = tokio::task::spawn_blocking(move || {
			handle.block_on(run_trial(&job.eval_case, job.rep, &job.harness_version, options))
		})
		.await;
		let joined = stderr.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).join("\n");
		match finished {
			Ok(Ok(row)) => TrialOutcome { row: Some(row), error: None, stderr: joined },
			Ok(Err(error)) => TrialOutcome { row: None, error: Some(error.to_string()), stderr: joined },
			Err(error) => TrialOutcome { row: None, error: Some(error.to_string()), stderr: joined },
		}
	})
}

/// What a run holds for its whole length (0574): a resume with other values is refused.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeldConditions {
	timeout_minutes: Option<f64>,
	profile: Option<String>,
	/// The setup-before-the-harness limit (0574, amended); absent in runs from before it.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	setup_limit_minutes: Option<f64>,
	/// Each arm's `--version` at the run's start.
	versions: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RunRecord {
	seed: u32,
	arms: Vec<String>,
	reps: u64,
	tasks: Vec<String>,
	pilot: bool,
	order: Vec<String>,
	/// Absent in runs from before 0574.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	held: Option<HeldConditions>,
	sessions: Vec<Map<String, Value>>,
}

/// Infra errors in a row that were sign-in failures; three stop the run (0482).
const AUTH_STOP: u32 = 3;

/// `<out>/run.lock`: the bench that runs in `--out` now (0574).
pub const RUN_LOCK: &str = "run.lock";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunLock {
	pid: u32,
	started_at: String,
	/// The holder's process start (Windows), so a reused pid isn't taken for the holder.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	process_start: Option<i64>,
}

/// A time as `YYYY-MM-DD HH:MM`, local.
fn local_minute(iso: &str) -> String {
	match chrono::DateTime::parse_from_rfc3339(iso) {
		Ok(at) => at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string(),
		Err(_) => "NaN-NaN-NaN NaN:NaN".to_owned(),
	}
}

/// Takes `<outDir>/run.lock` for this process (0574, as the service's lock, 0565): refused while a live bench holds it;
/// a lock whose process is gone (a crashed run) is taken over. Returns its release.
fn take_run_lock(out_dir: &Path) -> Result<impl FnOnce() + use<>> {
	let file = out_dir.join(RUN_LOCK);
	let held: Option<RunLock> = fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str(&text).ok());
	if let Some(held) = held {
		let start = process_started_at(held.pid);
		let same = match (held.process_start, start) {
			(Some(before), Some(now)) => (now - before).abs() < 1_000,
			_ => true,
		};
		if pid_alive(held.pid) && same {
			return Err(BenchUsageError::err(format!(
				"{} is in use by a bench (pid {}, since {}); stop it or use another --out",
				out_dir.display(),
				held.pid,
				local_minute(&held.started_at)
			)));
		}
		remove_tree(&file)?;
	}
	let pid = std::process::id();
	let mine = RunLock { pid, started_at: iso_now(), process_start: process_started_at(pid) };
	let text = format!("{}\n", serde_json::to_string(&mine)?);
	let created = fs::OpenOptions::new().write(true).create_new(true).open(&file);
	match created {
		Ok(mut handle) => std::io::Write::write_all(&mut handle, text.as_bytes())?,
		Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
			return Err(BenchUsageError::err(format!(
				"{} is in use by another bench starting now; stop it or use another --out",
				out_dir.display()
			)));
		}
		Err(error) => return Err(error.into()),
	}
	Ok(move || {
		let ours = fs::read_to_string(&file)
			.ok()
			.and_then(|text| serde_json::from_str::<RunLock>(&text).ok())
			.is_some_and(|lock| lock.pid == pid);
		if ours {
			let _ = remove_tree(&file);
		}
	})
}

fn version_text(value: Option<&Value>) -> Option<&str> {
	value.and_then(Value::as_str)
}

/// A resume whose held conditions differ from the run's: the refusal's text, or `None` when they match.
fn held_differs(out_dir: &Path, before: &HeldConditions, now: &HeldConditions) -> Option<String> {
	let minutes = |value: Option<f64>| value.map_or_else(|| "none".to_owned(), js_text);
	let profile = |held: &HeldConditions| match &held.profile {
		None => "no profile".to_owned(),
		Some(name) => format!("profile {name}"),
	};
	let mut changed: Vec<String> = Vec::new();
	if now.timeout_minutes != before.timeout_minutes {
		changed.push(format!("--timeout {}", minutes(now.timeout_minutes)));
	}
	if let Some(was) = before.setup_limit_minutes
		&& now.setup_limit_minutes != Some(was)
	{
		changed.push(format!("setup limit {} min", minutes(now.setup_limit_minutes)));
	}
	if now.profile != before.profile {
		changed.push(profile(now));
	}
	for (arm, version) in &now.versions {
		let version = version.as_str().unwrap_or_default();
		if version_text(before.versions.get(arm)) != Some(version) && !changed.iter().any(|each| each == version) {
			changed.push(version.to_owned());
		}
	}
	if changed.is_empty() {
		return None;
	}
	let mut was = vec![format!("--timeout {}", minutes(before.timeout_minutes))];
	if let Some(limit) = before.setup_limit_minutes {
		was.push(format!("setup limit {} min", js_text(limit)));
	}
	was.push(profile(before));
	let mut seen: Vec<&str> = Vec::new();
	for version in before.versions.values().filter_map(Value::as_str) {
		if !seen.contains(&version) {
			seen.push(version);
			was.push(version.to_owned());
		}
	}
	Some(format!(
		"{} ran with {}; this resume has {}; resume with the same flags or use a new --out",
		out_dir.display(),
		was.join(", "),
		changed.join(", ")
	))
}

/// Every file's text under `dir`: a trial's saved sessions, subagents' included, for the leak scan (EVL-10).
fn text_under(dir: &Path) -> String {
	fn files(dir: &Path, found: &mut Vec<PathBuf>) {
		let Ok(entries) = fs::read_dir(dir) else { return };
		let mut paths: Vec<PathBuf> = entries.filter_map(|entry| entry.ok().map(|entry| entry.path())).collect();
		paths.sort();
		for path in paths {
			if path.is_dir() {
				files(&path, found);
			} else {
				found.push(path);
			}
		}
	}
	let mut found = Vec::new();
	files(dir, &mut found);
	found
		.iter()
		.filter_map(|path| fs::read(path).ok())
		.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
		.collect::<Vec<_>>()
		.join("\n")
}

fn ledger_harness(harness: Harness) -> LedgerHarness {
	match harness {
		Harness::Omp => LedgerHarness::Omp,
		Harness::Claude => LedgerHarness::Claude,
		Harness::Opencode => LedgerHarness::Opencode,
	}
}

fn minutes_of(ms: u64) -> f64 {
	ms as f64 / 60_000.0
}

/// Runs the benchmark's trials into `out_dir`, resuming what is there, while it holds `<out_dir>/run.lock`. A free
/// worker takes the next trial in the seeded order whose task isn't running already, so two trials never share a
/// task's caches or tool folders at once. Each attempt's saved sessions are priced into the ledger (0538); A7's trials
/// stop once its priced spend reaches the limit (0537); an arm whose harness reports a new version stops (0574).
/// Returns the exit code: 0 when every trial has a row, 1 when some used up their attempts on infra errors or an arm
/// stopped on a new version, 3 when three sign-in failures in a row stopped the run, 4 when A7's spend limit (or an A7
/// model the price table lacks) stopped its trials.
pub async fn run_bench(options: BenchOptions) -> Result<u8> {
	fs::create_dir_all(options.out_dir.join("transcripts"))?;
	let release = take_run_lock(&options.out_dir)?;
	let result = run_locked(options).await;
	release();
	result
}

/// Which list a trial goes to instead of running.
#[derive(Clone, Copy)]
enum Skip {
	A7,
	Version,
}

/// What the workers share; one thread polls them all, so each borrow ends before an await.
struct Shared {
	queue: Vec<Trial>,
	state: super::tasks::ResumeState,
	exhausted: Vec<Trial>,
	auth_in_row: u32,
	stopped_by: Option<String>,
	a7_spent: f64,
	a7_stopped_why: Option<String>,
	a7_skipped: Vec<Trial>,
	/// Arms stopped because their harness reported a new version (0574), with why, in the order they stopped.
	version_stopped: Vec<(String, String)>,
	version_skipped: Vec<Trial>,
	/// Tasks with a trial running now.
	running: HashSet<String>,
}

/// One attempt at a trial: what its records need to know about it.
struct Attempt<'a> {
	trial: &'a Trial,
	arm: &'a Arm,
	task: &'a BenchTask,
	eval_case: &'a EvalCase,
	key: &'a str,
	/// 1 for the first try, 2–3 after infra errors.
	attempt: u64,
	sessions_dir: PathBuf,
}
struct Context<'a> {
	options: &'a BenchOptions,
	log: &'a dyn Fn(&str),
	execute: Execute,
	tasks: HashMap<String, (BenchTask, EvalCase)>,
	arms: HashMap<String, &'a Arm>,
	prices: &'static PriceTable,
	spend_limit: f64,
	wake: Notify,
	shared: RefCell<Shared>,
}

impl Context<'_> {
	fn is_a7(&self, trial: &Trial) -> bool {
		self.arms.get(&trial.arm).is_some_and(|arm| arm.variant.harness == Harness::Opencode)
	}

	/// Where a trial goes instead of running: an arm stopped by A7's limit or by a new harness version.
	fn skipped_to(&self, trial: &Trial) -> Option<Skip> {
		let shared = self.shared.borrow();
		if shared.a7_stopped_why.is_some() && self.is_a7(trial) {
			Some(Skip::A7)
		} else if shared.version_stopped.iter().any(|(arm, _)| *arm == trial.arm) {
			Some(Skip::Version)
		} else {
			None
		}
	}

	fn check_version(&self, arm: &Arm) {
		let Some(version_of) = &self.options.version_of else { return };
		if self.shared.borrow().version_stopped.iter().any(|(name, _)| *name == arm.name) {
			return;
		}
		let now = match version_of(arm) {
			Ok(version) => version,
			Err(error) => format!("nothing ({})", error.to_string().split('\n').next().unwrap_or("")),
		};
		if now != arm.harness_version {
			self.shared.borrow_mut().version_stopped.push((
				arm.name.clone(),
				format!(
					"{} --version now says {now}; the run started on {} (0482 pins it)",
					arm.variant.harness, arm.harness_version
				),
			));
		}
	}

	fn job(&self, at: &Attempt, transcript_name: &str) -> TrialJob {
		let options = self.options;
		TrialJob {
			eval_case: at.eval_case.clone(),
			rep: at.trial.rep,
			harness_version: at.arm.harness_version.clone(),
			options: TrialOptions {
				out_dir: options.out_dir.clone(),
				launcher: at.arm.launcher.clone(),
				variant: Some(at.arm.variant.clone()),
				profile: options.profile.clone(),
				timeout_ms: options.timeout_ms,
				setup_timeout_ms: options.setup_timeout_ms,
				transcript_name: Some(transcript_name.to_owned()),
				work_dir: options.work_dir.clone(),
				pty_session: options.pty_session.clone(),
				log_dir: Some(options.out_dir.join("logs").join(transcript_name)),
				sessions_dir: Some(at.sessions_dir.clone()),
				..TrialOptions::default()
			},
		}
	}

	/// Appends the attempt's priced spend; A7's attempts count toward its limit (0537).
	fn record_spend(&self, at: &Attempt, ledger: Option<&TrialLedger>) -> Result<()> {
		let spend = SpendRow {
			task: at.trial.task.clone(),
			arm: at.trial.arm.clone(),
			rep: at.trial.rep,
			attempt: at.attempt,
			harness: at.arm.variant.harness.to_string(),
			cost: ledger.map_or(0.0, |ledger| ledger.totals.cost),
			unpriced_models: ledger.map(|ledger| ledger.totals.unpriced_models.clone()).unwrap_or_default(),
		};
		append_row(&self.options.out_dir, "spend", &spend)?;
		if self.is_a7(at.trial) {
			let mut shared = self.shared.borrow_mut();
			shared.a7_spent += spend.cost;
			if !spend.unpriced_models.is_empty() {
				if shared.a7_stopped_why.is_none() {
					shared.a7_stopped_why = Some(format!(
						"the price table lacks {}, so A7's spend can't be counted",
						spend.unpriced_models.join(", ")
					));
				}
			} else if shared.a7_spent >= self.spend_limit && shared.a7_stopped_why.is_none() {
				shared.a7_stopped_why = Some(format!("A7's priced spend is ${}", to_fixed(shared.a7_spent, 2)));
			}
		}
		Ok(())
	}

	/// An infra error: its row, the retry or the stop it leads to.
	fn record_infra(&self, at: &Attempt, infra: &Infra) -> Result<()> {
		let row = InfraRow {
			task: at.trial.task.clone(),
			arm: at.trial.arm.clone(),
			rep: at.trial.rep,
			attempt: at.attempt,
			trial_index: at.trial.index,
			harness: at.arm.variant.harness.to_string(),
			kind: infra.kind,
			message: infra.message.clone(),
			at: iso_now(),
		};
		append_row(&self.options.out_dir, "errors", &row)?;
		let mut shared = self.shared.borrow_mut();
		shared.state.failures.insert(at.key.to_owned(), at.attempt);
		let kind = kind_text(&row);
		(self.log)(&format!(
			"infra {} (attempt {}, {kind}): {}",
			at.key,
			at.attempt,
			infra.message.split('\n').next().unwrap_or("")
		));
		shared.auth_in_row = if kind == "auth" { shared.auth_in_row + 1 } else { 0 };
		if shared.auth_in_row >= AUTH_STOP {
			if shared.stopped_by.is_none() {
				shared.stopped_by = Some(at.arm.variant.harness.to_string());
			}
		} else if at.attempt < MAX_ATTEMPTS {
			shared.queue.push(at.trial.clone());
		} else {
			shared.exhausted.push(at.trial.clone());
		}
		Ok(())
	}

	/// A trial that counts: its `results.jsonl` row and its ledger rows.
	fn record_row(&self, at: &Attempt, row: ResultRow, transcript: &str, ledger: Option<&TrialLedger>) -> Result<()> {
		let out_dir = &self.options.out_dir;
		let leak = scan_leaks(
			at.task,
			// Subagents' own records too (EVL-10): a search an explorer ran reaches the transcript only as its report.
			&format!("{transcript}\n{}", text_under(&at.sessions_dir)),
			Some(&at.eval_case.prompt),
		);
		let row = BenchRow {
			row,
			task: at.trial.task.clone(),
			arm: at.trial.arm.clone(),
			language: at.task.language,
			attempt: at.attempt,
			trial_index: at.trial.index,
			leak,
			ledger: ledger
				.map(|ledger| LedgerSummary { totals: ledger.totals.clone(), sessions: ledger.sessions.len() }),
		};
		append_row(out_dir, "results", &row)?;
		for ledger_row in ledger.iter().flat_map(|ledger| &ledger.rows) {
			let mut object = Map::new();
			object.insert("task".to_owned(), json!(at.trial.task));
			object.insert("arm".to_owned(), json!(at.trial.arm));
			object.insert("rep".to_owned(), json!(at.trial.rep));
			if let Value::Object(fields) = serde_json::to_value(ledger_row)? {
				object.extend(fields);
			}
			append_row(out_dir, "ledger", &object)?;
		}
		self.shared.borrow_mut().state.done.insert(at.key.to_owned());
		(self.log)(&format!(
			"{} {}{}",
			if row.row.hidden_passed == Some(true) { "pass" } else { "FAIL" },
			at.key,
			if row.leak.is_empty() { String::new() } else { format!(" (leak: {})", row.leak.join(", ")) }
		));
		Ok(())
	}

	async fn run_one(&self, trial: &Trial) -> Result<()> {
		let options = self.options;
		let (task, eval_case) = self.tasks.get(&trial.task).expect("every planned task was prepared");
		let arm = *self.arms.get(&trial.arm).expect("every planned arm exists");
		let key = trial_key(trial);
		let transcript_name = format!("{}-{}-{}", trial.task, trial.arm, trial.rep);
		let at = Attempt {
			trial,
			arm,
			task,
			eval_case,
			key: &key,
			attempt: self.shared.borrow().state.failures.get(&key).copied().unwrap_or(0) + 1,
			sessions_dir: options.out_dir.join("sessions").join(&transcript_name),
		};
		// An attempt that throws before it saves its sessions never prices the previous attempt's again (EVL-16).
		remove_tree(&at.sessions_dir)?;
		let outcome = (self.execute)(self.job(&at, &transcript_name)).await;
		self.check_version(arm);
		let ledger: Option<TrialLedger> = if at.sessions_dir.exists() {
			Some(ledger_of_dir(&at.sessions_dir, ledger_harness(arm.variant.harness), &bench_pricing(self.prices))?)
		} else {
			None
		};
		self.record_spend(&at, ledger.as_ref())?;
		let transcript_file = options.out_dir.join("transcripts").join(format!("{transcript_name}.jsonl"));
		let transcript = if outcome.row.is_some() && transcript_file.exists() {
			String::from_utf8_lossy(&fs::read(&transcript_file)?).into_owned()
		} else {
			String::new()
		};
		let infra = classify_infra(&InfraOutcome {
			error: outcome.error.clone(),
			row: outcome.row.clone(),
			stderr: outcome.stderr.clone(),
			transcript: transcript.clone(),
			prompt: eval_case.prompt.clone(),
			account_full: options.account_full.as_ref().is_some_and(|full| full()),
		});
		if let Some(infra) = infra {
			return self.record_infra(&at, &infra);
		}
		self.shared.borrow_mut().auth_in_row = 0;
		self.record_row(
			&at,
			outcome.row.expect("an outcome without an infra error has a row"),
			&transcript,
			ledger.as_ref(),
		)
	}

	/// One worker: takes the next trial in the seeded order whose task isn't running, until none is left.
	async fn worker(&self) -> Result<()> {
		loop {
			let woken = self.wake.notified();
			tokio::pin!(woken);
			woken.as_mut().enable();
			let next = {
				let mut shared = self.shared.borrow_mut();
				if shared.stopped_by.is_some() || shared.queue.is_empty() {
					return Ok(());
				}
				for i in (0..shared.queue.len()).rev() {
					let trial = shared.queue[i].clone();
					drop(shared);
					let to = self.skipped_to(&trial);
					shared = self.shared.borrow_mut();
					if let Some(to) = to {
						let moved = shared.queue.remove(i);
						match to {
							Skip::A7 => shared.a7_skipped.insert(0, moved),
							Skip::Version => shared.version_skipped.insert(0, moved),
						}
					}
				}
				if shared.queue.is_empty() {
					return Ok(());
				}
				let at = shared.queue.iter().position(|trial| !shared.running.contains(&trial.task));
				match at {
					None => None,
					Some(at) => {
						let trial = shared.queue.remove(at);
						shared.running.insert(trial.task.clone());
						Some(trial)
					}
				}
			};
			let Some(trial) = next else {
				woken.await;
				continue;
			};
			let outcome: Result<()> = async {
				wait_while_paused(&self.options.out_dir).await;
				if let Some(pace) = &self.options.pace {
					pace().await;
				}
				let to = self.skipped_to(&trial);
				let stopped = self.shared.borrow().stopped_by.is_some();
				if !stopped && to.is_none() {
					self.run_one(&trial).await?;
				} else if !stopped {
					match to {
						Some(Skip::A7) => self.shared.borrow_mut().a7_skipped.push(trial.clone()),
						Some(Skip::Version) => self.shared.borrow_mut().version_skipped.push(trial.clone()),
						None => {}
					}
				}
				Ok(())
			}
			.await;
			self.shared.borrow_mut().running.remove(&trial.task);
			self.wake.notify_waiters();
			outcome?;
		}
	}
}

fn kind_text(row: &InfraRow) -> String {
	serde_json::to_value(row.kind).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default()
}

/// The run's record: its plan and what it holds, with this session's conditions added; a resume keeps what the folder's
/// own record had, and is refused when the plan or the held conditions differ.
fn begin_record(options: &BenchOptions, order: &[Trial]) -> Result<RunRecord> {
	let held = HeldConditions {
		timeout_minutes: options.timeout_ms.map(minutes_of),
		profile: options.profile.clone(),
		setup_limit_minutes: Some(minutes_of(options.setup_timeout_ms.unwrap_or(SETUP_LIMIT_MS))),
		versions: options
			.arms
			.iter()
			.map(|arm| (arm.name.clone(), Value::String(arm.harness_version.clone())))
			.collect(),
	};
	let mut record = RunRecord {
		seed: options.seed,
		arms: options.arms.iter().map(|arm| arm.name.clone()).collect(),
		reps: options.reps,
		tasks: options.tasks.iter().map(|task| task.instance_id.clone()).collect(),
		pilot: options.pilot,
		order: order.iter().map(trial_key).collect(),
		held: Some(held.clone()),
		sessions: Vec::new(),
	};
	let record_file = options.out_dir.join("conditions.json");
	if record_file.exists() {
		let before: RunRecord = serde_json::from_str(&fs::read_to_string(&record_file)?)?;
		if before.order != record.order {
			return Err(BenchUsageError::err(format!(
				"{} holds a run with another plan (seed, arms, reps or tasks); resume it with the same flags or use a new --out",
				options.out_dir.display()
			)));
		}
		// A run from before 0574 recorded no held conditions: this session's become them.
		if let Some(before_held) = &before.held
			&& let Some(refused) = held_differs(&options.out_dir, before_held, &held)
		{
			return Err(BenchUsageError::err(refused));
		}
		record.held = Some(before.held.unwrap_or_else(|| held.clone()));
		record.sessions = before.sessions;
	}
	let held_now = record.held.clone().unwrap_or(held);
	let mut session = match &options.describe_host {
		Some(describe) => describe(),
		None => host_conditions(),
	};
	session.insert("workers".to_owned(), json!(options.workers));
	session.insert("timeoutMinutes".to_owned(), json!(held_now.timeout_minutes));
	session.insert("gradingLimitMinutes".to_owned(), json!(minutes_of(GRADING_LIMIT_MS)));
	session.insert(
		"setupLimitMinutes".to_owned(),
		json!(held_now.setup_limit_minutes.unwrap_or(minutes_of(SETUP_LIMIT_MS))),
	);
	session.insert("profile".to_owned(), json!(held_now.profile));
	session.insert(
		"workRoot".to_owned(),
		json!(options.work_dir.clone().unwrap_or_else(std::env::temp_dir).to_string_lossy()),
	);
	record.sessions.push(session);
	Ok(record)
}

fn save_record(out_dir: &Path, record: &RunRecord) -> Result<()> {
	let text = json_text(&serde_json::to_value(record)?, true);
	fs::write(out_dir.join("conditions.json"), format!("{text}\n"))?;
	Ok(())
}

/// A7's priced spend so far from `spend.jsonl`, and why it stays stopped on a resume, if it does.
fn a7_resume(out_dir: &Path, spend_limit: f64) -> Result<(f64, Option<String>)> {
	let a7_spend: Vec<SpendRow> = read_jsonl_file::<SpendRow>(&out_dir.join("spend.jsonl"))?
		.into_iter()
		.filter(|row| row.harness == "opencode")
		.collect();
	let a7_spent: f64 = a7_spend.iter().map(|row| row.cost).sum();
	let mut unpriced: Vec<&str> = Vec::new();
	for model in a7_spend.iter().flat_map(|row| &row.unpriced_models) {
		if !unpriced.contains(&model.as_str()) {
			unpriced.push(model);
		}
	}
	// A resume after an unpriced-model stop stays stopped: its spend still can't be counted (EVL-16).
	let why = if !unpriced.is_empty() {
		Some(format!("the price table lacks {}, so A7's spend can't be counted", unpriced.join(", ")))
	} else if a7_spent >= spend_limit {
		Some(format!("A7's priced spend is ${}", to_fixed(a7_spent, 2)))
	} else {
		None
	};
	Ok((a7_spent, why))
}

/// What the run says when it ends, and its exit code.
fn conclude(shared: &Shared, spend_limit: f64, log: &dyn Fn(&str)) -> u8 {
	// Exhausted trials are listed whatever else stopped the run (EVL-16).
	if !shared.exhausted.is_empty() {
		log(&format!(
			"{} trials used up {MAX_ATTEMPTS} attempts on infra errors: {}",
			shared.exhausted.len(),
			shared.exhausted.iter().map(trial_key).collect::<Vec<_>>().join(", ")
		));
	}
	for (arm, why) in &shared.version_stopped {
		let left = shared.version_skipped.iter().filter(|trial| trial.arm == *arm).count();
		log(&format!("stopped {arm}: {why}; {left} {arm} trials not run"));
	}
	if let Some(stopped_by) = &shared.stopped_by {
		log(&format!(
			"stopped: {AUTH_STOP} trials in a row could not sign in to {stopped_by}. Sign in to {stopped_by} again (a token refresh on another machine may have revoked the copied login), then rerun the same command to resume."
		));
		return 3;
	}
	if let Some(why) = &shared.a7_stopped_why
		&& !shared.a7_skipped.is_empty()
	{
		log(&format!(
			"A7 stopped ({why}; limit ${}, 0537): {} A7 trials not run: {}",
			js_text(spend_limit),
			shared.a7_skipped.len(),
			shared.a7_skipped.iter().map(trial_key).collect::<Vec<_>>().join(", ")
		));
		return 4;
	}
	u8::from(!shared.exhausted.is_empty() || !shared.version_skipped.is_empty())
}

async fn run_locked(options: BenchOptions) -> Result<u8> {
	let log_fn: LogFn = options.log.clone().unwrap_or_else(|| Arc::new(|line| println!("{line}")));
	let log = |line: &str| log_fn(line);
	let execute: Execute = options.execute.clone().unwrap_or_else(|| Arc::new(execute_on_thread));
	let arm_names: Vec<String> = options.arms.iter().map(|arm| arm.name.clone()).collect();
	let order = plan_trials(&options.tasks, &arm_names, options.reps, options.seed);
	let mut record = begin_record(&options, &order)?;
	save_record(&options.out_dir, &record)?;
	let mut tasks: HashMap<String, (BenchTask, EvalCase)> = HashMap::new();
	for task in &options.tasks {
		log(&format!("preparing {}", task.instance_id));
		let prepared = prepare_task(task, &options.cache_dir)?;
		tasks.insert(task.instance_id.clone(), (task.clone(), task_case(task, &prepared)));
	}
	let arms: HashMap<String, &Arm> = options.arms.iter().map(|arm| (arm.name.clone(), arm)).collect();
	let state = read_resume_state(&options.out_dir)?;
	let pending = pending_trials(&order, &state, MAX_ATTEMPTS);
	log(&format!(
		"{} trials: {} done, {} to run",
		order.len(),
		order.len() - pending.pending.len() - pending.exhausted.len(),
		pending.pending.len()
	));
	let prices = match options.prices {
		Some(prices) => prices,
		None => load_price_table()?,
	};
	let spend_limit = options.a7_spend_limit_usd.unwrap_or(A7_SPEND_LIMIT_USD);
	let (a7_spent, a7_stopped_why) = a7_resume(&options.out_dir, spend_limit)?;
	let context = Context {
		options: &options,
		log: &log,
		execute,
		tasks,
		arms,
		prices,
		spend_limit,
		wake: Notify::new(),
		shared: RefCell::new(Shared {
			queue: pending.pending.clone(),
			state,
			exhausted: pending.exhausted,
			auth_in_row: 0,
			stopped_by: None,
			a7_spent,
			a7_stopped_why,
			a7_skipped: Vec::new(),
			version_stopped: Vec::new(),
			version_skipped: Vec::new(),
			running: HashSet::new(),
		}),
	};
	let workers = (0..options.workers.max(1)).map(|_| context.worker());
	for result in join_all(workers).await {
		result?;
	}
	let shared = context.shared.into_inner();
	if let Some(session) = record.sessions.last_mut() {
		session.insert("finishedAt".to_owned(), json!(iso_now()));
		let pauses = options.pauses.as_ref().map(|pauses| pauses.lock().unwrap_or_else(|p| p.into_inner()).clone());
		session.insert("pauses".to_owned(), serde_json::to_value(pauses.unwrap_or_default())?);
		session.insert("a7SpendUsd".to_owned(), json!((shared.a7_spent * 100.0).round() / 100.0));
	}
	save_record(&options.out_dir, &record)?;
	Ok(conclude(&shared, spend_limit, &log))
}
