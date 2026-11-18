//! H3 (0482): crash and restart recovery, in a sandbox Jocasta with thread panes open.
//!
//!   jocasta-bench h3-recovery --out <dir> [--runs 20] [--faults herdr,service,pane] [--timeout 120]
//!                             [--restart "jocasta"] [--dry-run]
//!
//! Each run snapshots the sandbox herdr session's panes and every pane folder's git state, finds its target, injects one
//! fault and polls until the work is back, then compares. Recovery time runs from the moment the kill returned (the
//! lookups come before it):
//!   herdr    kills the session's herdr server (the parent of a pane's process, `pane.process_info`); recovered when
//!            every workspace that had an omp pane before has an omp pane again (0522: the service relaunches the
//!            panes under new ids). Each row also gives the ms until herdr first listed exactly the old pane ids
//!            (`oldPaneIdsMs`, null when no poll saw them).
//!   service  kills the service that `<data>/jocasta-service.json` names and runs `--restart` (default `jocasta`);
//!            recovered when a new service answers and every workspace that had an omp pane before has one again
//!            (0528; the new service relaunches the boss pane under a new id). Each restart's client ends after its
//!            run, root first and never the new service's tree, so one client is attached at a time. Rows of runs
//!            before 0576 named this fault `companion` (h3-recovery.json's `renamed` says so).
//!   pane     kills the process tree herdr started for one omp pane (omp itself), the one in the folder it killed
//!            longest ago (one at work first among equals), waiting when it must so one kill per folder per 10 minutes
//!            (0528): each kill then meets 0521's first ending, not its restart wait. Recovered when omp runs again in
//!            that workspace under another pane id (0521's restart gives the pane a new id) and every workspace that
//!            had an omp pane before has one again (0522).
//! Jocasta closing itself (0530): a run that didn't recover, after which no service answers (one still closing is
//!            waited for) and herdr lists no panes, counts not recovered with `stoppedItself`. `--restart` then starts
//!            Jocasta again, its client staying attached in place of the terminal's, and the next run starts once a
//!            service answers and every workspace that had an omp pane before has one again (`restartMs`); not within 5
//!            minutes, H3 ends there.
//! Lost work: each pane folder's `git status --porcelain` and `git diff` hash before and after, for the folders in a git
//!            work tree (the boss's home has no git state to lose); a git that fails there fails the run. Each pane's
//!            session file still parses (every non-empty line JSON, a `session` entry anywhere, 0523). Logging off and
//!            on (fault 4) can't be scripted; it is run by hand with the same checks (README).
//! Writes h3-recovery.jsonl (one row per run) and h3-recovery.json (rates with CIs and recovery-time summaries).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::evals::args::number_of;
use crate::evals::report::js_text;
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag_or, list_flag, parse_bench_args, run_script};
use crate::lib::git::git;
use crate::lib::jocasta::{SandboxJocasta, open_sandbox};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::proctree::{ProcessInfo, snapshot_processes, tree_of};
use crate::lib::stats::{BootstrapOptions, rate, summarize};
use crate::lib::timing::{WaitOptions, wait_for};
use jocasta::frontier::server::herdr::{AgentState, PaneInfo};
use jocasta::frontier::server::launch::ServiceState;
use jocasta::shared::clock::now_ms;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::git::git as git_read;
use jocasta::shared::jsonl::json_lines;
use jocasta::shared::pathkey::path_key;
use jocasta::shared::proctree::{kill_process, kill_tree, pid_alive};
use jocasta::shared::service_file::{ServiceFile, read_service_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fault {
	Herdr,
	Service,
	Pane,
}

pub const FAULTS: [Fault; 3] = [Fault::Herdr, Fault::Service, Fault::Pane];

impl Fault {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Herdr => "herdr",
			Self::Service => "service",
			Self::Pane => "pane",
		}
	}

	fn named(name: &str) -> Option<Self> {
		FAULTS.into_iter().find(|fault| fault.as_str() == name)
	}
}

/// The faults renamed since earlier runs' rows (0576, 0584): new name → old name.
pub const RENAMED_FAULTS: [(Fault, &str); 1] = [(Fault::Service, "companion")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderState {
	pub status: String,
	pub diff_hash: String,
}

/// A folder and its git state, in the order the panes listed the folders.
pub type Folders = Vec<(PathBuf, FolderState)>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H3Row {
	pub fault: Fault,
	pub run: usize,
	pub recovered: bool,
	pub recovery_ms: Option<f64>,
	/// Herdr fault only (0522): ms until herdr first listed exactly the old pane ids; null when never seen.
	pub old_pane_ids_ms: Option<f64>,
	pub same_panes: bool,
	pub folders_intact: bool,
	pub sessions_parse: bool,
	/// Jocasta had closed itself (0530): the run didn't recover, no service answered and herdr listed no panes.
	pub stopped_itself: bool,
	/// ms from `--restart` until a service answered and omp was back everywhere (0530); null when not started again.
	pub restart_ms: Option<f64>,
	pub target: Option<String>,
}

/// Milliseconds on a clock that only runs forward (`performance.now()`).
fn clock_ms() -> f64 {
	static START: LazyLock<Instant> = LazyLock::new(Instant::now);
	START.elapsed().as_secs_f64() * 1000.0
}

/// How long one of H3's pane listings may take before the poll counts herdr as not answering.
const LIST_MS: i64 = 3_000;

async fn panes(jocasta: &SandboxJocasta) -> Option<Vec<PaneInfo>> {
	let herdr = jocasta.herdr().await.ok()?;
	herdr.panes().await.ok()
}

/// A folder's git state; a git that fails errors, so a broken check never compares two empty outputs.
fn folder_state(cwd: &Path) -> Result<FolderState> {
	let status = git(cwd, &["status", "--porcelain"], None)?;
	let diff = git(cwd, &["diff", "HEAD"], None)?;
	let diff_hash = Sha256::digest(diff.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
	Ok(FolderState { status, diff_hash })
}

/// Same pane ids, as sets.
pub fn same_pane_ids(before: &[PaneInfo], after: &[PaneInfo]) -> bool {
	let a: HashSet<&str> = before.iter().map(|pane| pane.pane_id.as_str()).collect();
	let b: HashSet<&str> = after.iter().map(|pane| pane.pane_id.as_str()).collect();
	a.len() == b.len() && a.iter().all(|id| b.contains(id))
}

fn runs_omp(pane: &PaneInfo) -> bool {
	pane.agent.as_deref() == Some("omp")
}

/// Every workspace that had an omp pane before has an omp pane now, whatever its id (0522).
pub fn omp_back(before: &[PaneInfo], after: &[PaneInfo]) -> bool {
	let back: HashSet<&str> =
		after.iter().filter(|pane| runs_omp(pane)).map(|pane| pane.workspace_id.as_str()).collect();
	before.iter().filter(|pane| runs_omp(pane)).all(|pane| back.contains(pane.workspace_id.as_str()))
}

/// The pane fault's own pane is back (0521, 0522): omp runs again in the killed pane's workspace under another pane id
/// (0521's restart gives it a new one), so the dying omp, still listed under the old id, never counts.
pub fn pane_back(killed: &PaneInfo, now: &[PaneInfo]) -> bool {
	now.iter().any(|pane| pane.workspace_id == killed.workspace_id && runs_omp(pane) && pane.pane_id != killed.pane_id)
}

/// How long a Jocasta that closed itself has to come back before H3 ends (0530).
const RESTART_BOUND_MS: u64 = 5 * 60_000;

/// How long a service that is closing Jocasta gets to finish before H3 judges whether Jocasta closed itself (0530).
const CLOSING_BOUND_MS: u64 = 60_000;

/// Whether the service `jocasta-service.json` names answers as itself (`up`, or `busy` while its process lives); one
/// that is closing Jocasta answers `/health` until its last step, so it is waited for (`CLOSING_BOUND_MS`) and then
/// doesn't count.
async fn service_answers(jocasta: &SandboxJocasta) -> bool {
	let mut state = jocasta.health(None).await;
	if state == ServiceState::Closing {
		let latest = RefCell::new(state);
		wait_for(
			|| async {
				let now = jocasta.health(None).await;
				*latest.borrow_mut() = now;
				now != ServiceState::Closing
			},
			WaitOptions { timeout_ms: CLOSING_BOUND_MS, interval_ms: 500 },
		)
		.await;
		state = *latest.borrow();
	}
	matches!(state, ServiceState::Up | ServiceState::Busy)
}

/// Jocasta closed itself during a run (0530): the run didn't recover, no service answers, and herdr lists no panes (or
/// doesn't answer). A service killed by the service fault leaves herdr's panes, so that run isn't one.
pub fn closed_itself(recovered: bool, service_up: bool, listed: Option<&[PaneInfo]>) -> bool {
	!recovered && !service_up && listed.is_none_or(<[PaneInfo]>::is_empty)
}

/// Starts `command_line` through the shell, detached from this process, its output dropped; resolves its pid.
pub(crate) fn start_detached(command_line: &str) -> Result<u32> {
	#[cfg(windows)]
	let mut command = {
		use std::os::windows::process::CommandExt;
		const DETACHED_PROCESS: u32 = 0x0000_0008;
		const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
		let shell = jocasta::shared::env::var("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned());
		let mut command = std::process::Command::new(shell);
		command
			.raw_arg(format!("/d /s /c \"{command_line}\""))
			.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
		command
	};
	#[cfg(not(windows))]
	let mut command = {
		use std::os::unix::process::CommandExt;
		let mut command = std::process::Command::new("/bin/sh");
		command.arg("-c").arg(command_line).process_group(0);
		command
	};
	jocasta::shared::env::apply_to(&mut command);
	let child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
	Ok(child.id())
}

/// Starts a Jocasta that closed itself again (0530) with `restart`, whose client stays attached in place of the
/// terminal's; the ms until a service answers and every workspace that had an omp pane before has one again, or `None`
/// when that takes longer than `RESTART_BOUND_MS`.
async fn start_again(restart: &str, before: &[PaneInfo], jocasta: &SandboxJocasta) -> Result<Option<f64>> {
	let t0 = clock_ms();
	start_detached(restart)?;
	let back = wait_for(
		|| async {
			if jocasta.health(None).await != ServiceState::Up {
				return false;
			}
			panes(jocasta).await.is_some_and(|now| omp_back(before, &now))
		},
		WaitOptions { timeout_ms: RESTART_BOUND_MS, interval_ms: 1_000 },
	)
	.await;
	Ok(back.map(|_| (clock_ms() - t0).round()))
}

/// Between two kills in one folder (0528): 0521 counts endings in a row per folder and starts counting again 10 minutes
/// after the last ending that started omp again; 30 s more for the service's refresh.
pub const FOLDER_SPACING_MS: f64 = 10.0 * 60_000.0 + 30_000.0;

/// A folder as 0521 keys it (`pathKey`): case and slashes don't matter on Windows.
pub fn folder_key(pane: &PaneInfo) -> String {
	match &pane.cwd {
		Some(cwd) => path_key(cwd),
		None => path_key(Path::new(&pane.pane_id)),
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
	pub pane: PaneInfo,
	pub wait_ms: f64,
}

/// The pane fault's pane (0528): the omp pane whose folder's last run ended longest ago (never first), at work first
/// among equals; `wait_ms` until that folder is `FOLDER_SPACING_MS` past its last run.
pub fn pick_pane(list: &[PaneInfo], last_ended: &HashMap<String, f64>, now: f64) -> Option<Picked> {
	let ready =
		|pane: &PaneInfo| last_ended.get(&folder_key(pane)).map_or(f64::NEG_INFINITY, |last| last + FOLDER_SPACING_MS);
	let mut best: Option<&PaneInfo> = None;
	for pane in list {
		if !runs_omp(pane) {
			continue;
		}
		let better = match best {
			None => true,
			Some(best) => {
				ready(pane) < ready(best)
					|| (ready(pane) == ready(best)
						&& pane.agent_state == AgentState::Working
						&& best.agent_state != AgentState::Working)
			}
		};
		if better {
			best = Some(pane);
		}
	}
	best.map(|pane| Picked { pane: pane.clone(), wait_ms: (ready(pane) - now).max(0.0) })
}

/// Ends a restart's process tree root first, so `jocasta` is gone before it can see its herdr client exit and ask to
/// close (0434); never a process in `keep` (the new service's tree). Returns the pids it ended.
pub fn end_restart(
	root_pid: u32,
	keep: &HashSet<u32>,
	processes: &[ProcessInfo],
	mut kill: impl FnMut(u32) -> Result<()>,
) -> Vec<u32> {
	let mut ended = Vec::new();
	for info in tree_of(root_pid, processes) {
		if keep.contains(&info.pid) {
			continue;
		}
		// A process gone already is skipped.
		if kill(info.pid).is_ok() {
			ended.push(info.pid);
		}
	}
	ended
}

/// The herdr fault's poll (0522): recovered when omp is back; on the way it notes the ms from `since` until a listing
/// first holds exactly the old pane ids.
pub struct HerdrWatch<'a, C: Fn() -> f64> {
	before: &'a [PaneInfo],
	since: f64,
	clock: C,
	old_ids_ms: Option<f64>,
}

pub fn herdr_recovery<C: Fn() -> f64>(before: &[PaneInfo], since: f64, clock: C) -> HerdrWatch<'_, C> {
	HerdrWatch { before, since, clock, old_ids_ms: None }
}

impl<C: Fn() -> f64> HerdrWatch<'_, C> {
	pub fn check(&mut self, now: Option<&[PaneInfo]>) -> bool {
		let Some(now) = now else { return false };
		if self.old_ids_ms.is_none() && same_pane_ids(self.before, now) {
			self.old_ids_ms = Some(((self.clock)() - self.since).round());
		}
		omp_back(self.before, now)
	}

	pub fn old_pane_ids_ms(&self) -> Option<f64> {
		self.old_ids_ms
	}
}

/// Every folder's git state unchanged.
pub fn folders_intact(before: &[(PathBuf, FolderState)], after: &[(PathBuf, FolderState)]) -> bool {
	before.iter().all(|(cwd, state)| after.iter().find(|(path, _)| path == cwd).is_some_and(|(_, now)| now == state))
}

/// JavaScript's `String.prototype.trim` whitespace: a line with nothing else is empty.
fn blank(line: &str) -> bool {
	line.chars().all(|c| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
}

/// A session file that still parses (0523): every non-empty line is JSON and a `session` entry is in it, anywhere.
pub fn session_parses(text: &str) -> bool {
	let lines = text.split('\n').filter(|line| !blank(line.strip_suffix('\r').unwrap_or(line))).count();
	let entries: Vec<Value> = json_lines(text);
	entries.len() == lines && entries.iter().any(|entry| entry.get("type").and_then(Value::as_str) == Some("session"))
}

/// What the fault left to watch.
enum Recovery<'a> {
	Herdr(RefCell<HerdrWatch<'a, fn() -> f64>>),
	Service { old_pid: u32 },
	Pane(PaneInfo),
}

async fn recovered(recovery: &Recovery<'_>, jocasta: &SandboxJocasta, before: &[PaneInfo]) -> bool {
	match recovery {
		Recovery::Herdr(watch) => {
			let now = panes(jocasta).await;
			watch.borrow_mut().check(now.as_deref())
		}
		Recovery::Service { old_pid } => {
			let Some(next) = read_service_file() else { return false };
			if next.pid == *old_pid {
				return false;
			}
			let now = panes(jocasta).await;
			jocasta.health(Some(&next)).await == ServiceState::Up && now.is_some_and(|now| omp_back(before, &now))
		}
		Recovery::Pane(killed) => {
			let now = panes(jocasta).await;
			now.is_some_and(|now| pane_back(killed, &now) && omp_back(before, &now))
		}
	}
}

/// `jocasta-bench h3-recovery …`.
pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

/// What `--faults`, `--timeout` and `--restart` ask for.
struct Settings {
	faults: Vec<Fault>,
	names: Vec<String>,
	timeout_ms: f64,
	restart: String,
}

fn settings(args: &BenchArgs) -> Result<Settings> {
	let names = list_flag(args, "faults", &["herdr", "service", "pane"]);
	let mut faults = Vec::new();
	for name in &names {
		faults.push(
			Fault::named(name)
				.ok_or_else(|| BenchUsageError::err(format!("unknown fault {name} (herdr, service, pane)")))?,
		);
	}
	let timeout_ms = number_of(flag_or(args, "timeout", "120")) * 1000.0;
	if !timeout_ms.is_finite() || timeout_ms <= 0.0 {
		return Err(BenchUsageError::err("--timeout needs seconds above 0"));
	}
	Ok(Settings { faults, names, timeout_ms, restart: flag_or(args, "restart", "jocasta").to_owned() })
}

fn print_dry_run(args: &BenchArgs, settings: &Settings) {
	let restart = &settings.restart;
	print_plan(
		"H3 crash recovery",
		&[
			format!("{} runs of each fault: {} (log off/on is by hand)", args.runs, settings.names.join(", ")),
			"before each: herdr's panes, each pane folder's git status and diff hash, each session file".to_owned(),
			"herdr: kill the session's herdr server; recovered when every workspace with an omp pane has one again; timed from the kill".to_owned(),
			"herdr: also the time until herdr first lists exactly the old pane ids (a second figure)".to_owned(),
			format!("service: kill the service in jocasta-service.json, run `{restart}`; recovered when a new one answers and every workspace with an omp pane has one again; then the restart's client ends"),
			"pane: kill the process tree of the omp pane in the folder killed longest ago (one at work first), once per folder in 10.5 min; recovered when omp runs again in its workspace under another pane id and every workspace with an omp pane has one again".to_owned(),
			format!("Jocasta closing itself (no service, after one closing is waited for, and no panes after a run that didn't recover): not recovered, `{restart}` starts it again and its client stays; the next run once omp is back everywhere, at most 5 min, else H3 ends"),
			"lost work: git status and diff hash unchanged; each session file all JSON with a session entry".to_owned(),
			format!(
				"poll every 250 ms for up to {} s; report recovery rate, time to recovery and lost work",
				js_text(settings.timeout_ms / 1000.0)
			),
		],
		args.out.as_deref(),
	);
}

/// What one run needs from the whole run.
struct Context<'a> {
	jocasta: SandboxJocasta,
	herdr_session: String,
	out: &'a Path,
	settings: &'a Settings,
}

/// The pane fault's wait (0528): until its folder is `FOLDER_SPACING_MS` past its last run.
async fn wait_for_folder(ctx: &Context<'_>, run: usize, last_ended: &HashMap<String, f64>) {
	let listed = panes(&ctx.jocasta).await;
	let next = listed.and_then(|listed| pick_pane(&listed, last_ended, now_ms() as f64));
	if let Some(next) = next.filter(|next| next.wait_ms > 0.0) {
		println!(
			"pane run {run}: waiting {} s, one kill per folder per 10 minutes (0528)",
			js_text((next.wait_ms / 1000.0).ceil())
		);
		tokio::time::sleep(Duration::from_secs_f64(next.wait_ms / 1000.0)).await;
	}
}

/// The git state of every pane folder in a git work tree.
fn pane_folders(before: &[PaneInfo]) -> Result<Folders> {
	let mut cwds: Vec<&Path> = Vec::new();
	for cwd in before.iter().filter_map(|pane| pane.cwd.as_deref()) {
		if !cwds.contains(&cwd)
			&& cwd.exists()
			&& git_read(cwd, &["rev-parse", "--is-inside-work-tree"])?.as_deref() == Some("true")
		{
			cwds.push(cwd);
		}
	}
	cwds.into_iter().map(|cwd| Ok((cwd.to_path_buf(), folder_state(cwd)?))).collect()
}

/// Every pane's session file that exists still parses.
fn sessions_parse(before: &[PaneInfo]) -> Result<bool> {
	for path in before.iter().filter_map(|pane| pane.session_path.as_deref()).filter(|path| path.exists()) {
		if !session_parses(&String::from_utf8_lossy(&std::fs::read(path)?)) {
			return Ok(false);
		}
	}
	Ok(true)
}

/// A fault injected: what it hit and what to watch for.
struct Injected<'a> {
	target: String,
	killed_folder: Option<String>,
	restart_pid: Option<u32>,
	/// When the kill returned.
	t0: f64,
	recovery: Recovery<'a>,
}

/// Each fault's lookups come first; `t0` is when its kill returned (0522's time to recovery).
async fn inject<'a>(
	fault: Fault,
	ctx: &Context<'_>,
	before: &'a [PaneInfo],
	last_ended: &HashMap<String, f64>,
) -> Result<Injected<'a>> {
	let jocasta = &ctx.jocasta;
	match fault {
		Fault::Herdr => {
			let roots = jocasta.processes(&snapshot_processes()?, None).await?;
			let server = roots
				.herdr_server
				.ok_or_else(|| Error::other("couldn't find herdr's server process (parent of a pane's process)"))?;
			kill_tree(server).await;
			let t0 = clock_ms();
			let watch = herdr_recovery(before, t0, clock_ms as fn() -> f64);
			Ok(Injected {
				target: server.to_string(),
				killed_folder: None,
				restart_pid: None,
				t0,
				recovery: Recovery::Herdr(RefCell::new(watch)),
			})
		}
		Fault::Service => {
			let running: ServiceFile = read_service_file()
				.ok_or_else(|| Error::other("no service: <data>/jocasta-service.json is missing"))?;
			// The service alone: its panes live in herdr's process tree, not under it.
			kill_process(running.pid)?;
			let t0 = clock_ms();
			wait_for(|| async { !pid_alive(running.pid) }, WaitOptions::within(10_000)).await;
			Ok(Injected {
				target: running.pid.to_string(),
				killed_folder: None,
				restart_pid: Some(start_detached(&ctx.settings.restart)?),
				t0,
				recovery: Recovery::Service { old_pid: running.pid },
			})
		}
		Fault::Pane => {
			let picked =
				pick_pane(before, last_ended, now_ms() as f64).ok_or_else(|| Error::other("no omp pane to kill"))?;
			let pane = picked.pane;
			let herdr = jocasta.herdr().await?;
			let root = herdr.pane_root(&pane.pane_id, Some(10_000)).await.ok().flatten();
			let root = root.ok_or_else(|| Error::other(format!("herdr gave no process for pane {}", pane.pane_id)))?;
			// herdr started omp itself in the pane (no shell to keep): its whole tree goes, whatever omp's binary is.
			kill_tree(root).await;
			Ok(Injected {
				target: pane.pane_id.clone(),
				killed_folder: Some(folder_key(&pane)),
				restart_pid: None,
				t0: clock_ms(),
				recovery: Recovery::Pane(pane),
			})
		}
	}
}

/// One run's row, with what the run does after it.
struct Outcome {
	row: H3Row,
	killed_folder: Option<String>,
	restart_pid: Option<u32>,
}

/// One fault, injected and watched until the work is back or the time is up; its row is written and printed.
async fn one_run(ctx: &Context<'_>, fault: Fault, run: usize, last_ended: &HashMap<String, f64>) -> Result<Outcome> {
	let jocasta = &ctx.jocasta;
	if fault == Fault::Pane {
		wait_for_folder(ctx, run, last_ended).await;
	}
	let before = panes(jocasta).await.unwrap_or_default();
	if before.is_empty() {
		return Err(Error::other(format!("herdr session {} lists no panes", ctx.herdr_session)));
	}
	let folders = pane_folders(&before)?;
	let injected = inject(fault, ctx, &before, last_ended).await?;
	let recovery = &injected.recovery;
	let timeout_ms = ctx.settings.timeout_ms as u64;
	let waited = wait_for(|| recovered(recovery, jocasta, &before), WaitOptions { timeout_ms, interval_ms: 250 }).await;
	let recovery_ms = waited.map(|_| (clock_ms() - injected.t0).round());
	let listed = panes(jocasta).await;
	let same_panes = same_pane_ids(&before, listed.as_deref().unwrap_or_default());
	let mut now_folders: Folders = Vec::new();
	for (cwd, _) in &folders {
		now_folders.push((cwd.clone(), folder_state(cwd)?));
	}
	let intact = folders_intact(&folders, &now_folders);
	let parse = sessions_parse(&before)?;
	// 0530: Jocasta closed itself (0518's heal can take herdr started again for him quitting): started again.
	let stopped_itself = closed_itself(waited.is_some(), service_answers(jocasta).await, listed.as_deref());
	if stopped_itself {
		println!("{} run {run}: Jocasta closed itself; starting it again (0530)", fault.as_str());
	}
	let restart_ms = if stopped_itself { start_again(&ctx.settings.restart, &before, jocasta).await? } else { None };
	let old_pane_ids_ms = match recovery {
		Recovery::Herdr(watch) => watch.borrow().old_pane_ids_ms(),
		_ => None,
	};
	let row = H3Row {
		fault,
		run,
		recovered: waited.is_some(),
		recovery_ms,
		old_pane_ids_ms,
		same_panes,
		folders_intact: intact,
		sessions_parse: parse,
		stopped_itself,
		restart_ms,
		target: Some(injected.target),
	};
	append_row(ctx.out, "h3-recovery", &row)?;
	println!("{}", run_line(&row));
	Ok(Outcome { row, killed_folder: injected.killed_folder, restart_pid: injected.restart_pid })
}

/// The line a run prints.
fn run_line(row: &H3Row) -> String {
	let outcome = match row.recovery_ms {
		Some(ms) if row.recovered => format!("back in {} ms", js_text(ms)),
		_ => "not recovered".to_owned(),
	};
	let again = match (row.stopped_itself, row.restart_ms) {
		(false, _) => String::new(),
		(true, None) => "; Jocasta not back after its restart".to_owned(),
		(true, Some(ms)) => format!("; Jocasta back in {} ms after its restart", js_text(ms)),
	};
	format!("{} run {}: {outcome}{again}", row.fault.as_str(), row.run)
}

/// One client at a time (0528): the restart's tree goes, root first; the new service's stays.
fn end_restart_client(run: usize, restart_pid: u32) -> Result<()> {
	let processes = snapshot_processes()?;
	let keep: HashSet<u32> = read_service_file()
		.map(|service| tree_of(service.pid, &processes).iter().map(|info| info.pid).collect())
		.unwrap_or_default();
	let ended = end_restart(restart_pid, &keep, &processes, kill_process);
	println!("service run {run}: ended the restart's client ({} processes)", ended.len());
	Ok(())
}

/// One fault's rates and times.
fn fault_summary(fault: Fault, rows: &[H3Row]) -> Result<Value> {
	let options = BootstrapOptions::default();
	let of: Vec<&H3Row> = rows.iter().filter(|row| row.fault == fault).collect();
	let times = |pick: fn(&H3Row) -> Option<f64>| -> Vec<f64> { of.iter().filter_map(|row| pick(row)).collect() };
	let flags = |pick: fn(&H3Row) -> bool| -> Vec<bool> { of.iter().map(|row| pick(row)).collect() };
	let mut summary = Map::new();
	summary.insert("recovered".into(), serde_json::to_value(rate(&flags(|row| row.recovered), options))?);
	summary.insert("samePanes".into(), serde_json::to_value(rate(&flags(|row| row.same_panes), options))?);
	summary.insert("foldersIntact".into(), serde_json::to_value(rate(&flags(|row| row.folders_intact), options))?);
	summary.insert("sessionsParse".into(), serde_json::to_value(rate(&flags(|row| row.sessions_parse), options))?);
	summary.insert("recoveryMs".into(), serde_json::to_value(summarize(&times(|row| row.recovery_ms), options))?);
	summary.insert("stoppedItself".into(), json!(of.iter().filter(|row| row.stopped_itself).count()));
	summary.insert("restartMs".into(), serde_json::to_value(summarize(&times(|row| row.restart_ms), options))?);
	if fault == Fault::Herdr {
		let seen = summarize(&times(|row| row.old_pane_ids_ms), options);
		summary.insert("oldPaneIdsMs".into(), serde_json::to_value(seen)?);
	}
	Ok(Value::Object(summary))
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec { default_runs: 20, valued: &["--faults", "--timeout", "--restart"], ..ArgSpec::default() };
	let args = parse_bench_args(&argv, &spec)?;
	let settings = settings(&args)?;
	if args.dry_run {
		print_dry_run(&args, &settings);
		return Ok(());
	}
	let out = args.out.as_deref().ok_or_else(|| BenchUsageError::err("--out <dir> is required (outside the repo)"))?;
	let jocasta = open_sandbox(Some(LIST_MS))?;
	let herdr_session = jocasta.sandbox.herdr_session.clone();
	claim_rows(out, &["h3-recovery"])?;
	let started = conditions(&["herdr", "omp", "jocasta"]);
	let ctx = Context { jocasta, herdr_session, out, settings: &settings };
	let mut rows: Vec<H3Row> = Vec::new();
	// The pane fault's folders: when each one's last run ended (0528).
	let mut last_ended: HashMap<String, f64> = HashMap::new();
	for &fault in &settings.faults {
		for run in 0..args.runs as usize {
			let outcome = one_run(&ctx, fault, run, &last_ended).await?;
			let stuck = outcome.row.stopped_itself && outcome.row.restart_ms.is_none();
			rows.push(outcome.row);
			if stuck {
				return Err(Error::other(format!(
					"Jocasta closed itself in {} run {run} and omp wasn't back everywhere {} min after its restart",
					fault.as_str(),
					RESTART_BOUND_MS / 60_000
				)));
			}
			if let Some(folder) = outcome.killed_folder {
				last_ended.insert(folder, now_ms() as f64);
			}
			if let Some(pid) = outcome.restart_pid {
				end_restart_client(run, pid)?;
			}
			// Settle before the next fault.
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	}
	let mut by_fault = Map::new();
	for &fault in &settings.faults {
		by_fault.insert(fault.as_str().to_owned(), fault_summary(fault, &rows)?);
	}
	let renamed: Map<String, Value> =
		RENAMED_FAULTS.iter().map(|(fault, old)| (fault.as_str().to_owned(), json!(old))).collect();
	let summary = json!({
		"benchmark": "H3",
		"conditions": started.finished(),
		"renamed": renamed,
		"byFault": by_fault,
	});
	println!("wrote {}", write_json(out, "h3-recovery", &summary)?.display());
	Ok(())
}

#[cfg(test)]
#[path = "h3_recovery_tests.rs"]
mod tests;
