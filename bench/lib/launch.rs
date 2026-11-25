//! Starting a terminal agent in a real terminal for H5 and H6 and the task benchmark's plan-review turns: each tool
//! runs in a pty of its own (`pty.rs`, the vendored portable-pty), never in Jocasta's shell, and readiness is the tool's
//! ready pattern matching the pty's visible screen.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::lib::args::BenchUsageError;
use crate::lib::pty::Pty;
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

/// How a tool starts in a bench pty: the program a command line runs for its name (`on_path`, which follows PATHEXT on
/// Windows). An extensionless shim npm writes beside `opencode.cmd` for Git Bash can't be started by Windows ("not a
/// valid Win32 application"); and an npm `.cmd` shim that only runs an `.exe` is replaced by that `.exe`, so the pty
/// holds the tool alone, with no cmd.exe in its startup time or its memory.
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

/// A tool started in a pty of its own, as H5 and H6 and the task benchmark's plan-review turns start one (0683): the
/// tool alone, with `env` over this process's environment. Jocasta's panes are not involved.
pub fn launch(command: &[String], cwd: &Path, env: &[(String, String)]) -> Result<Pty> {
	Ok(Pty::spawn(command, cwd, env, &[])?)
}

/// Closes the tool's window: its process tree ends, then the pty.
pub async fn close_pty(pty: &Pty) {
	if let Some(root) = pty.pid() {
		kill_tree(root).await;
	}
	pty.end();
}
