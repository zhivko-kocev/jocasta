//! The client of H1's oracle (`omp-verdicts.ts`, which stays TypeScript, 0636): omp's own code loads only on Bun, so
//! the scripts reach it through a Bun process running that file, which finds and loads omp (`findOmpDir`, `loadOmp`),
//! judges a command through omp's approval (`ompJudge`) and runs one through `executeBash`, one request per line. The
//! oracle answers omp's approval; the git guard's refusals (measured, or `git_guard`'s reimplementation applied on omp's
//! tokenizer) are combined with it here.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::git_guard::{MeasuredGitGuardLine, git_guard_refusal_of};
use crate::lib::args::{BenchUsageError, REPO_ROOT};
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::guard::Verdict;
use jocasta::shared::process::on_path;

/// Every answer of the oracle starts with this; omp's own prints are ignored.
const ORACLE_MARK: &str = "\u{1}ORACLE ";

/// omp's package as the oracle loaded it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpInfo {
	pub dir: String,
	pub version: String,
	/// The bash tool can run a call in an ACP client's terminal.
	pub client_terminal_route: bool,
	/// The kinds of the options omp's ACP permission gate offers.
	pub permission_kinds: Vec<String>,
}

/// omp's settings as `ompJudge` summarizes them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpSettingsSummary {
	pub approval_mode: String,
	pub user_policies: Vec<String>,
	pub git_guard: bool,
	pub allow_compound_commands: bool,
	pub patterns: usize,
	pub restricted_folders: bool,
}

impl OmpSettingsSummary {
	/// `JSON.stringify(summary)`, as the plan and the results print it.
	pub fn json(&self) -> Value {
		json!({
			"approvalMode": self.approval_mode,
			"userPolicies": self.user_policies,
			"gitGuard": self.git_guard,
			"allowCompoundCommands": self.allow_compound_commands,
			"patterns": self.patterns,
			"restrictedFolders": self.restricted_folders,
		})
	}
}

/// What the layered settings say about the recorder's needs, from omp's own code.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeState {
	pub approval_mode_configured: bool,
	pub interceptor_enabled: bool,
	pub launch_enabled: bool,
	pub async_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeInit {
	pub summary: OmpSettingsSummary,
	pub state: JudgeState,
}

/// What the oracle's judge answers for a line: omp's approval alone.
#[derive(Debug, Clone, Deserialize)]
struct Approved {
	approval: Verdict,
	/// guard-pattern, critical-pattern, mode-default, user-policy, restricted-folder or tool.
	mechanism: String,
	#[serde(rename = "match")]
	pattern: Option<String>,
	critical: bool,
}

/// omp's verdict for one line: its approval, then the git guard's refusal on top (0580, 0623).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpVerdict {
	pub verdict: Verdict,
	/// guard-pattern, git-guard, critical-pattern, mode-default, user-policy, restricted-folder or tool.
	pub mechanism: String,
	/// The guard pattern, or the git guard's refusal.
	#[serde(rename = "match")]
	pub pattern: Option<String>,
	/// omp's approval alone (before the git guard).
	pub approval: Verdict,
	/// omp's `CRITICAL_BASH_PATTERNS` flagged the line (under `yolo` that asks nothing).
	pub critical: bool,
	/// The git guard's refusal, whatever the approval said (from `git_guard_source`).
	pub git_guard: Option<String>,
	/// `measured` (git-guard-check ran the line through omp's embedded shell), `reimplemented` (`git_guard`), or `off`
	/// (the guard has no `bash.gitGuard`).
	pub git_guard_source: &'static str,
}

/// One run of a line through omp's `executeBash`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Executed {
	pub exit_code: Option<i32>,
	pub timed_out: bool,
	pub output: String,
}

/// One line for omp's `executeBash`: its working folder, time limit, git guard setting and environment.
pub struct ExecCall<'a> {
	pub command: &'a str,
	pub cwd: &'a str,
	pub timeout_ms: u64,
	pub git_guard: bool,
	pub env: &'a Map<String, Value>,
}

/// A running oracle; dropping it ends the Bun process.
pub struct Oracle {
	/// The settings of `judge_init` have `bash.gitGuard` on.
	git_guard: bool,
	child: Child,
	stdin: ChildStdin,
	answers: Receiver<String>,
}

impl Oracle {
	/// Starts `bun bench/h1-guard/omp-verdicts.ts` in the repo.
	pub fn start() -> Result<Self> {
		let bun = on_path("bun", None).ok_or_else(|| {
			BenchUsageError::err("bun is not on PATH: omp's own code (H1's oracle, omp-verdicts.ts) loads only on Bun")
		})?;
		let script: PathBuf = REPO_ROOT.join("bench").join("h1-guard").join("omp-verdicts.ts");
		let mut command = Command::new(bun);
		command
			.arg(script)
			.current_dir(&*REPO_ROOT)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit());
		env::apply_to(&mut command);
		#[cfg(windows)]
		{
			use std::os::windows::process::CommandExt;
			command.creation_flags(0x0800_0000);
		}
		let mut child = command.spawn()?;
		let stdin = child.stdin.take().ok_or_else(|| Error::other("the oracle has no stdin"))?;
		let stdout = child.stdout.take().ok_or_else(|| Error::other("the oracle has no stdout"))?;
		let (send, answers) = mpsc::channel();
		thread::spawn(move || {
			for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
				if let Some(reply) = line.strip_prefix(ORACLE_MARK)
					&& send.send(reply.to_owned()).is_err()
				{
					return;
				}
			}
		});
		Ok(Self { git_guard: false, child, stdin, answers })
	}

	/// One request and its answer; a refusal of the oracle's is an error of the same kind it was there.
	fn call(&mut self, request: &Value) -> Result<Value> {
		writeln!(self.stdin, "{request}")?;
		self.stdin.flush()?;
		let line = self.answers.recv().map_err(|_| Error::other("the omp oracle ended before it answered"))?;
		let mut reply: Value = serde_json::from_str(&line)?;
		if let Some(message) = reply.get("error").and_then(Value::as_str) {
			let usage = reply.get("usage").and_then(Value::as_bool) == Some(true);
			return Err(if usage { BenchUsageError::err(message) } else { Error::other(message) });
		}
		Ok(reply.get_mut("ok").map_or(Value::Null, Value::take))
	}

	fn typed<T: for<'de> Deserialize<'de>>(&mut self, request: &Value) -> Result<T> {
		Ok(serde_json::from_value(self.call(request)?)?)
	}

	/// The package `findOmpDir` finds, if any.
	pub fn find(&mut self) -> Result<Option<String>> {
		self.typed(&json!({ "op": "find" }))
	}

	/// Imports omp's decision modules from `omp` (or the package `findOmpDir` finds); anything but the pinned version
	/// is refused unless `any_version`.
	pub fn load(&mut self, omp: Option<&Path>, any_version: bool) -> Result<OmpInfo> {
		let omp = omp.map(|dir| dir.to_string_lossy().into_owned());
		self.typed(&json!({ "op": "load", "omp": omp, "anyVersion": any_version }))
	}

	/// omp's `tokenizeShellSegments`.
	pub fn tokenize(&mut self, command: &str) -> Result<Vec<Vec<String>>> {
		self.typed(&json!({ "op": "tokenize", "command": command }))
	}

	/// Loads settings as a pane would (`config_files` as `--config`, in order); `agent_dir` unset means omp's own.
	pub fn judge_init(&mut self, config_files: &[&Path], cwd: &Path, agent_dir: Option<&Path>) -> Result<JudgeInit> {
		let files: Vec<String> = config_files.iter().map(|file| file.to_string_lossy().into_owned()).collect();
		let init: JudgeInit = self.typed(&json!({
			"op": "judge-init",
			"configFiles": files,
			"cwd": cwd.to_string_lossy(),
			"agentDir": agent_dir.map(|dir| dir.to_string_lossy().into_owned()),
		}))?;
		self.git_guard = init.summary.git_guard;
		Ok(init)
	}

	/// omp's verdict for one line, under the settings of `judge_init`: its approval, and where the guard has
	/// `bash.gitGuard` the refusal of that guard, from `measured` (what omp's shell did with this line) or, without it,
	/// the reimplementation on omp's own tokenizer. A line omp allows and the git guard refuses is `deny` by `git-guard`.
	pub fn judge(&mut self, command: &str, measured: Option<&MeasuredGitGuardLine>) -> Result<OmpVerdict> {
		let said: Approved = self.typed(&json!({ "op": "judge", "command": command }))?;
		let (git_guard_source, git_guard) = match (self.git_guard, measured) {
			(false, _) => ("off", None),
			(true, None) => ("reimplemented", git_guard_refusal_of(&self.tokenize(command)?)),
			(true, Some(line)) => ("measured", line.refusal.clone()),
		};
		if let Some(refusal) = &git_guard
			&& said.approval == Verdict::Allow
		{
			return Ok(OmpVerdict {
				verdict: Verdict::Deny,
				mechanism: "git-guard".to_owned(),
				pattern: Some(refusal.clone()),
				approval: Verdict::Allow,
				critical: said.critical,
				git_guard,
				git_guard_source,
			});
		}
		Ok(OmpVerdict {
			verdict: said.approval,
			mechanism: said.mechanism,
			pattern: said.pattern,
			approval: said.approval,
			critical: said.critical,
			git_guard,
			git_guard_source,
		})
	}

	/// Makes `env` this process's environment and starts omp's settings from an empty agent folder under `temp`.
	pub fn exec_init(&mut self, temp: &Path, env: &Map<String, Value>) -> Result<()> {
		self.call(&json!({ "op": "exec-init", "temp": temp.to_string_lossy(), "env": env })).map(|_| ())
	}

	/// omp's `executeBash` for one line.
	pub fn exec(&mut self, call: &ExecCall<'_>) -> Result<Executed> {
		self.typed(&json!({
			"op": "exec",
			"command": call.command,
			"cwd": call.cwd,
			"timeout": call.timeout_ms,
			"gitGuard": call.git_guard,
			"env": call.env,
		}))
	}
}

impl Drop for Oracle {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}
