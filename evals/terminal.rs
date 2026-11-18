//! A terminal for a trial turn that needs one (0567: omp's interactive plan review needs a TTY on stdin, `main.ts`).
//! Bun's PTY is POSIX-only, so on Windows the turn runs in a tab of a separate bench herdr session (`--pty-session`,
//! default `BENCH_PTY_SESSION`, as H5 and H6 use; never his or a sandbox Jocasta's), in the workspace
//! [`TRIAL_WORKSPACE`]. The turn ends when the tab's process is gone; past the limit its tree is killed. herdr
//! doesn't report the exit code: the caller judges the turn by what the turn itself logged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::lib::herdr::herdr_request;
use crate::lib::sandbox::refuse_his_session;
use jocasta::frontier::server::herdr::{Env, Herdr, HerdrOptions, TabPane, TabSpec, connect_herdr};
use jocasta::shared::errors::Result;
use jocasta::shared::proctree::{kill_tree, pid_alive};

pub const TRIAL_WORKSPACE: &str = "bench-trials";
const POLL_MS: u64 = 1_000;

/// One turn to run in a terminal: the command and its arguments, the folder, the environment (whole, as the trial's
/// own) and the time left for it.
#[derive(Debug, Clone)]
pub struct TerminalTurn {
	pub command: String,
	pub args: Vec<String>,
	pub cwd: PathBuf,
	pub env: HashMap<String, String>,
	pub timeout_ms: Option<u64>,
}

/// How a terminal turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalEnd {
	pub timed_out: bool,
	pub wall_ms: u64,
}

/// Runs a turn in a terminal; tests give their own instead of a herdr pane.
pub type TerminalRunner = Arc<dyn Fn(TerminalTurn) -> BoxFuture<'static, Result<TerminalEnd>> + Send + Sync>;

/// The part of herdr's client a trial's terminal uses; tests give a fake (the TypeScript's `Pick<Herdr, …>`).
pub trait TrialPanes: Send + Sync {
	fn ensure_workspace<'a>(&'a self, label: &'a str, cwd: &'a Path) -> BoxFuture<'a, Result<String>>;
	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, Result<TabPane>>;
	fn pane_root<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, Result<Option<u32>>>;
	fn close(&self);
}

struct HerdrPanes(Arc<dyn Herdr>);

impl TrialPanes for HerdrPanes {
	fn ensure_workspace<'a>(&'a self, label: &'a str, cwd: &'a Path) -> BoxFuture<'a, Result<String>> {
		Box::pin(async move { Ok(self.0.ensure_workspace(label, cwd, None, None).await?) })
	}

	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, Result<TabPane>> {
		Box::pin(async move { Ok(self.0.create_tab(workspace_id, tab).await?) })
	}

	fn pane_root<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, Result<Option<u32>>> {
		Box::pin(async move { Ok(self.0.pane_root(pane_id, None).await?) })
	}

	fn close(&self) {
		self.0.close();
	}
}

pub type Connect = Arc<dyn Fn(String) -> BoxFuture<'static, Result<Arc<dyn TrialPanes>>> + Send + Sync>;
pub type CloseTab = Arc<dyn Fn(String, String) -> BoxFuture<'static, ()> + Send + Sync>;

/// What [`herdr_terminal`] talks to; tests give a fake.
#[derive(Default, Clone)]
pub struct TerminalDeps {
	/// Where the trials' workspace starts its own default shell (EVL-17): the work root, never a trial's repo, which that
	/// shell would hold for as long as the session lives. The OS temp folder when unset.
	pub workspace_cwd: Option<PathBuf>,
	pub connect: Option<Connect>,
	pub close_tab: Option<CloseTab>,
}

/// The variables `env` sets or changes against `base` (this process's), for the pane's command (herdr's server supplies
/// the rest); a variable `env` drops becomes empty, so a key removed from the trial's environment stays out of the
/// pane's. Sorted by name.
pub fn env_delta(env: &HashMap<String, String>, base: &HashMap<String, String>) -> Env {
	let mut delta: HashMap<String, String> = HashMap::new();
	for (key, value) in env {
		if base.get(key) != Some(value) {
			delta.insert(key.clone(), value.clone());
		}
	}
	for key in base.keys() {
		if !env.contains_key(key) {
			delta.insert(key.clone(), String::new());
		}
	}
	let mut sorted: Vec<(String, String)> = delta.into_iter().collect();
	sorted.sort();
	sorted
}

/// Runs a turn in a new tab of `session`'s [`TRIAL_WORKSPACE`] and waits for its process to end. The tab's process id is
/// read once through the session's own client, asked again until herdr names it (a dropped answer is asked again,
/// 0517); from then on the process is watched locally (EVL-6), so a herdr hiccup never ends a turn early.
pub fn herdr_terminal(session: &str, deps: TerminalDeps) -> Result<TerminalRunner> {
	refuse_his_session(session, "the trials' terminal session")?;
	let session = session.to_owned();
	let connect: Connect = deps.connect.clone().unwrap_or_else(|| {
		Arc::new(|name: String| -> BoxFuture<'static, Result<Arc<dyn TrialPanes>>> {
			Box::pin(async move {
				let herdr = connect_herdr(HerdrOptions { session: name, ..HerdrOptions::default() }).await?;
				Ok(Arc::new(HerdrPanes(herdr)) as Arc<dyn TrialPanes>)
			})
		})
	});
	let close_tab: CloseTab = deps.close_tab.clone().unwrap_or_else(|| {
		Arc::new(|name: String, tab_id: String| -> BoxFuture<'static, ()> {
			Box::pin(async move {
				let _ = herdr_request::<Value>(&name, "tab.close", &json!({ "tab_id": tab_id }), None).await;
			})
		})
	});
	let workspace_cwd = deps.workspace_cwd.clone().unwrap_or_else(std::env::temp_dir);
	Ok(Arc::new(move |turn: TerminalTurn| {
		let (session, connect, close_tab, workspace_cwd) =
			(session.clone(), connect.clone(), close_tab.clone(), workspace_cwd.clone());
		Box::pin(async move {
			let started = Instant::now();
			let herdr = connect(session.clone()).await?;
			let ended = watch_turn(
				herdr.as_ref(),
				&Pane { session: &session, close_tab: &close_tab, workspace_cwd: &workspace_cwd },
				&turn,
				started,
			)
			.await;
			herdr.close();
			ended
		})
	}))
}

/// Where a turn runs: the session, how its tab closes, and where the trials' workspace starts.
struct Pane<'a> {
	session: &'a str,
	close_tab: &'a CloseTab,
	workspace_cwd: &'a Path,
}

async fn watch_turn(
	herdr: &dyn TrialPanes,
	where_: &Pane<'_>,
	turn: &TerminalTurn,
	started: Instant,
) -> Result<TerminalEnd> {
	let late = || turn.timeout_ms.is_some_and(|limit| started.elapsed().as_millis() as u64 > limit);
	let workspace = herdr.ensure_workspace(TRIAL_WORKSPACE, where_.workspace_cwd).await?;
	let mut command = vec![turn.command.clone()];
	command.extend(turn.args.iter().cloned());
	let tab = TabSpec {
		title: "bench trial".to_owned(),
		cwd: turn.cwd.clone(),
		command,
		env: Some(env_delta(&turn.env, &crate::evals::runner::process_env())),
	};
	let pane = herdr.create_tab(&workspace, &tab).await?;
	let mut pid: Option<u32> = None;
	while pid.is_none() && !late() {
		pid = herdr.pane_root(&pane.pane_id).await.ok().flatten();
		if pid.is_none() {
			tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
		}
	}
	let mut timed_out = pid.is_none();
	while let Some(root) = pid.filter(|root| pid_alive(*root)) {
		if late() {
			timed_out = true;
			kill_tree(root).await;
			break;
		}
		tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
	}
	(where_.close_tab)(where_.session.to_owned(), pane.tab_id.clone()).await;
	Ok(TerminalEnd { timed_out, wall_ms: (started.elapsed().as_secs_f64() * 1000.0).round() as u64 })
}
