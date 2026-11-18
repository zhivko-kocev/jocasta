//! H2 (0482, 0552, 0576): the time from "needs you" to his notification, in a sandbox Jocasta.
//!
//!   jocasta-bench h2-toast --out <dir> [--runs 30] [--minutes 120] [--condition host|herdr] [--start <project>]
//!                          [--dry-run]
//!
//! Jocasta raises its notifications through herdr's `notification.show`, which herdr hands to Windows' notification
//! service through the foreground client and answers `shown` (or `disabled`, `rate_limited`, `no_foreground_client`,
//! `busy`). While it runs, H2 records herdr's `pane.agent_status_changed` pushes for every pane (t1, `Herdr::watch`; a
//! reference figure, since the service's own path is the condition's) and reads the service's notification log,
//! `<data>/notifications.jsonl`, one line per `notification.show` call written when herdr's reply comes back (`at`,
//! `paneId`, `reason`, `title`, `body`, `result`). Each pane's omp session file (herdr reports its path) gives t0: an
//! `ask` tool call's message once complete (`completedAt`, not the request's start, so the model's generation isn't in
//! the figures) or a plan proposal. t3 is the moment herdr answered `shown` for the pane's `blocked` notification: the
//! notification reached Windows; how long Windows then takes to draw the banner isn't measured.
//! Conditions (0576): `host` (the default), the service reading each thread's state from omp's collab host, polled
//! every 1 s (0547); `herdr`, the sandbox service started with `JOCASTA_HOSTS=off`, so it ignores omp's host registry
//! and falls back to herdr's events and its tick. H2 reads the service's switch from `/health` and refuses a mismatch.
//! Trials come from threads he starts (the boss's and the today pane left out), or with `--start <project>` from
//! threads H2 starts as `jocasta thread new` does and reads as `jocasta thread list` does (no Allow, 0544, 0576; each
//! brief makes omp ask one question, so a model runs: VM only): then only those threads' panes are trials (a listed
//! thread's pane is the one in its branch's worktree), and the next thread starts once the last one's notification
//! came, or after 5 minutes without one, which the summary counts as a miss.
//! Writes h2-toast.jsonl (one row per trial), h2-notifications.jsonl (the log's lines of this run) and h2-toast.json
//! (median, p95 and CIs of t1−t0 and t3−t0, and the count of each herdr result).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::evals::args::number_of;
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag, flag_or, parse_bench_args, run_script};
use crate::lib::output::{Conditions, append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::sandbox::{Switch, require_service_switch};
use crate::lib::sessions::{ask_times, plan_ready_times};
use crate::lib::stats::{BootstrapOptions, summarize};
use crate::lib::threads::{ThreadLine, list_threads, start_thread};
use jocasta::frontier::protocol::js_number_text;
use jocasta::frontier::server::boss::{BOSS_WORKSPACE, is_today_pane};
use jocasta::frontier::server::herdr::{AgentState, Herdr, PaneInfo};
use jocasta::frontier::server::launch::OnOff;
use jocasta::shared::clock::now_ms;
use jocasta::shared::errors::Result;
use jocasta::shared::jsonl::json_lines;

use crate::lib::jocasta::open_sandbox;

pub const CONDITIONS: [&str; 2] = ["host", "herdr"];

/// One of herdr's `pane.agent_status_changed` pushes with the time this recorder saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct HerdrEvent {
	pub at: f64,
	pub pane_id: String,
	pub state: AgentState,
}

/// One line of the service's `<data>/notifications.jsonl`: a `notification.show` call and herdr's reply. The line is
/// kept whole, so the rows H2 copies are the service's own, whatever else a line carries.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NotificationEntry {
	row: Map<String, Value>,
}

impl NotificationEntry {
	/// Epoch ms when herdr's reply came back.
	pub fn at(&self) -> f64 {
		self.row.get("at").and_then(Value::as_f64).unwrap_or(f64::NAN)
	}

	/// `None` for a toast that belongs to no pane (`null`, or no field).
	pub fn pane_id(&self) -> Option<&str> {
		self.row.get("paneId").and_then(Value::as_str)
	}

	/// The toast's reason as the service names it: `blocked`, `done`, `failed` or `event`.
	pub fn reason(&self) -> &str {
		self.row.get("reason").and_then(Value::as_str).unwrap_or_default()
	}

	/// herdr's reason (`shown`, `disabled`, `rate_limited`, `no_foreground_client`, `busy`) or `error`.
	pub fn result(&self) -> &str {
		self.row.get("result").and_then(Value::as_str).unwrap_or_default()
	}
}

/// The service's notification log as entries; other lines (partial, malformed) are skipped.
pub fn parse_notifications(text: &str) -> Vec<NotificationEntry> {
	json_lines::<Map<String, Value>>(text)
		.into_iter()
		.filter(|line| {
			line.get("at").is_some_and(Value::is_number)
				&& line.get("result").is_some_and(Value::is_string)
				&& line.get("reason").is_some_and(Value::is_string)
		})
		.map(|row| NotificationEntry { row })
		.collect()
}

pub const NOTIFICATIONS_FILE: &str = "notifications.jsonl";

/// How long a started thread has to bring its notification before the next one starts (BEN-23).
pub const TRIAL_LIMIT_MS: f64 = 5.0 * 60_000.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H2Trial {
	pub pane_id: String,
	/// `ask` or `plan`.
	pub kind: &'static str,
	pub t0: f64,
	/// ms after t0; `None` when not seen.
	pub herdr_ms: Option<f64>,
	/// t3 − t0: herdr answered `shown` for the pane's `blocked` notification; `None` when it didn't.
	pub notification_ms: Option<f64>,
	/// herdr's answer to the pane's first `blocked` notification after t0; `None` when the service raised none.
	pub notification_result: Option<String>,
}

/// Pairs each "needs you" moment (t0) in a pane's session with the first herdr state change to blocked/idle/done and
/// the pane's `blocked` notifications, each at or after t0 and before the pane's next t0: t3 is the first one herdr
/// answered `shown`. `sessions` is each pane's session text in the order the panes were first seen.
pub fn correlate(
	sessions: &[(String, String)],
	events: &[HerdrEvent],
	notifications: &[NotificationEntry],
) -> Vec<H2Trial> {
	let blocked: Vec<&NotificationEntry> = notifications.iter().filter(|entry| entry.reason() == "blocked").collect();
	let mut trials: Vec<H2Trial> = Vec::new();
	for (pane_id, text) in sessions {
		let mut moments: Vec<(f64, &'static str)> = ask_times(text)
			.into_iter()
			.map(|t0| (t0, "ask"))
			.chain(plan_ready_times(text).into_iter().map(|t0| (t0, "plan")))
			.filter(|(t0, _)| t0.is_finite())
			.collect();
		moments.sort_by(|a, b| a.0.total_cmp(&b.0));
		for (i, (t0, kind)) in moments.iter().enumerate() {
			let next = moments.get(i + 1).map_or(f64::INFINITY, |moment| moment.0);
			let within = |at: f64| at >= *t0 && at < next;
			let herdr = events.iter().find(|event| {
				event.pane_id == *pane_id
					&& within(event.at)
					&& event.state != AgentState::Working
					&& event.state != AgentState::Unknown
			});
			let ours: Vec<&&NotificationEntry> = blocked
				.iter()
				.filter(|entry| entry.pane_id() == Some(pane_id.as_str()) && within(entry.at()))
				.collect();
			let shown = ours.iter().find(|entry| entry.result() == "shown");
			trials.push(H2Trial {
				pane_id: pane_id.clone(),
				kind,
				t0: *t0,
				herdr_ms: herdr.map(|event| event.at - t0),
				notification_ms: shown.map(|entry| entry.at() - t0),
				notification_result: ours.first().map(|entry| entry.result().to_owned()),
			});
		}
	}
	trials.sort_by(|a, b| a.t0.total_cmp(&b.t0));
	trials
}

/// The pane of the thread on `branch` (`feature/<slug>`): the one working in a folder `<slug>` inside a `<project>.wt`
/// folder, where the service puts a thread's worktree (`threadPlace`).
pub fn pane_of_branch<'a>(panes: &'a [PaneInfo], branch: &str) -> Option<&'a str> {
	let slug = branch.strip_prefix("feature/").unwrap_or(branch);
	panes
		.iter()
		.find(|pane| {
			let Some(cwd) = &pane.cwd else { return false };
			let cwd = cwd.to_string_lossy();
			let cwd = cwd.trim_end_matches(['\\', '/']);
			let cwd = Path::new(cwd);
			cwd.file_name().is_some_and(|name| name == slug)
				&& cwd
					.parent()
					.and_then(Path::file_name)
					.is_some_and(|parent| parent.to_string_lossy().ends_with(".wt"))
		})
		.map(|pane| pane.pane_id.as_str())
}

/// One of the bench's own threads (with `--start`): its branch, when it started and its pane once found.
#[derive(Debug, Clone, PartialEq)]
pub struct OurThread {
	pub branch: String,
	pub started_at: f64,
	pub pane_id: Option<String>,
}

/// Gives each of the bench's threads without a pane yet its pane once `jocasta thread list` shows a thread on its
/// branch (0576): the pane working in that branch's worktree (the list names no pane).
pub fn find_listed_panes(ours: &mut [OurThread], listed: &[ThreadLine], panes: &[PaneInfo]) {
	for thread in ours.iter_mut() {
		if thread.pane_id.is_none() && listed.iter().any(|line| line.branch == thread.branch) {
			thread.pane_id = pane_of_branch(panes, &thread.branch).map(str::to_owned);
		}
	}
}

/// The brief `--start` gives each thread: one question with omp's ask tool, then nothing else.
const ASK_BRIEF: &str = "Benchmark H2: use the ask tool once to ask me whether the greeting should say hello or hi, then stop and change nothing.";

/// One row of h2-toast.jsonl: the condition, then the trial's own fields.
#[derive(Serialize)]
struct TrialRow<'a> {
	condition: &'a str,
	#[serde(flatten)]
	trial: &'a H2Trial,
}

/// Whether the trials hold a notification result for the pane.
fn answered_in(trials: &[H2Trial], pane_id: Option<&str>) -> bool {
	pane_id.is_some_and(|pane| trials.iter().any(|t| t.pane_id == pane && t.notification_result.is_some()))
}

/// `Map.set`: a known key keeps its place and takes the new value.
fn upsert(paths: &mut Vec<(String, String)>, pane_id: &str, path: String) {
	match paths.iter_mut().find(|(known, _)| known == pane_id) {
		Some(entry) => entry.1 = path,
		None => paths.push((pane_id.to_owned(), path)),
	}
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

/// What a run records while it waits: herdr's state changes, the panes' session files and the service's log.
struct Recorder<'a> {
	herdr: Arc<dyn Herdr>,
	/// `--start`: the project the bench starts its own threads in.
	project: Option<&'a str>,
	events: Arc<Mutex<Vec<HerdrEvent>>>,
	session_paths: Arc<Mutex<Vec<(String, String)>>>,
	/// With `--start`, the bench's own threads by branch, their pane once `jocasta thread list` shows them, and when
	/// each started.
	ours: Vec<OurThread>,
	log_file: PathBuf,
	started_at: f64,
}

impl Recorder<'_> {
	fn read_log(&self) -> Vec<NotificationEntry> {
		match fs::read(&self.log_file) {
			Ok(bytes) => parse_notifications(&String::from_utf8_lossy(&bytes))
				.into_iter()
				.filter(|entry| entry.at() >= self.started_at)
				.collect(),
			Err(_) => Vec::new(),
		}
	}

	fn read_sessions(&self) -> Vec<(String, String)> {
		let paths = self.session_paths.lock().expect("the paths lock").clone();
		paths
			.into_iter()
			.filter_map(|(pane, path)| {
				let bytes = fs::read(&path).ok()?;
				Some((pane, String::from_utf8_lossy(&bytes).into_owned()))
			})
			.collect()
	}

	fn seen_events(&self) -> Vec<HerdrEvent> {
		self.events.lock().expect("the events lock").clone()
	}

	async fn refresh_panes(&mut self) -> Result<()> {
		let snapshot = self.herdr.snapshot().await?;
		let boss = snapshot.workspaces.iter().find(|w| w.label == BOSS_WORKSPACE).map(|w| w.id.as_str());
		if let Some(project) = self.project
			&& self.ours.iter().any(|thread| thread.pane_id.is_none())
		{
			find_listed_panes(&mut self.ours, &list_threads(Some(project)).await?, &snapshot.panes);
		}
		let mut paths = self.session_paths.lock().expect("the paths lock");
		for pane in &snapshot.panes {
			let trial = if self.project.is_some() {
				self.ours.iter().any(|thread| thread.pane_id.as_deref() == Some(pane.pane_id.as_str()))
			} else {
				Some(pane.workspace_id.as_str()) != boss && !is_today_pane(pane)
			};
			if let (true, Some(path)) = (trial, &pane.session_path) {
				upsert(&mut paths, &pane.pane_id, path.to_string_lossy().into_owned());
			}
		}
		Ok(())
	}

	/// Starts the next thread once the last one's notification came, or after `TRIAL_LIMIT_MS` without one.
	async fn start_next(&mut self, runs: u64, trials: &[H2Trial]) -> Result<()> {
		let Some(project) = self.project else { return Ok(()) };
		let due = match self.ours.last() {
			None => true,
			Some(last) => {
				answered_in(trials, last.pane_id.as_deref()) || now_ms() as f64 - last.started_at >= TRIAL_LIMIT_MS
			}
		};
		if (self.ours.len() as u64) < runs && due {
			let at = now_ms() as f64;
			let title = format!("h2 trial {}", self.ours.len() + 1);
			let branch = start_thread(project, &title, ASK_BRIEF).await?;
			self.ours.push(OurThread { branch, started_at: at, pane_id: None });
		}
		Ok(())
	}

	/// Waits for `runs` answered trials or the deadline, refreshing the panes every 2 s.
	async fn collect(&mut self, runs: u64, deadline: f64) -> Result<()> {
		let mut trials: Vec<H2Trial> = Vec::new();
		while (now_ms() as f64) < deadline {
			self.start_next(runs, &trials).await?;
			tokio::time::sleep(Duration::from_millis(2_000)).await;
			self.refresh_panes().await?;
			trials = correlate(&self.read_sessions(), &self.seen_events(), &self.read_log());
			if trials.iter().filter(|trial| trial.notification_result.is_some()).count() as u64 >= runs {
				break;
			}
		}
		Ok(())
	}
}

/// The `--dry-run` plan.
fn print_h2_plan(args: &BenchArgs, condition: &str, minutes: f64, project: Option<&str>) {
	let how = if condition == "host" {
		"host (omp's collab host, polled every 1 s)"
	} else {
		"herdr (service started with JOCASTA_HOSTS=off: herdr's events and tick)"
	};
	let trials = match project {
		Some(project) => format!(
			"start each trial as `jocasta thread new {project}` does (no Allow; model call: VM only), the next after the last one's notification or {} min: \"{ASK_BRIEF}\"",
			js_number_text(TRIAL_LIMIT_MS / 60_000.0)
		),
		None => "trials come from threads started by hand (the boss's and the today pane left out)".to_owned(),
	};
	print_plan(
		"H2 toast latency",
		&[
			format!("condition {how}; collect {} trials or stop after {} min", args.runs, js_number_text(minutes)),
			"refuse a run whose condition doesn't match the sandbox service's /health".to_owned(),
			"record herdr pane.agent_status_changed pushes (t1, a reference figure)".to_owned(),
			format!(
				"read the service's <data>/{NOTIFICATIONS_FILE}: t3 = herdr answered `shown` to the pane's blocked notification.show"
			),
			"t0 from each pane's omp session file: an ask tool call's completed message or a plan proposal".to_owned(),
			trials,
			"report median, p95 and bootstrap 95% CIs of t1−t0 and t3−t0, and how often herdr answered each result"
				.to_owned(),
		],
		args.out.as_deref(),
	);
}

/// h2-toast.json: the figures of the trials.
fn h2_summary(
	condition: &str,
	started: Conditions,
	trials: &[H2Trial],
	started_threads: Option<&[OurThread]>,
) -> Result<Value> {
	let seen_of = |ms: fn(&H2Trial) -> Option<f64>| {
		let values: Vec<f64> = trials.iter().filter_map(ms).collect();
		summarize(&values, BootstrapOptions::default())
	};
	let mut results: Map<String, Value> = Map::new();
	for trial in trials {
		let result = trial.notification_result.as_deref().unwrap_or("none");
		let count = results.get(result).and_then(Value::as_u64).unwrap_or(0) + 1;
		results.insert(result.to_owned(), json!(count));
	}
	let mut summary = Map::new();
	summary.insert("benchmark".to_owned(), json!("H2"));
	summary.insert("condition".to_owned(), json!(condition));
	summary.insert(
		"endPoint".to_owned(),
		json!("t3 = herdr answered `shown` to the service's notification.show for the pane's blocked toast (0552)"),
	);
	summary.insert("conditions".to_owned(), serde_json::to_value(started.finished())?);
	summary.insert("trials".to_owned(), json!(trials.len()));
	summary.insert("herdrMs".to_owned(), serde_json::to_value(seen_of(|trial| trial.herdr_ms))?);
	summary.insert("notificationMs".to_owned(), serde_json::to_value(seen_of(|trial| trial.notification_ms))?);
	summary.insert("notificationResults".to_owned(), Value::Object(results));
	if let Some(ours) = started_threads {
		summary.insert("started".to_owned(), json!(ours.len()));
		let missed: Vec<&str> = ours
			.iter()
			.filter(|thread| !answered_in(trials, thread.pane_id.as_deref()))
			.map(|thread| thread.branch.as_str())
			.collect();
		summary.insert("missed".to_owned(), json!(missed));
	}
	Ok(Value::Object(summary))
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec { default_runs: 30, valued: &["--minutes", "--condition", "--start"], ..ArgSpec::default() };
	let args = parse_bench_args(&argv, &spec)?;
	let minutes = number_of(flag_or(&args, "minutes", "120"));
	let condition = flag_or(&args, "condition", "host");
	if !CONDITIONS.contains(&condition) {
		return Err(BenchUsageError::err("--condition is host or herdr"));
	}
	if !minutes.is_finite() || minutes <= 0.0 {
		return Err(BenchUsageError::err("--minutes needs a number above 0"));
	}
	let project = flag(&args, "start");
	if args.dry_run {
		print_h2_plan(&args, condition, minutes, project);
		return Ok(());
	}
	let out = args.out.clone().expect("a real run has --out");
	let sandbox = open_sandbox(None)?;
	claim_rows(&out, &["h2-toast", "h2-notifications"])?;
	require_service_switch(Switch::Hosts, if condition == "herdr" { OnOff::Off } else { OnOff::On }).await?;
	let started = conditions(&["omp", "herdr", "jocasta"]);
	let herdr = sandbox.herdr().await?;
	// The service restarts herdr, not this recorder.
	herdr.keep_down();
	let mut recorder = Recorder {
		herdr: herdr.clone(),
		project,
		events: Arc::default(),
		session_paths: Arc::default(),
		ours: Vec::new(),
		log_file: Path::new(&sandbox.sandbox.data_dir).join(NOTIFICATIONS_FILE),
		started_at: now_ms() as f64,
	};
	recorder.refresh_panes().await?;
	let stop_watch = herdr.watch(
		{
			let paths = recorder.session_paths.clone();
			Box::new(move || paths.lock().expect("the paths lock").iter().map(|(pane, _)| pane.clone()).collect())
		},
		{
			let events = recorder.events.clone();
			Box::new(move |event| {
				events.lock().expect("the events lock").push(HerdrEvent {
					at: now_ms() as f64,
					pane_id: event.pane_id,
					state: event.state,
				});
			})
		},
	);
	let deadline = now_ms() as f64 + minutes * 60_000.0;
	let collected = recorder.collect(args.runs, deadline).await;
	stop_watch();
	herdr.close();
	collected?;
	let notifications = recorder.read_log();
	for entry in &notifications {
		append_row(&out, "h2-notifications", entry)?;
	}
	let mut trials = correlate(&recorder.read_sessions(), &recorder.seen_events(), &notifications);
	trials.truncate(usize::try_from(args.runs).unwrap_or(usize::MAX));
	for trial in &trials {
		append_row(&out, "h2-toast", &TrialRow { condition, trial })?;
	}
	let summary = h2_summary(condition, started, &trials, project.map(|_| recorder.ours.as_slice()))?;
	let file = write_json(&out, "h2-toast", &summary)?;
	println!("{} trials; wrote {}", trials.len(), file.display());
	Ok(())
}

#[cfg(test)]
#[path = "h2_toast_tests.rs"]
mod tests;
