//! `test/entry.test.ts` and the command's part of `test/shared/cli.test.ts`, as black-box tests of the built
//! `jocasta` binary: each one starts it with roots of its own, never his real folders, and no omp or git on its
//! PATH (0637 gives the usage-error texts clap's format); the PTY test alone adds git's own folder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The message the thread and worktree commands give when no service runs (`NOT_RUNNING`).
const NOT_RUNNING: &str = "Jocasta isn't running; start it with jocasta.";

/// A folder of the test's own, removed when it ends. Under `/tmp` on Linux and macOS, with a short name: the bridge's
/// socket lives under the data folder, and a socket's path must stay under 104 bytes (macOS's temp dir alone is about 50).
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

	/// `jocasta` with every root in the sandbox, and a PATH holding nothing: omp, mise and git are never found, as
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

/// What the sandbox's `jocasta` never inherits: a pane's key and the calendar's secret.
const UNSET: [&str; 2] = ["JOCASTA_PANE_KEY", "JOCASTA_CALENDAR_ICAL_SECRET"];

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
fn version_names_jocasta_omp() {
	let sandbox = Sandbox::new();
	let out = run(&sandbox, &["--version"]);
	assert_eq!(out.code, Some(0), "{}", out.stderr);
	assert_eq!(out.stdout.trim(), format!("jocasta {} (omp not found)", env!("CARGO_PKG_VERSION")));
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

/// Bare `jocasta` in a terminal of its own (120×40), as he runs it; its output is kept, so the shell never
/// blocks on it, and its tail names a failure.
struct Terminal {
	client: Box<dyn portable_pty::Child + Send + Sync>,
	keys: Box<dyn Write + Send>,
	shown: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
	_master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Terminal {
	/// Bare `jocasta`; `also_on_path` joins the sandbox's PATH after its own `bin`.
	fn bare(sandbox: &Sandbox, also_on_path: Option<&Path>) -> Self {
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

	/// His window closes: the pseudo console goes, as a terminal's does, and Jocasta ends on its own, the service and
	/// every pane's process (`roots`) with it. A kill would leave it no chance to clean up. On Linux and macOS the screen
	/// reader's copy of the terminal stays open while its read blocks, so the hang-up the kernel sends when a window's
	/// terminal closes is sent here.
	fn close(self, sandbox: &Sandbox, roots: &[u32]) {
		let Self { mut client, keys, _master, .. } = self;
		drop(keys);
		drop(_master);
		#[cfg(unix)]
		if let Some(pid) = client.process_id() {
			let hung_up = std::process::Command::new("kill").args(["-HUP", &pid.to_string()]).status();
			assert!(hung_up.is_ok_and(|status| status.success()), "the hang-up wasn't sent");
		}
		let service_file = sandbox.path().join("data").join("jocasta-service.json");
		wait_until(90, "Jocasta never closed", || !service_file.exists());
		wait_until(30, "Jocasta's process outlived its window", || matches!(client.try_wait(), Ok(Some(_))));
		for root in roots {
			wait_until(30, &format!("pane process {root} outlived Jocasta"), || {
				!jocasta::shared::proctree::pid_alive(*root)
			});
		}
	}

	fn has(&self, bytes: &[u8]) -> bool {
		self.shown.lock().expect("the screen").windows(bytes.len()).any(|window| window == bytes)
	}

	/// The terminal's 40 rows of 120 cells as drawn so far: text written at the cursor, which `CSI row;col H`, `CSI n C`
	/// and CR/LF move (the shell's redraw skips unchanged cells with `CSI n C`); other sequences are skipped.
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
	fn report(&mut self, sandbox: &Sandbox) -> String {
		let screen = {
			let shown = self.shown.lock().expect("the screen");
			String::from_utf8_lossy(&shown[shown.len().saturating_sub(2_000)..]).into_owned()
		};
		let drawn = self.screen().join("\n");
		format!(
			"client {:?}; the screen:\n{drawn}\nthe terminal: {screen:?}\n{}",
			self.client.try_wait(),
			logs(sandbox)
		)
	}
}

/// The sandbox's Jocasta logs, for a failure's message.
fn logs(sandbox: &Sandbox) -> String {
	let dir = sandbox.path().join("data").join("logs");
	let logs: Vec<String> = std::fs::read_dir(dir)
		.into_iter()
		.flatten()
		.flatten()
		.filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
		.map(|entry| {
			format!("{}:\n{}", entry.path().display(), std::fs::read_to_string(entry.path()).unwrap_or_default())
		})
		.collect();
	logs.join("\n")
}

/// One test: the failed start and the working one share a sandbox. Bare `jocasta` is one process, the service and the
/// shell together (0683): a start that can't finish ends it saying why, and closing the window ends the service.
#[test]
fn bare_jocasta_is_one_process_one_that_cant_start_ends_saying_why_closing_his_terminal_closes_jocasta_0549_0582_0683()
{
	let sandbox = Sandbox::new();
	let service_file = sandbox.path().join("data").join("jocasta-service.json");

	// The boss's home can't be written (a file holds the name of its `.omp` folder), so the boss, and with it the
	// service, can't start: bare `jocasta` says why and exits 1.
	let boss_home = sandbox.path().join("data").join("home");
	std::fs::create_dir_all(&boss_home).expect("the boss's home");
	std::fs::write(boss_home.join(".omp"), "").expect("the blocker");
	let mut first = Terminal::bare(&sandbox, None);
	let ended = waited(90, || matches!(first.client.try_wait(), Ok(Some(_))));
	assert!(ended, "jocasta outlived a service that didn't start; {}", first.report(&sandbox));
	let code = first.client.wait().expect("the client's exit").exit_code();
	assert_eq!(code, 1, "{}", first.report(&sandbox));
	let said = waited(10, || first.has(b"Jocasta stopped while starting"));
	assert!(said, "{}", first.report(&sandbox));
	assert!(!service_file.exists(), "a service file outlived a failed start");
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
	let mut terminal = Terminal::bare(&sandbox, Some(&git_dir));
	assert!(waited(90, || service_file.exists()), "the service never listened; {}", terminal.report(&sandbox));
	assert!(matches!(terminal.client.try_wait(), Ok(None)), "jocasta ended");
	// The shell draws the dock in the boss's workspace.
	let docked = waited(60, || terminal.screen().iter().any(|line| line.contains("NEEDS YOU")));
	assert!(docked, "the dock never showed; {}", terminal.report(&sandbox));
	// `jocasta worktree remove` waits for his Allow; the question is drawn in his terminal, its code read off it.
	let mut remove =
		sandbox.jocasta(&["worktree", "remove", &leftover.display().to_string()]).spawn().expect("worktree remove");
	let asked = waited(60, || terminal.code().is_some() || matches!(remove.try_wait(), Ok(Some(_))));
	if matches!(remove.try_wait(), Ok(Some(_))) {
		let out = finish(remove, 5);
		panic!("worktree remove ended unasked: {}{}; {}", out.stdout, out.stderr, terminal.report(&sandbox));
	}
	assert!(asked, "no question with a code showed; {}", terminal.report(&sandbox));
	let code = terminal.code().expect("the code");
	// The question ignores keys for its first 750 ms (0578).
	thread::sleep(Duration::from_millis(1_500));
	terminal.type_text(&code.to_lowercase());
	let answered = waited(60, || matches!(remove.try_wait(), Ok(Some(_))));
	assert!(answered, "worktree remove still waits after the code {code}; {}", terminal.report(&sandbox));
	let removed = finish(remove, 5);
	assert_eq!(removed.code, Some(0), "{}{}; {}", removed.stdout, removed.stderr, terminal.report(&sandbox));
	assert!(!leftover.exists(), "the leftover worktree stayed after his Allow");

	// His terminal window closes: Jocasta ends on its own and takes every pane's process with it.
	let roots = pane_roots(&sandbox);
	assert!(!roots.is_empty(), "the boss's pane has no process; {}", terminal.report(&sandbox));
	terminal.close(&sandbox, &roots);
}

/// Runs a `jocasta` word that may ask for his Allow in the terminal: a new code on screen is typed back (after the
/// question's 750 ms of ignored keys, 0578), until the word ends. `last` is the code typed before, which stays drawn.
fn allowed(sandbox: &Sandbox, terminal: &mut Terminal, last: &mut Option<String>, args: &[&str]) -> Out {
	let mut child = sandbox.jocasta(args).spawn().expect("a jocasta word");
	let deadline = Instant::now() + Duration::from_secs(90);
	while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
		if let Some(code) = terminal.code().filter(|code| last.as_ref() != Some(code)) {
			thread::sleep(Duration::from_millis(1_500));
			terminal.type_text(&code.to_lowercase());
			*last = Some(code);
		}
		thread::sleep(Duration::from_millis(200));
	}
	finish(child, 10)
}

/// The pid of every pane's process, as the service's `panes` command lists them.
fn pane_roots(sandbox: &Sandbox) -> Vec<u32> {
	panes(sandbox).iter().filter_map(|pane| pane["root"].as_u64().and_then(|root| u32::try_from(root).ok())).collect()
}

/// The service's panes, each as the `panes` command's object.
fn panes(sandbox: &Sandbox) -> Vec<Value> {
	let listed = cli(sandbox, "panes", json!({})).expect("the panes command");
	serde_json::from_str::<Value>(&listed).ok().and_then(|list| list.as_array().cloned()).expect("a list of panes")
}

/// One `cli` command of the sandbox's running service on its bridge, as the `jocasta thread` words make it (`panes`
/// and `pane_text` have no word of their own); the answer's text, or the error's.
fn cli(sandbox: &Sandbox, command: &str, args: Value) -> Result<String, String> {
	use jocasta::shared::bridge_wire::CliPeer;

	let file = std::fs::read_to_string(sandbox.path().join("data").join("jocasta-service.json"))
		.map_err(|error| format!("no service file: {error}"))?;
	let file: Value = serde_json::from_str(&file).map_err(|error| error.to_string())?;
	let port = file["port"].as_u64().and_then(|port| u16::try_from(port).ok()).ok_or("the file has no port")?;
	let token = file["token"].as_str().ok_or("the file has no token")?.to_owned();
	let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
	runtime.block_on(async move {
		let mut peer = CliPeer::open(port, &token).await.map_err(|error| error.to_string())?;
		let reply = peer
			.request(json!({ "t": "cli", "command": command, "args": args }))
			.await
			.map_err(|error| error.to_string())?;
		if reply.get("ok").and_then(Value::as_bool) == Some(false) {
			return Err(reply["error"].as_str().unwrap_or_default().to_owned());
		}
		Ok(reply["text"].as_str().unwrap_or_default().to_owned())
	})
}

/// The opt-in smoke on this machine (`mise run smoke`, `JOCASTA_HERDR_SMOKE=1`, the variable's name kept): a thread
/// started from the command line (through his Allow in the terminal) gets a pane of its own in a worktree, which the
/// service lists and reads through the bridge; closing the window ends every pane's process, the thread's too (0683;
/// the old smoke's omp found as a thread on herdr's events).
#[test]
fn live_smoke_a_thread_gets_a_pane_the_service_reads_and_the_windows_close_ends_every_process() {
	if std::env::var("JOCASTA_HERDR_SMOKE").as_deref() != Ok("1") {
		return;
	}
	let sandbox = Sandbox::new();
	let service_file = sandbox.path().join("data").join("jocasta-service.json");
	fake_omp(&sandbox);
	let git_dir = git_folder();
	leftover_worktree(&sandbox);
	let path = std::env::join_paths([sandbox.path().join("bin").as_path(), &git_dir]).expect("a PATH");
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
	let mut terminal = Terminal::bare(&sandbox, Some(&git_dir));
	assert!(waited(90, || service_file.exists()), "the service never listened; {}", terminal.report(&sandbox));
	assert!(waited(60, || cli(&sandbox, "ping", json!({})).is_ok()), "no answer; {}", terminal.report(&sandbox));
	assert!(waited(60, || !panes(&sandbox).is_empty()), "no boss pane; {}", terminal.report(&sandbox));
	let before = panes(&sandbox);
	let threads = |sandbox: &Sandbox| {
		finish(sandbox.jocasta(&["thread", "list"]).spawn().expect("thread list"), 30).stdout.trim().to_owned()
	};
	assert_eq!(threads(&sandbox), "No threads.");

	let mut last_code = None;
	let started =
		allowed(&sandbox, &mut terminal, &mut last_code, &["thread", "new", "demo", "say hi", "--title", "smoke"]);
	assert_eq!(started.code, Some(0), "{}{}; {}", started.stdout, started.stderr, terminal.report(&sandbox));
	let listed = waited(30, || threads(&sandbox) != "No threads.");
	assert!(listed, "the thread never showed in the list; {}", terminal.report(&sandbox));
	let grown = waited(30, || panes(&sandbox).len() > before.len());
	assert!(grown, "the thread got no pane; {}", terminal.report(&sandbox));
	let pane = panes(&sandbox)
		.into_iter()
		.find(|pane| !before.iter().any(|old| old["pane_id"] == pane["pane_id"]))
		.expect("the thread's pane");
	let id = pane["pane_id"].as_str().expect("its id").to_owned();
	let root = pane["root"].as_u64().and_then(|root| u32::try_from(root).ok()).expect("its process");
	assert!(jocasta::shared::proctree::pid_alive(root), "the thread's process isn't running");
	cli(&sandbox, "pane_text", json!({ "pane": id })).expect("the pane's screen through the bridge");

	// Closing the window ends the thread's pane process with the boss's: nothing outlives Jocasta.
	let roots = pane_roots(&sandbox);
	assert!(roots.contains(&root), "the thread's process isn't listed; {}", terminal.report(&sandbox));
	terminal.close(&sandbox, &roots);
}
