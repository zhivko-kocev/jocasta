//! Starting a terminal agent in a real terminal for H5 and H6: Bun's PTY is POSIX-only, so on Windows each tool starts
//! in a pane of a separate bench herdr session (`--pty-session`, never his or the sandbox Jocasta's), and readiness is
//! herdr's `pane.wait_for_output` matching the tool's ready pattern on the visible screen.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use regex::Regex;
use serde_json::json;

use crate::lib::args::BenchUsageError;
use crate::lib::herdr::herdr_request;
use crate::lib::sandbox::refuse_his_session;
use jocasta::frontier::server::herdr::{Env, Herdr, HerdrOptions, TabSpec, connect_herdr};
use jocasta::shared::env;
use jocasta::shared::errors::Result;
use jocasta::shared::process::on_path;
use jocasta::shared::proctree::kill_tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BenchTool {
	Omp,
	Claude,
	Opencode,
}

pub const BENCH_TOOLS: [BenchTool; 3] = [BenchTool::Omp, BenchTool::Claude, BenchTool::Opencode];

impl BenchTool {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Omp => "omp",
			Self::Claude => "claude",
			Self::Opencode => "opencode",
		}
	}

	fn named(name: &str) -> Option<Self> {
		BENCH_TOOLS.into_iter().find(|tool| tool.as_str() == name)
	}

	/// What each tool shows once its prompt takes input (`--ready tool=<regex>` replaces one). Seen in the VM on
	/// 2026-10-28: omp 18.13.1's editor hint below its input box; unsigned opencode 1.18.22's command hint under its
	/// prompt; Claude Code 2.1.296's footer, which reads "auto mode on (shift+tab to cycle)" in its default mode, auto,
	/// and "? for shortcuts" in the others.
	fn default_ready(self) -> &'static str {
		match self {
			Self::Omp => "to change thinking effort",
			Self::Claude => "\\? for shortcuts|shift\\+tab to cycle",
			Self::Opencode => "ctrl\\+p",
		}
	}

	/// Why the VM runs this tool without a sign-in (0506): timed starting and measured idle, never sent a prompt.
	pub fn unsigned(self) -> Option<&'static str> {
		match self {
			Self::Opencode => Some("not signed in: its Anthropic sign-in takes only an API key (0506)"),
			_ => None,
		}
	}
}

/// Claude Code asks whether to trust each folder it hasn't seen, and a bench pane can't answer: with Claude Code in the
/// tools, the panes work in `--cwd`, a folder trusted once, never in a fresh temp folder.
pub fn require_trusted_cwd(tools: &[impl AsRef<str>], cwd: Option<&str>) -> Result<()> {
	if tools.iter().any(|tool| tool.as_ref() == "claude") && cwd.is_none() {
		return Err(BenchUsageError::err(
			"Claude Code asks to trust each new folder, and a bench pane can't answer: pass --cwd <a folder trusted once in Claude Code>",
		));
	}
	Ok(())
}

static CMD_SHIM_TARGET: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"(?i)"%dp0%\\([^"]+\.exe)""#).expect("a fixed pattern"));
static CMD_OR_BAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.(cmd|bat)$").expect("a fixed pattern"));

/// How a tool starts in a bench pane: the program a command line runs for its name (`on_path`, which follows PATHEXT on
/// Windows). herdr 0.9.5 picks the extensionless shim npm writes beside `opencode.cmd` for Git Bash, which Windows
/// can't start ("not a valid Win32 application"); and an npm `.cmd` shim that only runs an `.exe` is replaced by that
/// `.exe`, so the pane holds the tool alone, with no cmd.exe in its startup time or its memory.
pub fn tool_command(tool: BenchTool) -> Result<Vec<String>> {
	tool_command_with(tool, &|name| on_path(name, None))
}

/// [`tool_command`] with `which` finding programs.
pub fn tool_command_with(tool: BenchTool, which: &dyn Fn(&str) -> Option<PathBuf>) -> Result<Vec<String>> {
	let Some(found) = which(tool.as_str()) else {
		return Err(BenchUsageError::err(format!("{} is not on PATH", tool.as_str())));
	};
	let found_text = found.to_string_lossy().into_owned();
	if !CMD_OR_BAT.is_match(&found_text) {
		return Ok(vec![found_text]);
	}
	let shim = fs::read_to_string(&found)?;
	let Some(target) = CMD_SHIM_TARGET.captures(&shim).map(|found| found[1].to_owned()) else {
		return Ok(vec!["cmd.exe".to_owned(), "/d".to_owned(), "/c".to_owned(), found_text]);
	};
	let mut exe = found.parent().map(Path::to_path_buf).unwrap_or_default();
	exe.extend(target.split('\\'));
	Ok(vec![exe.to_string_lossy().into_owned()])
}

/// `--ready omp=<regex>,claude=<regex>` over the defaults.
pub fn ready_patterns(spec: Option<&str>) -> Result<BTreeMap<BenchTool, String>> {
	let mut patterns: BTreeMap<BenchTool, String> =
		BENCH_TOOLS.iter().map(|tool| (*tool, tool.default_ready().to_owned())).collect();
	for part in spec.map_or_else(Vec::new, |spec| spec.split(',').collect()) {
		let at = part.find('=');
		let tool = at.filter(|at| *at >= 1).and_then(|at| BenchTool::named(&part[..at]));
		let (Some(at), Some(tool)) = (at, tool) else {
			return Err(BenchUsageError::err(format!("--ready takes tool=<regex>, not \"{part}\"")));
		};
		patterns.insert(tool, part[at + 1..].to_owned());
	}
	Ok(patterns)
}

/// The bench herdr session's client, never his session (`jocasta`, `jarvis`) nor the sandbox Jocasta's
/// (`JOCASTA_HERDR_SESSION`, from `environment` or the process's), whose service relaunches what runs there. A session
/// with no server gets one: a plain `jocasta --session <name> server` (the `jocasta` beside this binary), which runs no
/// Jocasta service.
pub async fn bench_herdr(session: &str, environment: Option<&HashMap<String, String>>) -> Result<Arc<dyn Herdr>> {
	refuse_his_session(session, "--pty-session")?;
	let sandbox = match environment {
		Some(map) => map.get("JOCASTA_HERDR_SESSION").cloned().filter(|value| !value.is_empty()),
		None => env::var_nonempty("JOCASTA_HERDR_SESSION"),
	};
	if sandbox.is_some_and(|sandbox| session.to_lowercase() == sandbox.to_lowercase()) {
		return Err(BenchUsageError::err(format!("--pty-session can't be the sandbox's session {session}")));
	}
	let options = HerdrOptions { session: session.to_owned(), ..HerdrOptions::default() };
	if let Ok(herdr) = connect_herdr(options.clone()).await {
		return Ok(herdr);
	}
	let jocasta = std::env::current_exe()?.with_file_name(format!("jocasta{}", std::env::consts::EXE_SUFFIX));
	let mut server = std::process::Command::new(jocasta);
	server
		.args(["--session", session, "server"])
		.env_remove("JOCASTA_SERVICE")
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null());
	server.spawn()?;
	let deadline = Instant::now() + std::time::Duration::from_secs(10);
	loop {
		match connect_herdr(options.clone()).await {
			Ok(herdr) => return Ok(herdr),
			Err(error) if Instant::now() >= deadline => return Err(error.into()),
			Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedPane {
	pub tab_id: String,
	pub pane_id: String,
}

/// What [`launch_in_pane`] may be given.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
	/// `bench <program>` when absent.
	pub title: Option<String>,
	pub env: Option<Env>,
}

/// A new tab in the bench workspace running `command` in `cwd`, titled `bench <program>` unless `title` is given.
pub async fn launch_in_pane(
	herdr: &dyn Herdr,
	workspace_id: &str,
	command: &[String],
	cwd: &Path,
	options: LaunchOptions,
) -> Result<LaunchedPane> {
	let title = options.title.unwrap_or_else(|| format!("bench {}", command.first().map_or("", String::as_str)));
	let spec = TabSpec { title, cwd: cwd.to_path_buf(), command: command.to_vec(), env: options.env };
	let created = herdr.create_tab(workspace_id, &spec).await?;
	Ok(LaunchedPane { tab_id: created.tab_id, pane_id: created.pane_id })
}

/// ms until the pane's visible screen matches `pattern`, or `None` past `timeout_ms`; timed here, waited on by herdr.
pub async fn wait_ready(session: &str, pane_id: &str, pattern: &str, timeout_ms: i64) -> Option<f64> {
	let started = Instant::now();
	let params = json!({
		"pane_id": pane_id,
		"source": "visible",
		"match": { "type": "regex", "value": pattern },
		"timeout_ms": timeout_ms,
	});
	herdr_request::<serde_json::Value>(session, "pane.wait_for_output", &params, Some(timeout_ms + 5_000))
		.await
		.ok()?;
	Some(started.elapsed().as_secs_f64() * 1000.0)
}

/// Ends the pane's process tree and closes its tab.
pub async fn close_pane(herdr: &dyn Herdr, session: &str, pane: &LaunchedPane) {
	if let Some(root) = herdr.pane_root(&pane.pane_id, None).await.ok().flatten() {
		kill_tree(root).await;
	}
	let _ = herdr_request::<serde_json::Value>(session, "tab.close", &json!({ "tab_id": pane.tab_id }), None).await;
}
