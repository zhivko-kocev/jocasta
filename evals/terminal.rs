//! A terminal for a trial turn that needs one (0567: omp's interactive plan review needs a TTY on stdin, `main.ts`).
//! Bun's PTY is POSIX-only, so the turn runs in a pty of the bench's own (`crate::lib::pty`, the same one H3, H5 and H6
//! use), with the trial's environment as the process's. The turn ends when the process is gone; past the limit its tree
//! is killed. The pty's exit code isn't read: the caller judges the turn by what the turn itself logged.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;

use crate::lib::pty::Pty;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::proctree::kill_tree;

const POLL_MS: u64 = 250;

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

/// Runs a turn in a terminal; tests give their own instead of a pty.
pub type TerminalRunner = Arc<dyn Fn(TerminalTurn) -> BoxFuture<'static, Result<TerminalEnd>> + Send + Sync>;

/// The variables `env` sets or changes against `base` (this process's), and the names it drops, so a key removed from
/// the trial's environment stays out of the pty's process. Both sorted by name.
pub fn env_changes(
	env: &HashMap<String, String>,
	base: &HashMap<String, String>,
) -> (Vec<(String, String)>, Vec<String>) {
	let mut set: Vec<(String, String)> =
		env.iter().filter(|(key, value)| base.get(*key) != Some(value)).map(|(k, v)| (k.clone(), v.clone())).collect();
	set.sort();
	let mut unset: Vec<String> = base.keys().filter(|key| !env.contains_key(*key)).cloned().collect();
	unset.sort();
	(set, unset)
}

/// Runs a turn in a pty and waits for its process to end; past the turn's limit the process tree is killed.
pub fn pty_terminal() -> TerminalRunner {
	Arc::new(move |turn: TerminalTurn| {
		Box::pin(async move {
			let started = Instant::now();
			let (set, unset) = env_changes(&turn.env, &crate::evals::runner::process_env());
			let unset: Vec<&str> = unset.iter().map(String::as_str).collect();
			let mut argv = vec![turn.command.clone()];
			argv.extend(turn.args.iter().cloned());
			let pty = Pty::spawn(&argv, &turn.cwd, &set, &unset).map_err(|error| Error::other(error.to_string()))?;
			let mut timed_out = false;
			while pty.running() {
				if turn.timeout_ms.is_some_and(|limit| started.elapsed().as_millis() as u64 > limit) {
					timed_out = true;
					if let Some(root) = pty.pid() {
						kill_tree(root).await;
					}
					break;
				}
				tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
			}
			pty.end();
			Ok(TerminalEnd { timed_out, wall_ms: (started.elapsed().as_secs_f64() * 1000.0).round() as u64 })
		}) as BoxFuture<'static, Result<TerminalEnd>>
	})
}
