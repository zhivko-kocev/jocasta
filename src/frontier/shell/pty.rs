//! One pane (0683): a pty made by the vendored portable-pty (its ConPTY flags and bundled `conpty/` runtime), its output
//! parsed into `alacritty_terminal`'s `Term` by a reader thread of Jocasta's own, and the listener that tells the shell.
//!
//! Not alacritty's own `tty` and `EventLoop`: under the bundled ConPTY they stall past about 64 KB of output (the
//! spike's finding, `docs/research/own-shell-spike.md`). The reader's buffer is 256 KB and each batch is one wake-up.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, Rgb};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::crossterm::event::Event;

/// Scrollback lines kept per pane (0683).
const SCROLLBACK: usize = 10_000;

/// What the UI thread hears.
pub enum Ui {
	/// A pane's output was parsed (by pane uid).
	Wake(u64),
	/// Something the frame shows changed: a title, an exit, a tab made or closed by the service.
	Changed,
	/// A pane's process ended (by pane uid), and whether it ended cleanly (exit code 0).
	Exit(u64, bool),
	/// A program asked to copy text (OSC 52).
	Clip(String),
	/// A key, mouse or paste event of his.
	Input(Event),
	/// A message of the service's half of the shell (`protocol.rs`'s `SHELL_*` kinds).
	Service(String, String),
	/// The console input ended (the terminal window was closed): leave.
	Quit,
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The listener of one `Term`: parser answers go back to the program, everything else to the UI thread.
#[derive(Clone)]
pub struct Proxy {
	uid: u64,
	ui: Sender<Ui>,
	/// A wake-up is queued and the UI hasn't drawn since: further output sends no more.
	pub pending: Arc<AtomicBool>,
	title: Arc<Mutex<String>>,
	writer: Writer,
	size: Arc<Mutex<WindowSize>>,
}

impl Proxy {
	fn write(&self, bytes: &[u8]) {
		let mut writer = locked(&self.writer);
		let _ = writer.write_all(bytes);
		let _ = writer.flush();
	}
}

/// The colours answered to OSC 10/11/4 queries: Jocasta's own palette's foreground and background, grey otherwise.
fn answer_color(index: usize) -> Rgb {
	match index {
		256 => Rgb { r: 0xc0, g: 0xca, b: 0xf5 },
		257 => Rgb { r: 0x1a, g: 0x1b, b: 0x26 },
		_ => Rgb { r: 0x80, g: 0x80, b: 0x80 },
	}
}

impl EventListener for Proxy {
	fn send_event(&self, event: TermEvent) {
		match event {
			TermEvent::Wakeup => {
				if !self.pending.swap(true, Ordering::AcqRel) {
					let _ = self.ui.send(Ui::Wake(self.uid));
				}
			}
			TermEvent::Title(title) => {
				*locked(&self.title) = title;
				let _ = self.ui.send(Ui::Changed);
			}
			TermEvent::ResetTitle => {
				locked(&self.title).clear();
				let _ = self.ui.send(Ui::Changed);
			}
			TermEvent::PtyWrite(text) => self.write(text.as_bytes()),
			TermEvent::ColorRequest(index, format) => self.write(format(answer_color(index)).as_bytes()),
			TermEvent::TextAreaSizeRequest(format) => {
				let size = *locked(&self.size);
				self.write(format(size).as_bytes());
			}
			TermEvent::ClipboardStore(_, text) => {
				let _ = self.ui.send(Ui::Clip(text));
			}
			TermEvent::ClipboardLoad(_, format) => self.write(format("").as_bytes()),
			TermEvent::ChildExit(status) => {
				let _ = self.ui.send(Ui::Exit(self.uid, status.success()));
			}
			TermEvent::Exit => {
				let _ = self.ui.send(Ui::Exit(self.uid, true));
			}
			TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange | TermEvent::Bell => {}
		}
	}
}

/// What starts a pane.
pub struct Launch<'a> {
	pub argv: &'a [String],
	pub cwd: &'a Path,
	pub env: &'a [(String, String)],
	pub cols: u16,
	pub rows: u16,
}

struct Live {
	cols: u16,
	rows: u16,
	master: Option<Box<dyn MasterPty + Send>>,
	/// None for a sleeping pane, which runs nothing.
	killer: Option<Box<dyn ChildKiller + Send + Sync>>,
}

/// The second line of a sleeping pane's screen (0676).
pub const WAKE_LINE: &str = "press any key to wake";

/// What a sleeping pane does on a key (0689): asks for its wake, then calls the callback with how it went (`Err` says
/// why it didn't). It must not block: it is called on the UI thread.
pub type Wake = Arc<dyn Fn(Box<dyn FnOnce(Result<(), String>) + Send>) + Send + Sync>;

/// `text` without control characters: a title is outside text and must not move the cursor or recolour the pane.
fn plain(text: &str) -> String {
	text.chars().filter(|c| !c.is_control()).collect()
}

/// A sleeping pane's screen: `text`, then [`WAKE_LINE`], each line ended for a raw terminal.
pub fn sleeper_screen(text: &str) -> String {
	format!("\r\n  {}\r\n  {WAKE_LINE}\r\n", plain(text))
}

type Grid = Weak<FairMutex<Term<Proxy>>>;

/// Writes `text` on a sleeping pane's grid and tells the UI.
fn say(grid: &Grid, uid: u64, ui: &Sender<Ui>, text: &str) {
	if let Some(term) = grid.upgrade() {
		let mut parser: Processor = Processor::new();
		parser.advance(&mut *term.lock(), text.as_bytes());
		let _ = ui.send(Ui::Wake(uid));
	}
}

/// A sleeping pane's input: the first key asks for the wake; keys while one is under way do nothing.
struct StillInput {
	uid: u64,
	ui: Sender<Ui>,
	grid: Arc<OnceLock<Grid>>,
	busy: Arc<AtomicBool>,
	wake: Wake,
}

impl Write for StillInput {
	fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
		if bytes.is_empty() || self.busy.swap(true, Ordering::AcqRel) {
			return Ok(bytes.len());
		}
		let grid = self.grid.get().cloned().unwrap_or_default();
		let (uid, ui, busy) = (self.uid, self.ui.clone(), Arc::clone(&self.busy));
		// Not here: a write can come while the grid is locked (a parser answer).
		let (said_grid, said_ui) = (grid.clone(), ui.clone());
		std::thread::spawn(move || say(&said_grid, uid, &said_ui, "\r\n  waking...\r\n"));
		(self.wake)(Box::new(move |result| {
			if let Err(why) = result {
				say(&grid, uid, &ui, &format!("  {}\r\n  {WAKE_LINE}\r\n", plain(&why)));
			}
			busy.store(false, Ordering::Release);
		}));
		Ok(bytes.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

/// One pane: its grid, its process and its pty. Shared as `Arc<Pane>`.
pub struct Pane {
	/// A number no other pane of this process has, for its events.
	pub uid: u64,
	pub term: Arc<FairMutex<Term<Proxy>>>,
	pub proxy: Proxy,
	/// The pane's process (omp in Jocasta's panes).
	pub pid: Option<u32>,
	title: Arc<Mutex<String>>,
	size: Arc<Mutex<WindowSize>>,
	live: Mutex<Live>,
}

fn window_size(cols: u16, rows: u16) -> WindowSize {
	WindowSize { num_lines: rows, num_cols: cols, cell_width: 8, cell_height: 16 }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
	PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

fn other<E: std::fmt::Display>(error: E) -> std::io::Error {
	std::io::Error::other(error.to_string())
}

/// A pane's grid and the listener it talks to the program and the UI through.
struct PaneGrid {
	proxy: Proxy,
	term: Arc<FairMutex<Term<Proxy>>>,
	title: Arc<Mutex<String>>,
	size: Arc<Mutex<WindowSize>>,
}

impl PaneGrid {
	fn new(uid: u64, ui: &Sender<Ui>, writer: Writer, cols: u16, rows: u16) -> Self {
		let size = Arc::new(Mutex::new(window_size(cols, rows)));
		let title = Arc::new(Mutex::new(String::new()));
		let proxy = Proxy {
			uid,
			ui: ui.clone(),
			pending: Arc::new(AtomicBool::new(false)),
			title: Arc::clone(&title),
			writer,
			size: Arc::clone(&size),
		};
		let config = Config { kitty_keyboard: true, scrolling_history: SCROLLBACK, ..Config::default() };
		let term = Arc::new(FairMutex::new(Term::new(
			config,
			&TermSize::new(usize::from(cols), usize::from(rows)),
			proxy.clone(),
		)));
		Self { proxy, term, title, size }
	}
}

impl Pane {
	/// Starts `launch.argv` in a new pty and the threads that feed its grid. The pane's environment is Jocasta's own
	/// with `launch.env` over it; `TERM` and `COLORTERM` tell its program what this terminal can draw.
	pub fn spawn(uid: u64, ui: Sender<Ui>, launch: &Launch<'_>) -> std::io::Result<Self> {
		let Some((program, args)) = launch.argv.split_first() else {
			return Err(std::io::Error::other("a pane needs a command"));
		};
		let (cols, rows) = (launch.cols.max(1), launch.rows.max(1));
		let pair = native_pty_system().openpty(pty_size(cols, rows)).map_err(other)?;
		let mut command = CommandBuilder::new(program);
		command.args(args);
		command.cwd(launch.cwd);
		command.env("TERM", "xterm-256color");
		command.env("COLORTERM", "truecolor");
		for (name, value) in launch.env {
			command.env(name, value);
		}
		let mut child = pair.slave.spawn_command(command).map_err(other)?;
		drop(pair.slave);
		let pid = child.process_id();
		let killer = child.clone_killer();
		let mut reader = pair.master.try_clone_reader().map_err(other)?;
		let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(other)?));
		let PaneGrid { proxy, term, title, size } = PaneGrid::new(uid, &ui, writer, cols, rows);
		// The reader: bytes → the parser → `Term`, then one wake-up per batch.
		let (reader_term, reader_proxy) = (Arc::clone(&term), proxy.clone());
		std::thread::Builder::new().name("pty-reader".into()).spawn(move || {
			let mut parser: Processor = Processor::new();
			let mut buf = vec![0u8; 256 * 1024];
			loop {
				match reader.read(&mut buf) {
					Ok(0) | Err(_) => break,
					Ok(count) => {
						{
							let mut term = reader_term.lock();
							parser.advance(&mut *term, &buf[..count]);
						}
						reader_proxy.send_event(TermEvent::Wakeup);
					}
				}
			}
		})?;
		// The child's exit.
		std::thread::Builder::new().name("pty-wait".into()).spawn(move || {
			let clean = child.wait().is_ok_and(|status| status.success());
			let _ = ui.send(Ui::Exit(uid, clean));
		})?;
		let live = Live { cols, rows, master: Some(pair.master), killer: Some(killer) };
		Ok(Self { uid, term, proxy, pid, title, size, live: Mutex::new(live) })
	}

	/// A sleeping pane (0689): no process and no pty, its grid showing [`sleeper_screen`]`(text)`; a key in it calls
	/// `wake` (one at a time).
	pub fn still(uid: u64, ui: Sender<Ui>, (cols, rows): (u16, u16), text: &str, wake: Wake) -> Self {
		let (cols, rows) = (cols.max(1), rows.max(1));
		let cell: Arc<OnceLock<Grid>> = Arc::default();
		let input =
			StillInput { uid, ui: ui.clone(), grid: Arc::clone(&cell), busy: Arc::new(AtomicBool::new(false)), wake };
		let writer: Writer = Arc::new(Mutex::new(Box::new(input)));
		let PaneGrid { proxy, term, title, size } = PaneGrid::new(uid, &ui, writer, cols, rows);
		let _ = cell.set(Arc::downgrade(&term));
		let mut parser: Processor = Processor::new();
		parser.advance(&mut *term.lock(), sleeper_screen(text).as_bytes());
		let live = Live { cols, rows, master: None, killer: None };
		Self { uid, term, proxy, pid: None, title, size, live: Mutex::new(live) }
	}

	/// The title its program set (OSC 0/2), empty when none.
	pub fn title(&self) -> String {
		locked(&self.title).clone()
	}

	/// A sleeping pane (0689), which runs nothing.
	pub fn asleep(&self) -> bool {
		locked(&self.live).killer.is_none()
	}

	/// Bytes to the program, as typed.
	pub fn send(&self, bytes: &[u8]) {
		if !bytes.is_empty() {
			self.proxy.write(bytes);
		}
	}

	/// Resizes the pty and the grid; nothing when the size is the same.
	pub fn resize(&self, cols: u16, rows: u16) {
		let mut live = locked(&self.live);
		if (cols, rows) == (live.cols, live.rows) || cols == 0 || rows == 0 {
			return;
		}
		live.cols = cols;
		live.rows = rows;
		*locked(&self.size) = window_size(cols, rows);
		if let Some(master) = &live.master {
			let _ = master.resize(pty_size(cols, rows));
		}
		drop(live);
		self.term.lock().resize(TermSize::new(usize::from(cols), usize::from(rows)));
	}

	/// Ends the child and closes the pty (on a thread of its own: closing a ConPTY can wait on its output). The caller
	/// that wants the child's whole tree gone lists it first (`proctree::end_processes`): the tree can't be found once
	/// its root is gone.
	pub fn close(&self) {
		let mut live = locked(&self.live);
		if let Some(killer) = live.killer.as_mut() {
			let _ = killer.kill();
		}
		if let Some(master) = live.master.take() {
			std::thread::spawn(move || drop(master));
		}
	}
}

impl Drop for Pane {
	fn drop(&mut self) {
		self.close();
	}
}

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;
