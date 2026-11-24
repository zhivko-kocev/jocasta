//! `test/entry.test.ts` and the command's part of `test/shared/cli.test.ts`, as black-box tests of the built
//! `jocasta` binary: each one starts it with roots of its own, never his real folders, and no omp, herdr or git on its
//! PATH (0637 gives the usage-error texts clap's format); the PTY test alone adds git's own folder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

/// The message the thread and worktree commands give when no service runs (`NOT_RUNNING`).
const NOT_RUNNING: &str = "Jocasta isn't running; start it with jocasta.";

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
	for args in [["uninstall", "--nope"], ["setup", "now"]] {
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
	for word in ["setup", "uninstall", "diagnose", "project", "thread", "worktree", "--version"] {
		assert!(out.stdout.contains(word), "{word} missing from {}", out.stdout);
	}
	for hidden in ["push-hook", "\n  plugin "] {
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
/// .exe): it answers the service's relay check (`config get collab.relayUrl --json`) with a loopback relay and its spend
/// step (`usage --json`) with no reports, and otherwise waits, as the boss pane's omp would. The spend step's run isn't
/// ended with the service, so a waiting answer would outlive the test.
fn fake_omp(sandbox: &Sandbox) {
	let source = sandbox.path().join("fake_omp.rs");
	std::fs::write(
		&source,
		r#"fn main() {
	match std::env::args().nth(1).as_deref() {
		Some("config") => println!("{{\"value\":\"ws://127.0.0.1:1\"}}"),
		Some("usage") => println!("{{\"generatedAt\":0,\"reports\":[]}}"),
		_ => std::thread::sleep(std::time::Duration::from_secs(3600)),
	}
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

/// The folder of the git on the test's own PATH, so the sandbox's service finds git and nothing else of the machine's.
fn git_folder() -> PathBuf {
	let name = format!("git{}", std::env::consts::EXE_SUFFIX);
	std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
		.find(|dir| dir.join(&name).is_file())
		.expect("git on the test's PATH")
}

/// Runs git in `dir` with an identity of its own.
fn git(dir: &Path, args: &[&str]) {
	let out = Command::new("git")
		.args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
		.args(args)
		.current_dir(dir)
		.output()
		.expect("git");
	assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Repository `code/demo` with one commit and a clean worktree `code/demo.wt/left` no thread holds; its path.
fn leftover_worktree(sandbox: &Sandbox) -> PathBuf {
	let main = sandbox.path().join("code").join("demo");
	std::fs::create_dir_all(&main).expect("the repository's folder");
	std::fs::write(main.join("README.md"), "demo\n").expect("a README");
	git(&main, &["init", "-q"]);
	git(&main, &["add", "."]);
	git(&main, &["commit", "-qm", "First"]);
	let left = sandbox.path().join("code").join("demo.wt").join("left");
	git(&main, &["worktree", "add", "-q", "-b", "left", &left.display().to_string()]);
	left
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
	/// Bare `jocasta` in session `session`; `also_on_path` joins the sandbox's PATH after its own `bin`.
	fn bare(sandbox: &Sandbox, session: &str, also_on_path: Option<&Path>) -> Self {
		use portable_pty::{CommandBuilder, PtySize, native_pty_system};

		let pair = native_pty_system()
			.openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })
			.expect("a terminal");
		let mut bare = CommandBuilder::new(env!("CARGO_BIN_EXE_jocasta"));
		bare.cwd(sandbox.path());
		for (name, value) in sandbox.roots() {
			bare.env(name, value);
		}
		if let Some(also) = also_on_path {
			let path = std::env::join_paths([sandbox.path().join("bin").as_path(), also]).expect("a PATH");
			bare.env("PATH", &path);
			bare.env("Path", &path);
		}
		for name in UNSET {
			bare.env_remove(name);
		}
		bare.env("JOCASTA_HERDR_SESSION", session);
		bare.env("TERM", "xterm-256color");
		// herdr's default workspace closes only while its shell shows one prompt line (0495): a plain `sh`, not a bash
		// that greets with a sudo hint (Ubuntu) or a zsh notice (macOS).
		if cfg!(unix) {
			bare.env("SHELL", "/bin/sh");
			bare.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
		}
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

	/// The terminal's 40 rows of 120 cells as drawn so far: text written at the cursor, which `CSI row;col H`, `CSI n C`
	/// and CR/LF move (herdr's client skips unchanged cells with `CSI n C`); other sequences are skipped.
	fn screen(&self) -> Vec<String> {
		let shown = self.shown.lock().expect("the screen");
		let text = String::from_utf8_lossy(&shown);
		let mut cells = vec![vec![' '; 120]; 40];
		let (mut row, mut col) = (0_usize, 0_usize);
		let mut chars = text.chars();
		while let Some(c) = chars.next() {
			match c {
				'\x1b' => match chars.next() {
					Some('[') => {
						let mut params = String::new();
						let mut last = None;
						for p in chars.by_ref() {
							if ('\x40'..='\x7e').contains(&p) {
								last = Some(p);
								break;
							}
							params.push(p);
						}
						let numbers: Vec<usize> = params.split(';').map(|n| n.parse().unwrap_or(1)).collect();
						let first = numbers.first().copied().unwrap_or(1).max(1);
						match last {
							Some('H') => {
								row = first - 1;
								col = numbers.get(1).copied().unwrap_or(1).max(1) - 1;
							}
							Some('C') => col += first,
							_ => {}
						}
					}
					// An OSC runs to BEL or ST.
					Some(']') => while chars.next().is_some_and(|p| p != '\x07' && p != '\x1b') {},
					_ => {}
				},
				'\r' => col = 0,
				'\n' => row += 1,
				c if !c.is_control() => {
					if let Some(cell) = cells.get_mut(row).and_then(|line| line.get_mut(col)) {
						*cell = c;
					}
					col += 1;
				}
				_ => {}
			}
		}
		cells.into_iter().map(|line| line.into_iter().collect()).collect()
	}

	/// The three capital letters drawn right after "Type " on screen: a question's code.
	fn code(&self) -> Option<String> {
		self.screen().iter().find_map(|line| {
			let after = &line[line.find("Type ")? + "Type ".len()..];
			let code: String = after.chars().take_while(char::is_ascii_uppercase).collect();
			(code.len() == 3).then_some(code)
		})
	}

	/// Types `text` into the terminal.
	fn type_text(&mut self, text: &str) {
		self.keys.write_all(text.as_bytes()).expect("typing");
		self.keys.flush().expect("typing");
	}

	/// What a failure shows: the terminal's tail, whether the client still runs, and the sandbox's logs.
	fn report(&mut self, sandbox: &Sandbox, session: &str) -> String {
		let screen = {
			let shown = self.shown.lock().expect("the screen");
			String::from_utf8_lossy(&shown[shown.len().saturating_sub(2_000)..]).into_owned()
		};
		let drawn = self.screen().join("\n");
		format!(
			"client {:?}; the screen:\n{drawn}\nthe terminal: {screen:?}\n{}",
			self.client.try_wait(),
			logs(sandbox, session)
		)
	}
}

/// The sandbox's Jocasta logs and herdr session `session`'s, for a failure's message.
fn logs(sandbox: &Sandbox, session: &str) -> String {
	let data = sandbox.path().join("data");
	let logs: Vec<String> = [data.join("logs"), data.join("herdr").join("sessions").join(session)]
		.iter()
		.flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
		.filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
		.map(|entry| {
			format!("{}:\n{}", entry.path().display(), std::fs::read_to_string(entry.path()).unwrap_or_default())
		})
		.collect();
	logs.join("\n")
}

/// One test: the failed start and the working one share a sandbox and its herdr sessions.
#[test]
fn bare_jocasta_runs_the_service_in_herdrs_server_one_that_cant_start_ends_it_saying_why_closing_his_terminal_closes_jocasta_a_plain_server_runs_none_0549_0582()
 {
	let sandbox = Sandbox::new();
	let service_file = sandbox.path().join("data").join("jocasta-service.json");

	// No omp on PATH: the service can't start, so herdr's server stops, and bare `jocasta` says why and exits 1.
	// The boss's home can't be written (a file holds the name of its `.omp` folder), so the boss, and with it the service,
	// can't start.
	let boss_home = sandbox.path().join("data").join("home");
	std::fs::create_dir_all(&boss_home).expect("the boss's home");
	std::fs::write(boss_home.join(".omp"), "").expect("the blocker");
	let mut first = Terminal::bare(&sandbox, "f", None);
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
	std::fs::remove_file(boss_home.join(".omp")).expect("the blocker goes");

	fake_omp(&sandbox);
	// A project with a leftover worktree, for an Allow; the service runs git from git's own folder.
	let git_dir = git_folder();
	let leftover = leftover_worktree(&sandbox);
	let added = finish(
		sandbox
			.jocasta(&["project", "add", &sandbox.path().join("code").join("demo").display().to_string()])
			.env("PATH", std::env::join_paths([sandbox.path().join("bin").as_path(), &git_dir]).expect("a PATH"))
			.spawn()
			.expect("project add"),
		30,
	);
	assert_eq!(added.code, Some(0), "{}{}", added.stdout, added.stderr);
	let mut terminal = Terminal::bare(&sandbox, "p", Some(&git_dir));
	assert!(waited(90, || service_file.exists()), "the service never listened; {}", terminal.report(&sandbox, "p"));
	wait_until(90, "the service never made workspace Jocasta", || has_jocastas_workspace(&sandbox, "p"));
	assert!(matches!(terminal.client.try_wait(), Ok(None)), "herdr's client ended");
	// herdr's client draws the dock in the boss's workspace.
	let docked = waited(60, || terminal.screen().iter().any(|line| line.contains("NEEDS YOU")));
	assert!(docked, "the dock never showed; {}", terminal.report(&sandbox, "p"));
	// `jocasta worktree remove` waits for his Allow; the question is drawn in his terminal, its code read off it.
	let mut remove =
		sandbox.jocasta(&["worktree", "remove", &leftover.display().to_string()]).spawn().expect("worktree remove");
	let asked = waited(60, || terminal.code().is_some() || matches!(remove.try_wait(), Ok(Some(_))));
	if matches!(remove.try_wait(), Ok(Some(_))) {
		let out = finish(remove, 5);
		panic!("worktree remove ended unasked: {}{}; {}", out.stdout, out.stderr, terminal.report(&sandbox, "p"));
	}
	assert!(asked, "no question with a code showed; {}", terminal.report(&sandbox, "p"));
	let code = terminal.code().expect("the code");
	// The question ignores keys for its first 750 ms (0578).
	thread::sleep(Duration::from_millis(1_500));
	terminal.type_text(&code.to_lowercase());
	let answered = waited(60, || matches!(remove.try_wait(), Ok(Some(_))));
	assert!(answered, "worktree remove still waits after the code {code}; {}", terminal.report(&sandbox, "p"));
	let removed = finish(remove, 5);
	assert_eq!(removed.code, Some(0), "{}{}; {}", removed.stdout, removed.stderr, terminal.report(&sandbox, "p"));
	assert!(!leftover.exists(), "the leftover worktree stayed after his Allow");
	// A plain server of another session runs no service: it answers, and the service file still names the first.
	let named = std::fs::read_to_string(&service_file)
		.unwrap_or_else(|error| panic!("the service file: {error}; {}", terminal.report(&sandbox, "p")));
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

/// The value at `pointer` in herdr's answer, with or without its `result` envelope.
fn answered<'a>(answer: &'a Value, pointer: &str) -> Option<&'a str> {
	answer.pointer(&format!("/result{pointer}")).or_else(|| answer.pointer(pointer)).and_then(Value::as_str)
}

/// A served session with a project whose worktree runs a stand-in omp, found as a thread: the server, the omp's pane
/// and its workspace. The relay's fixed port allows one service at a time, so `mise run smoke` runs one test at once.
struct Smoke<'a> {
	sandbox: &'a Sandbox,
	session: &'static str,
	server: Ends,
	pane: String,
	workspace: String,
}

impl<'a> Smoke<'a> {
	fn start(sandbox: &'a Sandbox, session: &'static str) -> Self {
		let service_file = sandbox.path().join("data").join("jocasta-service.json");
		fake_omp(sandbox);
		let git_dir = git_folder();
		let path = std::env::join_paths([sandbox.path().join("bin").as_path(), &git_dir]).expect("a PATH");
		let worktree = leftover_worktree(sandbox);
		let main = sandbox.path().join("code").join("demo");
		let added = finish(
			sandbox
				.jocasta(&["project", "add", &main.display().to_string()])
				.env("PATH", &path)
				.spawn()
				.expect("project add"),
			30,
		);
		assert_eq!(added.code, Some(0), "{}{}", added.stdout, added.stderr);
		let server = in_session(sandbox, session, &["server"])
			.env("JOCASTA_SERVICE", "1")
			.env("PATH", &path)
			.env("Path", &path)
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.expect("starting jocasta server");
		let server = Ends(server);
		wait_until(90, "the service never listened", || service_file.exists());
		wait_until(90, "the service never made workspace Jocasta", || has_jocastas_workspace(sandbox, session));
		let mut smoke = Self { sandbox, session, server, pane: String::new(), workspace: String::new() };
		assert_eq!(smoke.threads().trim(), "No threads.");

		let cwd = worktree.display().to_string();
		let made = smoke.herdr(&["workspace", "create", "--cwd", &cwd, "--label", "left"]);
		let made: Value = serde_json::from_str(&made.stdout)
			.unwrap_or_else(|_| panic!("herdr's answer: {}{}", made.stdout, made.stderr));
		smoke.pane =
			answered(&made, "/root_pane/pane_id").unwrap_or_else(|| panic!("no root pane in {made}")).to_owned();
		smoke.workspace =
			answered(&made, "/workspace/workspace_id").unwrap_or_else(|| panic!("no workspace in {made}")).to_owned();
		let omp = sandbox.path().join("bin").join(format!("omp{}", std::env::consts::EXE_SUFFIX)).display().to_string();
		// `pane run` types the command into the pane's shell (PowerShell on Windows), and the sandbox path may hold spaces.
		let omp = if cfg!(windows) { format!("& '{omp}'") } else { format!("'{omp}'") };
		let pane = smoke.pane.clone();
		let ran = smoke.herdr(&["pane", "run", &pane, &omp]);
		assert_eq!(ran.code, Some(0), "{}{}", ran.stdout, ran.stderr);
		let found = waited(20, || smoke.thread().is_some());
		assert!(found, "the omp pane in the worktree never became a thread: {}", smoke.report());
		smoke
	}

	/// A herdr word in the smoke's session.
	fn herdr(&self, args: &[&str]) -> Out {
		finish(in_session(self.sandbox, self.session, args).spawn().expect("a herdr word"), 30)
	}

	fn threads(&self) -> String {
		finish(self.sandbox.jocasta(&["thread", "list"]).spawn().expect("thread list"), 30).stdout
	}

	/// The first thread's name in a `thread list` that answered (exit 0), or `None` while it lists none or doesn't
	/// answer; the rest of its line (time, spend) changes from one call to the next.
	fn thread(&self) -> Option<String> {
		let listed = finish(self.sandbox.jocasta(&["thread", "list"]).spawn().expect("thread list"), 30);
		let name = listed.stdout.split_whitespace().next()?;
		(listed.code == Some(0) && listed.stdout.trim() != "No threads.").then(|| name.to_owned())
	}

	fn report(&self) -> String {
		let panes = self.herdr(&["pane", "list"]).stdout;
		format!("{}\nherdr's panes: {panes}\n{}", self.threads(), logs(self.sandbox, self.session))
	}

	/// `server stop`; the server process it started has ended (at once after a handoff), and so has the service.
	fn stop(&mut self) {
		let stop = self.herdr(&["server", "stop"]);
		assert_eq!(stop.code, Some(0), "{}{}", stop.stdout, stop.stderr);
		let service_file = self.sandbox.path().join("data").join("jocasta-service.json");
		let server = &mut self.server.0;
		wait_until(90, "the server never ended", || matches!(server.try_wait(), Ok(Some(_))) && !service_file.exists());
	}
}

/// The opt-in smoke on a real server (`mise run smoke`, `JOCASTA_HERDR_SMOKE=1`): herdr's events reach the service, so
/// an omp started in a project's worktree is a thread within seconds and its closed workspace drops it, long before the
/// minute's full refresh (fork plan finding 8).
#[test]
fn live_smoke_an_omp_pane_in_a_worktree_is_found_and_dropped_on_herdrs_events() {
	if std::env::var("JOCASTA_HERDR_SMOKE").as_deref() != Ok("1") {
		return;
	}
	let sandbox = Sandbox::new();
	let mut smoke = Smoke::start(&sandbox, "l");
	let workspace = smoke.workspace.clone();
	let closed = smoke.herdr(&["workspace", "close", &workspace]);
	assert_eq!(closed.code, Some(0), "{}{}", closed.stdout, closed.stderr);
	let dropped = waited(20, || smoke.threads().trim() == "No threads.");
	assert!(dropped, "the closed thread stayed: {}", smoke.report());
	smoke.stop();
}

/// The opt-in smoke of an upgrade on Linux and macOS (0649): herdr's live handoff moves the panes to a new server
/// process, the old one ends, and the service starts again in the new one and finds the same thread there, its omp
/// never stopped.
#[cfg(unix)]
#[test]
fn live_smoke_a_live_handoff_keeps_a_threads_pane_and_the_service_starts_again_in_the_new_server() {
	if std::env::var("JOCASTA_HERDR_SMOKE").as_deref() != Ok("1") {
		return;
	}
	let sandbox = Sandbox::new();
	let service_file = sandbox.path().join("data").join("jocasta-service.json");
	let mut smoke = Smoke::start(&sandbox, "h");
	let thread = smoke.thread().expect("the thread before the handoff");
	let before = std::fs::read_to_string(&service_file).expect("the service file");
	let handoff = smoke.herdr(&["server", "live-handoff"]);
	assert_eq!(handoff.code, Some(0), "{}{}; {}", handoff.stdout, handoff.stderr, smoke.report());
	assert!(handoff.stderr.contains("live handoff complete"), "{}", handoff.stderr);
	let server = &mut smoke.server.0;
	// The old server ends once its service has (its stop waits on herdr calls the handoff's loop answers late).
	wait_until(90, "the old server outlived its handoff", || matches!(server.try_wait(), Ok(Some(_))));
	let restarted = waited(90, || std::fs::read_to_string(&service_file).is_ok_and(|now| now != before));
	assert!(restarted, "the service never started in the new server: {}", smoke.report());
	let panes = smoke.herdr(&["pane", "list"]).stdout;
	assert!(panes.contains(&smoke.pane), "the thread's pane didn't survive the handoff: {}", smoke.report());
	let found = waited(60, || smoke.thread().as_ref() == Some(&thread));
	assert!(found, "the new service never found thread {thread}: {}", smoke.report());
	smoke.stop();
}

/// A child process killed when the test ends, on a failure too.
struct Ends(Child);

impl Drop for Ends {
	fn drop(&mut self) {
		let _ = self.0.kill();
	}
}
