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

/// A folder of the test's own, removed when it ends. Under `/tmp` on Linux and macOS, with a short name: herdr's
/// session sockets live in it, and a socket's path must stay under 104 bytes (macOS's temp dir alone is about 50).
struct Sandbox(PathBuf);

impl Sandbox {
	fn new() -> Self {
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let base = if cfg!(unix) { PathBuf::from("/tmp") } else { std::env::temp_dir() };
		let dir = base.join(format!("je-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst)));
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
			// Inside one of herdr's panes these would point the fork at that herdr.
			.env_remove("HERDR_SOCKET_PATH")
			.env_remove("HERDR_CLIENT_SOCKET_PATH")
			.env_remove("HERDR_SESSION")
			.env_remove("HERDR_ENV")
			.env_remove("HERDR_CONFIG_PATH")
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
	for hidden in ["push-hook", "pane-check", "\n  plugin "] {
		assert!(!out.stdout.contains(hidden), "{hidden} in {}", out.stdout);
	}
	// herdr's commands, built in, are named after Jocasta's own.
	assert!(out.stdout.contains("herdr's commands, built in"), "{}", out.stdout);
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
	for args in [&["thread", "list"][..], &["worktree", "leftover"], &["thread", "stop", "x"]] {
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

/// [`Sandbox::jocasta`] in herdr session `t` with the machine's PATH: a server's panes and plugins start shells.
fn herdr_t(sandbox: &Sandbox, args: &[&str]) -> Command {
	let mut command = sandbox.jocasta(&[&["--session", "t"][..], args].concat());
	if let Some(path) = std::env::var_os("PATH") {
		command.env("PATH", &path).env("Path", path);
	}
	command
}

/// A herdr server in session `t`, stopped (and waited for) when dropped.
struct HerdrServer<'a> {
	sandbox: &'a Sandbox,
	child: Option<Child>,
}

impl<'a> HerdrServer<'a> {
	/// Starts `jocasta --session t server` and waits until `pane list` answers.
	fn start(sandbox: &'a Sandbox) -> Self {
		let child = herdr_t(sandbox, &["server"])
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.expect("starting jocasta server");
		let server = Self { sandbox, child: Some(child) };
		let deadline = Instant::now() + Duration::from_secs(30);
		while finish(herdr_t(sandbox, &["pane", "list"]).spawn().expect("jocasta pane list"), 30).code != Some(0) {
			assert!(Instant::now() < deadline, "the server never answered");
			thread::sleep(Duration::from_millis(100));
		}
		server
	}
}

impl Drop for HerdrServer<'_> {
	/// Stops the server and waits up to 30 s for it, killing it after; never panics (it may run while a test unwinds).
	fn drop(&mut self) {
		if let Ok(stop) = herdr_t(self.sandbox, &["server", "stop"]).spawn() {
			let _ = finish(stop, 30);
		}
		if let Some(mut child) = self.child.take() {
			let deadline = Instant::now() + Duration::from_secs(30);
			while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
				thread::sleep(Duration::from_millis(50));
			}
			let _ = child.kill();
			let _ = child.wait();
		}
	}
}

#[test]
fn a_herdr_word_without_its_server_prints_herdrs_server_not_running_and_exits_1() {
	let sandbox = Sandbox::new();
	let out = run(&sandbox, &["pane", "list"]);
	assert_eq!(out.code, Some(1), "{}{}", out.stdout, out.stderr);
	assert!(format!("{}{}", out.stdout, out.stderr).contains("server_not_running"), "{}{}", out.stdout, out.stderr);
	// herdr's files live in Jocasta's data dir, not herdr's own.
	assert!(!sandbox.path().join(".config").join("herdr").exists());
}

#[test]
fn herdrs_help_is_herdrs_and_its_version_flag_names_herdrs_version() {
	let sandbox = Sandbox::new();
	let help = run(&sandbox, &["workspace", "--help"]);
	assert_eq!(help.code, Some(0), "{}", help.stderr);
	assert!(help.stdout.contains("workspace"), "{}", help.stdout);
	assert!(!help.stdout.contains("Usage: jocasta [COMMAND]"), "{}", help.stdout);
	let version = run(&sandbox, &["-V"]);
	assert_eq!(version.code, Some(0), "{}", version.stderr);
	assert!(version.stdout.contains(&herdr::embed::version()), "{}", version.stdout);
}

#[test]
fn a_herdr_server_in_a_named_session_answers_jocastas_pane_list_and_its_plugins_reach_it_through_herdr_bin_path() {
	let sandbox = Sandbox::new();
	let server = HerdrServer::start(&sandbox);
	let panes = finish(herdr_t(&sandbox, &["pane", "list"]).spawn().expect("jocasta pane list"), 30);
	let answer: Value = serde_json::from_str(&panes.stdout).expect("pane list's JSON");
	assert!(answer.get("error").is_none(), "{answer}");

	// A plugin whose action runs `$HERDR_BIN_PATH pane list`: herdr gives it the running `jocasta`.
	let plugin = sandbox.path().join("plugin");
	std::fs::create_dir_all(&plugin).expect("making the plugin");
	let command: Vec<String> = if cfg!(windows) {
		let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into());
		vec![shell, "/d".into(), "/c".into(), "%HERDR_BIN_PATH% pane list > out.json".into()]
	} else {
		vec!["/bin/sh".into(), "-c".into(), "\"$HERDR_BIN_PATH\" pane list > out.json".into()]
	};
	let manifest = format!(
		"id = \"example.entry\"\nname = \"Entry\"\nversion = \"0.1.0\"\nmin_herdr_version = \"0.6.10\"\n\
		 platforms = [\"linux\", \"macos\", \"windows\"]\n\n[[actions]]\nid = \"panes\"\ntitle = \"Panes\"\ncommand = {}\n",
		serde_json::to_string(&command).expect("the command as TOML")
	);
	std::fs::write(plugin.join("herdr-plugin.toml"), manifest).expect("writing the manifest");
	let plugin_arg = plugin.to_string_lossy().into_owned();
	let link = finish(herdr_t(&sandbox, &["plugin", "link", &plugin_arg]).spawn().expect("plugin link"), 30);
	assert_eq!(link.code, Some(0), "{}{}", link.stdout, link.stderr);
	let invoke = finish(
		herdr_t(&sandbox, &["plugin", "action", "invoke", "panes", "--plugin", "example.entry"])
			.spawn()
			.expect("plugin action invoke"),
		30,
	);
	assert_eq!(invoke.code, Some(0), "{}{}", invoke.stdout, invoke.stderr);
	let deadline = Instant::now() + Duration::from_secs(30);
	let seen = loop {
		let text = std::fs::read_to_string(plugin.join("out.json")).unwrap_or_default();
		if let Ok(seen) = serde_json::from_str::<Value>(&text) {
			break seen;
		}
		assert!(Instant::now() < deadline, "the action never wrote pane list's answer: {text:?}");
		thread::sleep(Duration::from_millis(100));
	};
	assert_eq!(seen, answer);
	drop(server);
}
