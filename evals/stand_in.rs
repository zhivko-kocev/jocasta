//! The stand-ins the runner's tests start in place of a real harness, as the TypeScript's `test/evals/fixtures/*.ts`
//! (omp, claude, sleeper) and the one-line `bun -e` scripts its tests used as shell commands. A stand-in is this very
//! binary started as `jocasta-bench stand-in <mode> …`; the arm exists only in builds with `test-support`, so a release
//! `jocasta-bench` has none. (The test executable itself can't stand in: libtest prints its own lines on stdout, where a
//! launcher's `--version` must print one line of its own.)

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::{Value, json};

/// Runs the stand-in `argv` (the arguments after the program name) names, or `None` when it names none.
pub fn run(argv: &[String]) -> Option<i32> {
	if argv.first().map(String::as_str) != Some("stand-in") {
		return None;
	}
	let mode = argv.get(1).map_or("", String::as_str);
	let rest = argv.get(2..).unwrap_or_default();
	Some(match mode {
		"omp" => omp(rest),
		"claude" => claude(rest),
		"sleeper" => sleeper(rest),
		"branch" => branch(rest),
		"sleep" => {
			sleep_for(rest.first().and_then(|ms| ms.parse().ok()).unwrap_or(0));
			0
		}
		"hang" => hang(rest),
		"file-is" => file_is(rest),
		"missing" => i32::from(rest.first().is_some_and(|file| std::path::Path::new(file).exists())),
		"hooks" => hooks(),
		_ => 2,
	})
}

fn sleep_for(ms: u64) {
	std::thread::sleep(Duration::from_millis(ms));
}

fn after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
	let at = argv.iter().position(|arg| arg == flag)?;
	argv.get(at + 1).map(String::as_str)
}

fn print(line: &Value) {
	println!("{line}");
	let _ = std::io::stdout().flush();
}

/// `Write <file>: <text>` as a prompt writes that file in the cwd.
fn write_prompt(prompt: &str) -> Option<String> {
	static WRITE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Write (\S+): (.*)$").expect("a fixed pattern"));
	let found = WRITE.captures(prompt)?;
	let _ = std::fs::write(&found[1], format!("{}\n", &found[2]));
	Some(found[1].to_owned())
}

fn append_line(file: &str, line: &Value) {
	if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
		let _ = writeln!(out, "{line}");
	}
}

fn usage(input: u64, total: f64) -> Value {
	json!({
		"input": input,
		"output": 10,
		"cacheRead": 400,
		"cacheWrite": 0,
		"totalTokens": input + 410,
		"cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": total },
	})
}

fn cwd_text() -> String {
	std::env::current_dir().map(|dir| dir.display().to_string()).unwrap_or_default()
}

/// A stand-in for `omp`: `--version` prints a version; otherwise it prints canned `--mode json` lines (the session
/// header, a `task` tool's result with one subagent's usage, two assistant `message_end` turns). A prompt
/// `Write <file>: <text>` writes that file in the cwd; a prompt starting `Fail` exits 1; a prompt starting `Review`
/// answers with two findings, `[P1]` and `[P3]`. Each run appends its argv, its `--config` overlay's contents, its cwd,
/// git branch and `--append-system-prompt` file's text to `JOCASTA_TEST_OMP_LOG` when set. With `--session-dir <dir>` it
/// also writes a session file there (one priced request on `claude-opus-5-5`, one `read` call and its result), as omp
/// keeps one per session; with `--prewalk` it adds the `model_change` entry (role `prewalk`) omp writes when the prewalk
/// switch fires.
fn omp(argv: &[String]) -> i32 {
	if argv.iter().any(|arg| arg == "--version") {
		println!("omp/0.0.0-stub");
		return 0;
	}
	let prompt = after(argv, "-p").unwrap_or_default();
	if let Ok(log) = std::env::var("JOCASTA_TEST_OMP_LOG") {
		let overlay = after(argv, "--config").and_then(|file| std::fs::read_to_string(file).ok());
		let branch = Command::new("git").args(["branch", "--show-current"]).output().ok();
		let branch = branch.map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned()).unwrap_or_default();
		let appended = after(argv, "--append-system-prompt").and_then(|file| std::fs::read_to_string(file).ok());
		let mut entry = json!({ "argv": argv });
		if let Some(overlay) = overlay.and_then(|text| serde_json::from_str::<Value>(&text).ok()) {
			entry["overlay"] = overlay;
		}
		entry["cwd"] = json!(cwd_text());
		entry["branch"] = json!(branch);
		if let Some(appended) = appended {
			entry["appendedText"] = json!(appended);
		}
		append_line(&log, &entry);
	}
	let written = write_prompt(prompt);
	let text = if prompt.starts_with("Review") { "[P1] a.ts:1 wrong sum\n[P3] b.ts:2 naming" } else { "Done." };
	print(&json!({ "type": "session", "version": 3, "id": "stub-session", "cwd": cwd_text() }));
	print(&json!({ "type": "agent_start" }));
	print(
		&json!({ "type": "message_end", "message": { "role": "user", "content": [{ "type": "text", "text": prompt }] } }),
	);
	print(&json!({
		"type": "message_end",
		"message": {
			"role": "assistant",
			"content": [{ "type": "toolCall", "id": "call-1", "name": "write", "arguments": { "path": written.unwrap_or_default() } }],
			"provider": "anthropic",
			"model": "stub-model",
			"usage": usage(500, 0.5),
		},
	}));
	print(&json!({
		"type": "tool_execution_end",
		"toolCallId": "call-2",
		"toolName": "task",
		"result": { "content": [], "details": { "results": [{ "requests": 2 }], "usage": usage(100, 0.25) } },
	}));
	print(&json!({
		"type": "message_end",
		"message": {
			"role": "assistant",
			"content": [{ "type": "text", "text": text }],
			"provider": "anthropic",
			"model": "stub-model",
			"usage": usage(600, 0.5),
		},
	}));
	print(&json!({ "type": "agent_end", "messages": [] }));
	if let Some(dir) = after(argv, "--session-dir") {
		write_session(dir, argv.iter().any(|arg| arg == "--prewalk"));
	}
	i32::from(prompt.starts_with("Fail"))
}

fn write_session(dir: &str, prewalk: bool) {
	let _ = std::fs::create_dir_all(dir);
	let at = "2026-11-09T10:00:00.000Z";
	let mut entries = vec![
		json!({ "type": "session", "version": 3, "id": "stub-session", "timestamp": at, "cwd": cwd_text() }),
		json!({
			"type": "message",
			"id": "m1",
			"timestamp": at,
			"message": {
				"role": "assistant",
				"provider": "anthropic",
				"model": "claude-opus-5-5",
				"content": [{ "type": "toolCall", "id": "c1", "name": "read", "arguments": { "path": "lib.txt", "i": "Reading lib" } }],
				"usage": { "input": 1000, "output": 100, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 1100 },
			},
		}),
		json!({
			"type": "message",
			"id": "m2",
			"timestamp": at,
			"message": { "role": "toolResult", "toolCallId": "c1", "toolName": "read", "content": [{ "type": "text", "text": "hello" }] },
		}),
	];
	if prewalk {
		entries.push(
			json!({ "type": "model_change", "id": "mc1", "timestamp": at, "model": "anthropic/stub-smol", "role": "prewalk" }),
		);
	}
	let file = PathBuf::from(dir).join("stub-session.jsonl");
	if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
		for entry in entries {
			let _ = writeln!(out, "{entry}");
		}
	}
}

/// A stand-in for Claude Code's `claude`: `--version` prints a version; otherwise `-p <prompt>` prints canned
/// `--output-format stream-json` lines (init, one assistant message with a tool_use, the result with usage and cost).
/// A prompt `Write <file>: <text>` writes that file in the cwd. Each run appends its argv to `JOCASTA_TEST_OMP_LOG`.
fn claude(argv: &[String]) -> i32 {
	if argv.iter().any(|arg| arg == "--version") {
		println!("2.1.296 (Claude Code)");
		return 0;
	}
	let prompt = after(argv, "-p").unwrap_or_default();
	if let Ok(log) = std::env::var("JOCASTA_TEST_OMP_LOG") {
		append_line(&log, &json!({ "argv": argv }));
	}
	write_prompt(prompt);
	print(&json!({ "type": "system", "subtype": "init", "model": "claude-stub" }));
	print(&json!({
		"type": "assistant",
		"message": {
			"model": "claude-stub",
			"content": [{ "type": "tool_use", "id": "t1", "name": "Write", "input": {} }, { "type": "text", "text": "Done." }],
		},
	}));
	print(&json!({
		"type": "result",
		"subtype": "success",
		"num_turns": 2,
		"total_cost_usd": 0.5,
		"usage": { "input_tokens": 10, "output_tokens": 20, "cache_read_input_tokens": 300, "cache_creation_input_tokens": 40 },
	}));
	0
}

/// A sleeping child of this stand-in (so the trial is a process tree), its pid written to `pid_file`.
fn start_sleeping_child(pid_file: &str) {
	if let Ok(exe) = std::env::current_exe()
		&& let Ok(child) = Command::new(exe)
			.args(["stand-in", "sleep", "120000"])
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
	{
		let _ = std::fs::write(pid_file, child.id().to_string());
	}
}

/// An omp that never finishes: `--version` answers, as the runner asks once per run; otherwise it starts a sleeping
/// child (so the trial is a process tree), records the child's pid in `JOCASTA_TEST_SLEEPER_PID_FILE`, prints one JSON
/// line and sleeps.
fn sleeper(argv: &[String]) -> i32 {
	if argv.iter().any(|arg| arg == "--version") {
		println!("omp/0.0.0-sleeper");
		return 0;
	}
	start_sleeping_child(&std::env::var("JOCASTA_TEST_SLEEPER_PID_FILE").unwrap_or_default());
	print(&json!({ "type": "agent_start" }));
	sleep_for(120_000);
	0
}

/// A shell command that never ends: it starts a sleeping child, records its pid in the file named, and sleeps.
fn hang(argv: &[String]) -> i32 {
	start_sleeping_child(argv.first().map_or("", String::as_str));
	sleep_for(120_000);
	0
}

/// Exits 0 when the file's trimmed text is the one given, after the delay given (ms); 1 otherwise, a missing file too.
fn file_is(argv: &[String]) -> i32 {
	let [file, text, rest @ ..] = argv else { return 2 };
	let same = std::fs::read_to_string(file).is_ok_and(|content| content.trim() == text);
	sleep_for(rest.first().and_then(|ms| ms.parse().ok()).unwrap_or(0));
	i32::from(!same)
}

/// An omp that commits to a branch of its own: `--version` answers; otherwise it checks out `fix/issue-3` in its cwd and
/// writes `out.txt`.
fn branch(argv: &[String]) -> i32 {
	if argv.iter().any(|arg| arg == "--version") {
		println!("omp/0.0.0-stub");
		return 0;
	}
	let _ = Command::new("git").args(["checkout", "-q", "-b", "fix/issue-3"]).status();
	let _ = std::fs::write("out.txt", "hello\n");
	0
}

/// The launcher of a stand-in `mode`: the `jocasta-bench` the test executable's package built.
#[cfg(test)]
pub fn launcher(mode: &str) -> Vec<String> {
	vec![exe(), "stand-in".to_owned(), mode.to_owned()]
}

/// The `jocasta-bench` executable of this build: the one in the nearest folder above the test executable that has one
/// (`target/<profile>/`; a test executable of the same name sits in `deps/` or `build/` below it).
#[cfg(test)]
pub fn exe() -> String {
	let test_exe = std::env::current_exe().expect("the test executable");
	let name = format!("jocasta-bench{}", std::env::consts::EXE_SUFFIX);
	let own = test_exe.parent().expect("a folder");
	own.ancestors()
		.skip(1)
		.map(|folder| folder.join(&name))
		.find(|file| file.exists())
		.expect("the jocasta-bench binary, built beside the tests")
		.display()
		.to_string()
}

/// A setup that installs hooks refusing every commit, husky's way (`core.hooksPath`) and simple-git-hooks' way
/// (`.git/hooks`).
fn hooks() -> i32 {
	for dir in [".husky", ".git/hooks"] {
		let _ = std::fs::create_dir_all(dir);
		let file = format!("{dir}/pre-commit");
		let _ = std::fs::write(&file, "#!/bin/sh\nexit 1\n");
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
		}
	}
	let _ = Command::new("git").args(["config", "core.hooksPath", ".husky"]).status();
	0
}
