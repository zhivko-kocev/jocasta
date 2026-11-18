//! `test/entry.test.ts` and the command's part of `test/shared/cli.test.ts`, as black-box tests of the built
//! `jocasta` binary: each one starts it with roots of its own, never his real folders, and no omp, herdr or git on its
//! PATH (0637 gives the usage-error texts clap's format).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The message the thread and worktree commands give when no service runs (`NOT_RUNNING`).
const NOT_RUNNING: &str = "Jocasta isn't running; start it with jocasta.";
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// A folder of the test's own, removed when it ends.
struct Sandbox(PathBuf);

impl Sandbox {
	fn new() -> Self {
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let dir = std::env::temp_dir().join(format!(
			"jocasta-entry-{}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::SeqCst)
		));
		std::fs::create_dir_all(dir.join("bin")).expect("making the sandbox");
		Self(dir)
	}

	fn path(&self) -> &Path {
		&self.0
	}

	/// `jocasta` with every root in the sandbox, and a PATH holding nothing: omp, herdr, mise and git are never found, as
	/// on a CI runner.
	fn jocasta(&self, args: &[&str]) -> Command {
		self.command(Path::new(env!("CARGO_BIN_EXE_jocasta")), args)
	}

	/// [`Sandbox::jocasta`] for the binary at `exe`.
	fn command(&self, exe: &Path, args: &[&str]) -> Command {
		let root = self.path();
		let mut command = Command::new(exe);
		command
			.args(args)
			.env("PATH", root.join("bin"))
			.env("Path", root.join("bin"))
			.env("HOME", root)
			.env("USERPROFILE", root)
			.env("LOCALAPPDATA", root.join("local"))
			.env("APPDATA", root.join("roaming"))
			.env("MISE_CONFIG_DIR", root.join("mise"))
			.env("JOCASTA_DATA_DIR", root.join("data"))
			.env("JOCASTA_CODING_AGENT_DIR", root.join("config"))
			.env("PI_CODING_AGENT_DIR", root.join("omp").join("agent"))
			.env("PI_CONFIG_DIR", root.join("omp"))
			.env_remove("JOCASTA_HERDR_SESSION")
			.env_remove("JOCASTA_PANE_KEY")
			.env_remove("JOCASTA_CALENDAR_ICAL_SECRET")
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped());
		command
	}
}

impl Drop for Sandbox {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// Copies folder `from` to `to`, whole.
fn copy_dir(from: &Path, to: &Path) {
	std::fs::create_dir_all(to).expect("making a folder");
	for entry in std::fs::read_dir(from).expect("reading a folder").flatten() {
		let target = to.join(entry.file_name());
		if entry.file_type().expect("a file type").is_dir() {
			copy_dir(&entry.path(), &target);
		} else {
			std::fs::copy(entry.path(), target).expect("copying a file");
		}
	}
}

struct Out {
	code: Option<i32>,
	stdout: String,
	stderr: String,
}

/// Reads a pipe to its end on a thread of its own, so a long output never fills it.
fn drain(pipe: impl Read + Send + 'static) -> thread::JoinHandle<String> {
	thread::spawn(move || {
		let mut text = String::new();
		let mut pipe = pipe;
		let _ = pipe.read_to_string(&mut text);
		text
	})
}

/// Waits for `child` to end, killing it (and failing) after `seconds`.
fn finish(mut child: Child, seconds: u64) -> Out {
	let stdout = drain(child.stdout.take().expect("piped stdout"));
	let stderr = drain(child.stderr.take().expect("piped stderr"));
	let deadline = Instant::now() + Duration::from_secs(seconds);
	let status = loop {
		if let Some(status) = child.try_wait().expect("waiting for jocasta") {
			break status;
		}
		if Instant::now() > deadline {
			let _ = child.kill();
			let _ = child.wait();
			panic!("jocasta still running after {seconds} s");
		}
		thread::sleep(Duration::from_millis(20));
	};
	Out { code: status.code(), stdout: stdout.join().expect("stdout"), stderr: stderr.join().expect("stderr") }
}

fn run(sandbox: &Sandbox, args: &[&str]) -> Out {
	finish(sandbox.jocasta(args).spawn().expect("starting jocasta"), 30)
}

#[test]
fn version_names_jocasta_omp_and_herdr() {
	let sandbox = Sandbox::new();
	let out = run(&sandbox, &["--version"]);
	assert_eq!(out.code, Some(0), "{}", out.stderr);
	// No Bun runtime in this build: the line ends with herdr's version (0625).
	assert_eq!(out.stdout.trim(), format!("jocasta {} (omp not found, herdr not found)", env!("CARGO_PKG_VERSION")));
}

#[test]
fn other_words_print_clap_usage_and_exit_2_a_word_that_is_no_command_named_first_0140_0581_0637() {
	let sandbox = Sandbox::new();
	let unknown = run(&sandbox, &["threads", "list"]);
	assert_eq!(unknown.code, Some(2));
	assert!(unknown.stdout.is_empty(), "{}", unknown.stdout);
	assert!(unknown.stderr.starts_with("error: unrecognized subcommand 'threads'"), "{}", unknown.stderr);
	assert!(unknown.stderr.contains("Usage: jocasta"), "{}", unknown.stderr);
	for args in [["mcp", "--nope"], ["setup", "now"]] {
		let out = run(&sandbox, &args);
		assert_eq!(out.code, Some(2), "{args:?}");
		assert!(out.stderr.starts_with("error: "), "{}", out.stderr);
		assert!(out.stderr.contains("Usage: jocasta"), "{}", out.stderr);
	}
}

#[test]
fn help_goes_to_stdout_and_exits_0_without_the_internal_words() {
	let sandbox = Sandbox::new();
	let out = run(&sandbox, &["--help"]);
	assert_eq!(out.code, Some(0));
	assert!(out.stderr.is_empty(), "{}", out.stderr);
	assert!(out.stdout.contains("Usage: jocasta"), "{}", out.stdout);
	for word in ["setup", "uninstall", "diagnose", "mcp", "project", "thread", "worktree", "--version"] {
		assert!(out.stdout.contains(word), "{word} missing from {}", out.stdout);
	}
	for hidden in ["push-hook", "pane-check", "plugin"] {
		assert!(!out.stdout.contains(hidden), "{hidden} in {}", out.stdout);
	}
}

#[test]
fn project_usage_and_help_print_as_they_are_and_touch_nothing() {
	let sandbox = Sandbox::new();
	for args in [&["project", "--help"][..], &["project"], &["project", "rename", "x"], &["project", "add"]] {
		let out = run(&sandbox, args);
		assert_eq!(out.code, Some(2), "{args:?}");
		assert_eq!(out.stderr.trim(), "usage: jocasta project list | add <path> | remove <name|id>");
	}
	assert!(!sandbox.path().join("data").exists());
}

#[test]
fn thread_and_worktree_bad_usage_exits_2_with_the_usage_without_the_service_exit_3_says_so_0544() {
	let sandbox = Sandbox::new();
	let bad: [&[&str]; 4] =
		[&["thread"], &["thread", "nope"], &["thread", "finish", "x", "squash"], &["worktree", "remove"]];
	for args in bad {
		let out = run(&sandbox, args);
		assert_eq!(out.code, Some(2), "{args:?}");
		// A usage prints as it is: its continuation lines line up (0581).
		assert!(out.stderr.starts_with(&format!("usage: jocasta {}", args[0])), "{}", out.stderr);
	}
	for args in [&["thread", "list"][..], &["worktree", "list"], &["thread", "stop", "x"]] {
		let out = run(&sandbox, args);
		assert_eq!(out.code, Some(3), "{args:?}");
		assert_eq!(out.stderr.trim(), format!("jocasta: {NOT_RUNNING}"));
	}
}

#[test]
fn jocasta_mcp_answers_initialize_on_stdio_and_its_tools_say_jocasta_isnt_running_without_the_service() {
	let sandbox = Sandbox::new();
	let mut command = sandbox.jocasta(&["mcp", "--boss"]);
	let mut child = command.stdin(Stdio::piped()).spawn().expect("starting jocasta mcp");
	let mut stdin = child.stdin.take().expect("jocasta mcp has no stdin");
	let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
		"protocolVersion": MCP_PROTOCOL_VERSION, "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } });
	let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
	let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
		"name": "history_search", "arguments": { "project": "demo", "query": "x" } } });
	for message in [initialize, initialized, call] {
		writeln!(stdin, "{message}").expect("writing to jocasta mcp");
	}
	drop(stdin);
	let out = finish(child, 30);
	assert_eq!(out.code, Some(0), "{}", out.stderr);
	let replies: Vec<Value> = out.stdout.trim().lines().map(|line| serde_json::from_str(line).expect("JSON")).collect();
	assert_eq!(
		replies[0],
		json!({ "jsonrpc": "2.0", "id": 1, "result": {
			"protocolVersion": MCP_PROTOCOL_VERSION,
			"capabilities": { "tools": { "listChanged": false } },
			"serverInfo": { "name": "jocasta", "version": env!("CARGO_PKG_VERSION") } } })
	);
	assert_eq!(
		replies[1],
		json!({ "jsonrpc": "2.0", "id": 2, "result": { "content": [{ "type": "text", "text": NOT_RUNNING }], "isError": true } })
	);
}

#[test]
fn jocasta_setup_ends_after_its_last_answer_while_its_console_stays_open_as_a_terminals_does() {
	let sandbox = Sandbox::new();
	// Setup reads the shipped `config/` beside `jocasta.exe`: an installed layout of its own, as the installer leaves.
	let install = sandbox.path().join("install");
	copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("config"), &install.join("config"));
	let exe = install.join(Path::new(env!("CARGO_BIN_EXE_jocasta")).file_name().expect("the binary's name"));
	std::fs::copy(env!("CARGO_BIN_EXE_jocasta"), &exe).expect("copying the binary");
	let mut command = sandbox.command(&exe, &["setup"]);
	let mut child = command.stdin(Stdio::piped()).spawn().expect("starting jocasta setup");
	// Read as it goes, so a long diff never fills the pipe.
	let stdout = drain(child.stdout.take().expect("piped stdout"));
	let _stderr = drain(child.stderr.take().expect("piped stderr"));
	let mut stdin = child.stdin.take().expect("jocasta setup has no stdin");
	stdin.write_all(b"n\n").expect("answering");
	stdin.flush().expect("answering");
	let deadline = Instant::now() + Duration::from_secs(15);
	let ended = loop {
		if let Some(status) = child.try_wait().expect("waiting for jocasta") {
			break Some(status);
		}
		if Instant::now() > deadline {
			break None;
		}
		thread::sleep(Duration::from_millis(20));
	};
	// stdin is still open here: the command mustn't wait on it.
	if ended.is_none() {
		let _ = child.kill();
		let _ = child.wait();
	}
	drop(stdin);
	assert_eq!(ended.and_then(|status| status.code()), Some(0), "still running");
	assert!(stdout.join().expect("stdout").contains("Nothing written."));
}
