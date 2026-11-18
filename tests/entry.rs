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

	/// The environment [`Sandbox::jocasta`] sets: every root in the sandbox.
	fn roots(&self) -> Vec<(&'static str, PathBuf)> {
		let root = self.path();
		vec![
			("PATH", root.join("bin")),
			("Path", root.join("bin")),
			("HOME", root.to_path_buf()),
			("USERPROFILE", root.to_path_buf()),
			("LOCALAPPDATA", root.join("local")),
			("APPDATA", root.join("roaming")),
			("MISE_CONFIG_DIR", root.join("mise")),
			("JOCASTA_DATA_DIR", root.join("data")),
			("JOCASTA_CODING_AGENT_DIR", root.join("config")),
			("PI_CODING_AGENT_DIR", root.join("omp").join("agent")),
			("PI_CONFIG_DIR", root.join("omp")),
		]
	}

	/// [`Sandbox::jocasta`] for the binary at `exe`.
	fn command(&self, exe: &Path, args: &[&str]) -> Command {
		let mut command = Command::new(exe);
		command.args(args).envs(self.roots());
		for name in UNSET {
			command.env_remove(name);
		}
		command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
		command
	}
}

/// What the sandbox's `jocasta` never inherits: a live check's session, a pane's key, the calendar's secret, and,
/// inside one of herdr's panes, what would point the fork at that herdr.
const UNSET: [&str; 10] = [
	"JOCASTA_HERDR_SESSION",
	"JOCASTA_PANE_KEY",
	"JOCASTA_CALENDAR_ICAL_SECRET",
	"JOCASTA_SERVICE",
	"HERDR_SOCKET_PATH",
	"HERDR_CLIENT_SOCKET_PATH",
	"HERDR_SESSION",
	"HERDR_ENV",
	"HERDR_CONFIG_PATH",
	"HERDR_PANE_ID",
];

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
	// herdr is built in: its version is the fork's, whatever is installed.
	assert_eq!(
		out.stdout.trim(),
		format!("jocasta {} (omp not found, herdr {})", env!("CARGO_PKG_VERSION"), herdr::embed::version())
	);
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

/// [`Sandbox::jocasta`] in herdr session `name`, as bare `jocasta` would run it (`JOCASTA_HERDR_SESSION`).
fn in_session(sandbox: &Sandbox, name: &str, args: &[&str]) -> Command {
	let mut command = sandbox.jocasta(&[&["--session", name][..], args].concat());
	command.env("JOCASTA_HERDR_SESSION", name);
	command
}

/// Waits up to `seconds` for `done`; whether it came.
fn waited(seconds: u64, mut done: impl FnMut() -> bool) -> bool {
	let deadline = Instant::now() + Duration::from_secs(seconds);
	while !done() {
		if Instant::now() >= deadline {
			return false;
		}
		thread::sleep(Duration::from_millis(200));
	}
	true
}

/// Waits up to `seconds` for `done`, failing with `what`.
fn wait_until(seconds: u64, what: &str, done: impl FnMut() -> bool) {
	assert!(waited(seconds, done), "{what}");
}

/// Whether session `name`'s herdr lists a workspace labelled "Jocasta".
fn has_jocastas_workspace(sandbox: &Sandbox, name: &str) -> bool {
	let out = finish(in_session(sandbox, name, &["workspace", "list"]).spawn().expect("workspace list"), 30);
	out.stdout.contains("\"Jocasta\"")
}

/// A stand-in omp on the sandbox's PATH, built with the toolchain's own rustc (Windows runs a bare `omp` only as an
/// .exe): it answers the service's relay check (`config get collab.relayUrl --json`) with a loopback relay, and
/// otherwise waits, as the boss pane's omp would.
fn fake_omp(sandbox: &Sandbox) {
	let source = sandbox.path().join("fake_omp.rs");
	std::fs::write(
		&source,
		r#"fn main() {
	if std::env::args().nth(1).as_deref() == Some("config") {
		println!("{{\"value\":\"ws://127.0.0.1:1\"}}");
		return;
	}
	std::thread::sleep(std::time::Duration::from_secs(3600));
}
"#,
	)
	.expect("the fake omp's source");
	let rustc = Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
	let omp = sandbox.path().join("bin").join(format!("omp{}", std::env::consts::EXE_SUFFIX));
	let built = Command::new(rustc)
		.args(["--edition", "2024", "-o"])
		.arg(&omp)
		.arg(&source)
		.current_dir(sandbox.path())
		.output()
		.expect("rustc");
	assert!(built.status.success(), "the fake omp didn't build: {}", String::from_utf8_lossy(&built.stderr));
}

/// Bare `jocasta` in a terminal of its own (120×40), as he runs it; its output is kept, so herdr's client never
/// blocks on it, and its tail names a failure.
struct Terminal {
	client: Box<dyn portable_pty::Child + Send + Sync>,
	keys: Box<dyn Write + Send>,
	shown: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
	_master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Terminal {
	fn bare(sandbox: &Sandbox, session: &str) -> Self {
		use portable_pty::{CommandBuilder, PtySize, native_pty_system};

		let pair = native_pty_system()
			.openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })
			.expect("a terminal");
		let mut bare = CommandBuilder::new(env!("CARGO_BIN_EXE_jocasta"));
		bare.cwd(sandbox.path());
		for (name, value) in sandbox.roots() {
			bare.env(name, value);
		}
		for name in UNSET {
			bare.env_remove(name);
		}
		bare.env("JOCASTA_HERDR_SESSION", session);
		bare.env("TERM", "xterm-256color");
		let client = pair.slave.spawn_command(bare).expect("starting bare jocasta");
		drop(pair.slave);
		let mut screen = pair.master.try_clone_reader().expect("the terminal's output");
		let shown = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
		let kept = std::sync::Arc::clone(&shown);
		thread::spawn(move || {
			let mut buffer = [0_u8; 8192];
			while let Ok(read @ 1..) = screen.read(&mut buffer) {
				kept.lock().expect("the screen").extend_from_slice(&buffer[..read]);
			}
		});
		let mut terminal = Self {
			client,
			keys: pair.master.take_writer().expect("the terminal's input"),
			shown,
			_master: pair.master,
		};
		// Windows' pseudo console asks the terminal where its cursor is before any output flows; a terminal answers.
		if cfg!(windows) {
			wait_until(30, "the console never asked for its cursor", || terminal.has(b"\x1b[6n"));
			terminal.keys.write_all(b"\x1b[1;1R").expect("the cursor's answer");
		}
		terminal
	}

	fn has(&self, bytes: &[u8]) -> bool {
		self.shown.lock().expect("the screen").windows(bytes.len()).any(|window| window == bytes)
	}

	/// What a failure shows: the terminal's tail, whether the client still runs, and the sandbox's logs.
	fn report(&mut self, sandbox: &Sandbox, session: &str) -> String {
		let screen = {
			let shown = self.shown.lock().expect("the screen");
			String::from_utf8_lossy(&shown[shown.len().saturating_sub(2_000)..]).into_owned()
		};
		let data = sandbox.path().join("data");
		let logs: Vec<String> = [data.join("logs"), data.join("herdr").join("sessions").join(session)]
			.iter()
			.flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
			.filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
			.map(|entry| {
				format!("{}:\n{}", entry.path().display(), std::fs::read_to_string(entry.path()).unwrap_or_default())
			})
			.collect();
		format!("client {:?}; the terminal: {screen:?}\n{}", self.client.try_wait(), logs.join("\n"))
	}
}

/// One test, as the service's collab relay has one fixed port (0561): two services at once would refuse the second.
#[test]
fn bare_jocasta_runs_the_service_in_herdrs_server_one_that_cant_start_ends_it_saying_why_closing_his_terminal_closes_jocasta_a_plain_server_runs_none_0549_0582()
 {
	let sandbox = Sandbox::new();
	let service_file = sandbox.path().join("data").join("jocasta-service.json");

	// No omp on PATH: the service can't start, so herdr's server stops, and bare `jocasta` says why and exits 1.
	let mut first = Terminal::bare(&sandbox, "f");
	let ended = waited(90, || matches!(first.client.try_wait(), Ok(Some(_))));
	assert!(ended, "herdr's client outlived a service that didn't start; {}", first.report(&sandbox, "f"));
	let code = first.client.wait().expect("the client's exit").exit_code();
	assert_eq!(code, 1, "{}", first.report(&sandbox, "f"));
	let said = waited(10, || first.has(b"Jocasta stopped while starting"));
	assert!(said, "{}", first.report(&sandbox, "f"));
	assert!(!service_file.exists(), "a service file outlived a failed start");
	wait_until(30, "herdr's server outlived the failed start", || {
		finish(in_session(&sandbox, "f", &["pane", "list"]).spawn().expect("pane list"), 30).code == Some(1)
	});
	drop(first);

	fake_omp(&sandbox);
	let mut terminal = Terminal::bare(&sandbox, "p");
	assert!(waited(90, || service_file.exists()), "the service never listened; {}", terminal.report(&sandbox, "p"));
	wait_until(90, "the service never made workspace Jocasta", || has_jocastas_workspace(&sandbox, "p"));
	assert!(matches!(terminal.client.try_wait(), Ok(None)), "herdr's client ended");
	// A plain server of another session runs no service: it answers, and the service file still names the first.
	let named = std::fs::read_to_string(&service_file).expect("the service file");
	let plain = HerdrServer::start(&sandbox);
	assert!(!has_jocastas_workspace(&sandbox, "t"));
	drop(plain);
	assert_eq!(std::fs::read_to_string(&service_file).expect("the service file"), named);

	// His terminal closes: herdr's server sees its last client go, and with nothing at work Jocasta stops as one.
	terminal.client.kill().expect("closing the terminal");
	drop(terminal);
	wait_until(90, "Jocasta never closed", || !service_file.exists());
	wait_until(30, "herdr's server outlived Jocasta", || {
		finish(in_session(&sandbox, "p", &["pane", "list"]).spawn().expect("pane list"), 30).code == Some(1)
	});

	// A server started headless with `JOCASTA_SERVICE=1` runs it too, and `server stop` ends both.
	let mut server = in_session(&sandbox, "s", &["server"])
		.env("JOCASTA_SERVICE", "1")
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.expect("starting jocasta server");
	wait_until(90, "the service never listened", || service_file.exists());
	wait_until(90, "the service never made workspace Jocasta", || has_jocastas_workspace(&sandbox, "s"));
	let stop = finish(in_session(&sandbox, "s", &["server", "stop"]).spawn().expect("server stop"), 30);
	assert_eq!(stop.code, Some(0), "{}{}", stop.stdout, stop.stderr);
	wait_until(90, "the server process never ended", || matches!(server.try_wait(), Ok(Some(_))));
	assert!(!service_file.exists(), "the service's file outlived it");
}
