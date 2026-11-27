//! A program in a pty of the benchmark's own (0683): Bare `jocasta` is the terminal itself, so it needs a real console,
//! and the competitors' TUIs need one to draw at all. The pty is the vendored portable-pty's (its ConPTY flags and
//! bundled runtime), its output is parsed into `alacritty_terminal`'s grid, and the grid's text is what readiness is
//! matched on and what a failure shows. Closing the window is ending the pty ([`Pty::end`]).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use regex::Regex;

/// The size of every bench terminal.
const COLS: u16 = 120;
const ROWS: u16 = 40;

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Answers what the program asks of its terminal (the cursor's place, which Windows' pseudo console waits for before
/// any output flows).
#[derive(Clone)]
struct Answers(Writer);

impl EventListener for Answers {
	fn send_event(&self, event: Event) {
		if let Event::PtyWrite(text) = event {
			let mut writer = locked(&self.0);
			let _ = writer.write_all(text.as_bytes());
			let _ = writer.flush();
		}
	}
}

/// A running program and its terminal.
pub struct Pty {
	term: Arc<Mutex<Term<Answers>>>,
	child: Mutex<Box<dyn Child + Send + Sync>>,
	writer: Writer,
	master: Mutex<Option<Box<dyn MasterPty + Send>>>,
	pid: Option<u32>,
}

impl Pty {
	/// Starts `argv` in `cwd` with this process's environment, `env` over it and the names in `unset` out of it.
	pub fn spawn(argv: &[String], cwd: &Path, env: &[(String, String)], unset: &[&str]) -> std::io::Result<Self> {
		let Some((program, args)) = argv.split_first() else {
			return Err(std::io::Error::other("a pty needs a command"));
		};
		let pair = native_pty_system()
			.openpty(PtySize { rows: ROWS, cols: COLS, pixel_width: 0, pixel_height: 0 })
			.map_err(other)?;
		let mut command = CommandBuilder::new(program);
		command.args(args);
		command.cwd(cwd);
		command.env("TERM", "xterm-256color");
		command.env("COLORTERM", "truecolor");
		// On Windows the pty's base environment takes PATH and PATHEXT from the registry over the process's own; this
		// process's are what the program must see (a sandbox's stub `omp` first on PATH).
		for (name, value) in std::env::vars_os() {
			if name.eq_ignore_ascii_case("PATH") || name.eq_ignore_ascii_case("PATHEXT") {
				command.env(name, value);
			}
		}
		for name in unset {
			command.env_remove(name);
		}
		for (name, value) in env {
			command.env(name, value);
		}
		let child = pair.slave.spawn_command(command).map_err(other)?;
		drop(pair.slave);
		let pid = child.process_id();
		let mut reader = pair.master.try_clone_reader().map_err(other)?;
		let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(other)?));
		let term = Arc::new(Mutex::new(Term::new(
			Config::default(),
			&TermSize::new(usize::from(COLS), usize::from(ROWS)),
			Answers(Arc::clone(&writer)),
		)));
		let fed = Arc::clone(&term);
		std::thread::Builder::new().name("bench-pty-reader".into()).spawn(move || {
			let mut parser: Processor = Processor::new();
			let mut buffer = vec![0_u8; 64 * 1024];
			while let Ok(count @ 1..) = reader.read(&mut buffer) {
				parser.advance(&mut *locked(&fed), &buffer[..count]);
			}
		})?;
		Ok(Self { term, child: Mutex::new(child), writer, master: Mutex::new(Some(pair.master)), pid })
	}

	/// The process started in the pty.
	pub fn pid(&self) -> Option<u32> {
		self.pid
	}

	/// Bytes to the program, as typed.
	pub fn send(&self, bytes: &[u8]) {
		let mut writer = locked(&self.writer);
		let _ = writer.write_all(bytes);
		let _ = writer.flush();
	}

	/// The program's exit code once it has ended.
	pub fn exit_code(&self) -> Option<u32> {
		locked(&self.child).try_wait().ok().flatten().map(|status| status.exit_code())
	}

	/// Whether the program still runs.
	pub fn running(&self) -> bool {
		self.exit_code().is_none()
	}

	/// The visible grid as text: rows trimmed at the right, the empty rows at the bottom dropped.
	pub fn screen(&self) -> String {
		let term = locked(&self.term);
		let grid = term.grid();
		let mut lines: Vec<String> = Vec::new();
		for line in 0..term.screen_lines() {
			let row = &grid[Line(i32::try_from(line).unwrap_or(0))];
			let mut text = String::new();
			for column in 0..term.columns() {
				let cell = &row[Column(column)];
				if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
				{
					continue;
				}
				text.push(cell.c);
				if let Some(extra) = cell.zerowidth() {
					text.extend(extra);
				}
			}
			lines.push(text.trim_end().to_owned());
		}
		while lines.last().is_some_and(String::is_empty) {
			lines.pop();
		}
		lines.join("\n")
	}

	/// ms until the visible screen matches `pattern`, or `None` past `timeout_ms` (or when the program ends first).
	pub async fn wait_for(&self, pattern: &str, timeout_ms: i64) -> Option<f64> {
		let pattern = Regex::new(pattern).ok()?;
		let started = Instant::now();
		let limit = Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
		loop {
			if pattern.is_match(&self.screen()) {
				return Some(started.elapsed().as_secs_f64() * 1000.0);
			}
			if started.elapsed() >= limit || !self.running() {
				return None;
			}
			tokio::time::sleep(Duration::from_millis(25)).await;
		}
	}

	/// Waits up to `timeout_ms` for the program to end; its exit code.
	pub async fn wait_exit(&self, timeout_ms: i64) -> Option<u32> {
		let started = Instant::now();
		let limit = Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
		loop {
			if let Some(code) = self.exit_code() {
				return Some(code);
			}
			if started.elapsed() >= limit {
				return None;
			}
			tokio::time::sleep(Duration::from_millis(50)).await;
		}
	}

	/// Closes the window as he does: the pseudo console goes (on a thread of its own: closing a ConPTY can wait on its
	/// output) and the program is left to end by itself, with every process it started (0683). On Linux and macOS the
	/// program is sent the hangup a closing terminal sends: the reader and writer keep their copies of the pty open, so
	/// dropping the master alone never hangs it up (0720).
	pub fn close_window(&self) {
		if let Some(master) = locked(&self.master).take() {
			std::thread::spawn(move || drop(master));
		}
		#[cfg(unix)]
		if let Some(pid) = self.pid {
			let _ = std::process::Command::new("kill")
				.args(["-HUP", &pid.to_string()])
				.stdout(std::process::Stdio::null())
				.stderr(std::process::Stdio::null())
				.status();
		}
	}

	/// Closes the window: ends the program and closes the pty (on a thread of its own: closing a ConPTY can wait on its
	/// output). Whoever wants the program's whole tree gone lists it first (`kill_tree`): it can't be found once its
	/// root is gone.
	pub fn end(&self) {
		let _ = locked(&self.child).kill();
		if let Some(master) = locked(&self.master).take() {
			std::thread::spawn(move || drop(master));
		}
	}
}

impl Drop for Pty {
	fn drop(&mut self) {
		self.end();
	}
}

fn other<E: std::fmt::Display>(error: E) -> std::io::Error {
	std::io::Error::other(error.to_string())
}

/// `command_line` as the shell runs it: the argv that starts it in a [`Pty`].
pub fn shell_command(command_line: &str) -> Vec<String> {
	if cfg!(windows) {
		let shell = jocasta::shared::env::var("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned());
		vec![shell, "/d".to_owned(), "/s".to_owned(), "/c".to_owned(), command_line.to_owned()]
	} else {
		vec!["/bin/sh".to_owned(), "-c".to_owned(), command_line.to_owned()]
	}
}
