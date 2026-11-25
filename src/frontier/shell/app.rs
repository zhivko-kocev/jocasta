//! The shell's event loop (0683): one terminal window, the sidebar, the front tab's pane and, beside the boss's, the
//! `today` dock; his questions over everything. It draws only on damage (a pane's output, a title, a service message, an
//! input that changes what is shown), never for a hidden tab, and never for a key by itself.
//!
//! Frames follow the spike's findings (`docs/research/own-shell-spike.md` §4): the floor between frames is a
//! `thread::sleep` (`recv_timeout` rounds to 15 ms on Windows), a frame waits for 3 ms of quiet (at most 12 ms), and
//! each frame is one write inside DEC 2026 synchronized output with the cursor set once after.

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{TermMode, viewport_to_point};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm;
use ratatui::crossterm::event::{
	DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange,
	EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseButton,
	MouseEvent, MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::{
	BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
	disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::pty::Ui;
use super::tabs::{Front, Hub};
use super::{JocastaShell, Reply, keys, sidebar, term};
use crate::frontier::protocol::{PluginAction, SHELL_ACTION, ShellAction};
use crate::frontier::server::native::Native;
use crate::frontier::tui::TOKYO_NIGHT;
use crate::shared::clock::now_ms;

/// The sidebar's width, and the narrowest terminal that has one.
const SIDEBAR: u16 = 28;
const SIDEBAR_MIN_TERMINAL: u16 = 70;
/// The dock's width, and the narrowest main area (beside the sidebar) that has one.
const DOCK: u16 = 46;
const DOCK_MIN_MAIN: u16 = 90;
/// A frame waits for this much quiet, and at most this long after the first change.
const QUIET: Duration = Duration::from_millis(3);
const MAX_WAIT: Duration = Duration::from_millis(12);
/// How often the shell looks at what the bridge says (the sidebar's states) and ticks the dock's clock.
const SLOW_TICK: Duration = Duration::from_secs(1);
/// This many text presses with no gap are a paste, on Windows, where a paste arrives as single key presses.
const PASTE_BURST: usize = 8;
/// The id the service knows this shell by (`Native::joined`).
const CLIENT: u64 = 1;

/// The terminal: crossterm's backend over a buffered stdout, so each frame is one write.
type Term = Terminal<CrosstermBackend<io::BufWriter<io::Stdout>>>;

/// What the shell is made of.
pub struct Parts {
	pub hub: Arc<Hub>,
	pub shell: JocastaShell,
	pub native: Arc<Native>,
	pub tx: Sender<Ui>,
	pub rx: Receiver<Ui>,
}

fn base64(data: &[u8]) -> String {
	const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut out = String::new();
	for chunk in data.chunks(3) {
		let n = u32::from(chunk[0]) << 16
			| u32::from(*chunk.get(1).unwrap_or(&0)) << 8
			| u32::from(*chunk.get(2).unwrap_or(&0));
		out.push(T[(n >> 18) as usize & 63] as char);
		out.push(T[(n >> 12) as usize & 63] as char);
		out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
		out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
	}
	out
}

/// Where the frame's parts go.
#[derive(Clone, Copy, Default)]
struct Layout {
	sidebar: Rect,
	pane: Rect,
	dock: Option<Rect>,
	status: Rect,
}

/// Lays the frame out for `area`; `docked` says the front tab is the boss's.
fn layout(area: Rect, docked: bool) -> Layout {
	let status = Rect { y: area.y + area.height.saturating_sub(1), height: area.height.min(1), ..area };
	let body = Rect { height: area.height.saturating_sub(1), ..area };
	let side = if area.width >= SIDEBAR_MIN_TERMINAL { SIDEBAR.min(area.width / 3) } else { 0 };
	let sidebar = Rect { width: side, ..body };
	let main = Rect { x: body.x + side, width: body.width - side, ..body };
	if docked && main.width >= DOCK_MIN_MAIN {
		let dock = Rect { x: main.x + main.width - DOCK, width: DOCK, ..main };
		return Layout { sidebar, pane: Rect { width: main.width - DOCK, ..main }, dock: Some(dock), status };
	}
	Layout { sidebar, pane: main, dock: None, status }
}

fn inside(rect: Rect, column: u16, row: u16) -> bool {
	column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

struct App {
	hub: Arc<Hub>,
	shell: JocastaShell,
	native: Arc<Native>,
	/// The dock has the keys.
	dock_focus: bool,
	/// Ctrl+B was pressed: the next key is a shell chord.
	prefix: bool,
	/// Chords taken by the shell whose release must not reach a pane.
	swallowed: Vec<KeyCode>,
	selecting: bool,
	/// What the frame last drew in, for the mouse.
	laid: Layout,
	/// The typed text of the goto prompt (Alt+G), when it is open.
	goto: Option<String>,
	/// The window title last set.
	title: String,
}

impl App {
	fn front(&self) -> Option<Front> {
		self.hub.front()
	}

	/// The shell's own messages go to the service.
	fn deliver(&self, reply: &Reply) {
		for (kind, data) in &reply.send {
			self.native.message(CLIENT, kind, data);
		}
	}

	fn action(&self, action: PluginAction) {
		let pane_id = self.front().map(|front| front.pane_id);
		let data = serde_json::to_string(&ShellAction { action, pane_id }).unwrap_or_default();
		self.native.message(CLIENT, SHELL_ACTION, &data);
	}

	fn draw(&mut self, terminal: &mut Term) -> io::Result<()> {
		let now = now_ms();
		let size = terminal.size()?;
		let area = Rect { x: 0, y: 0, width: size.width, height: size.height };
		let front = self.front();
		if let Some(front) = &front {
			let title = format!("Jocasta · {}", front.workspace_label);
			if title != self.title {
				crossterm::queue!(terminal.backend_mut(), SetTitle(&title))?;
				self.title = title;
			}
		}
		let docked = front.as_ref().is_some_and(|front| self.shell.docks(&front.workspace_id));
		let laid = layout(area, docked);
		self.laid = laid;
		self.hub.set_area(laid.pane.width.saturating_add(laid.dock.map_or(0, |dock| dock.width)), laid.pane.height);
		if let Some(front) = &front {
			front.pane.resize(laid.pane.width, laid.pane.height);
			front.pane.proxy.pending.store(false, std::sync::atomic::Ordering::Release);
		}
		let rows = self.hub.rows(now);
		let status = self.shell.status().unwrap_or_default();
		let asking = self.shell.asking();
		let dock_focus = self.dock_focus && laid.dock.is_some() && !asking;
		crossterm::queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
		let shell = &mut self.shell;
		terminal.draw(|frame| {
			let buf = frame.buffer_mut();
			buf.set_style(area, Style::new().fg(TOKYO_NIGHT.fg).bg(TOKYO_NIGHT.bg0));
			if laid.sidebar.width > 0 {
				sidebar::draw(buf, laid.sidebar, &rows);
			}
			let mut cursor = None;
			match &front {
				Some(front) => {
					let term = front.pane.term.lock();
					cursor = term::draw_term(buf, laid.pane, &term);
				}
				None => {
					buf.set_stringn(
						laid.pane.x + 2,
						laid.pane.y + 1,
						"Starting…",
						usize::from(laid.pane.width.saturating_sub(2)),
						Style::new().fg(TOKYO_NIGHT.comment),
					);
				}
			}
			if let Some(dock) = laid.dock {
				shell.draw_dock(buf, dock, dock_focus);
			}
			let hint = if self.goto.is_some() {
				"Enter jumps · Esc cancels"
			} else if dock_focus {
				"dock: Esc gives the keys back"
			} else {
				"Ctrl+B Alt+T today · Alt+1-9 agents · Alt+G goto"
			};
			// The HUD (0403, 0437, 0554): the blocked count and today's spend, then the time.
			let clock = chrono::Local::now().format("%H:%M").to_string();
			let hud = if status.is_empty() { clock } else { format!("{status} · {clock}") };
			let line = match &self.goto {
				Some(typed) => format!("goto: {typed}▏  ·  {hint}"),
				None => format!("{hud}  ·  {hint}"),
			};
			buf.set_style(laid.status, Style::new().fg(TOKYO_NIGHT.fg3).bg(TOKYO_NIGHT.bg1));
			buf.set_stringn(
				laid.status.x + 1,
				laid.status.y,
				line,
				usize::from(laid.status.width.saturating_sub(1)),
				Style::new().fg(TOKYO_NIGHT.fg3).bg(TOKYO_NIGHT.bg1).remove_modifier(Modifier::BOLD),
			);
			let question = shell.asking().then(|| shell.draw_question(buf, area));
			match (question, dock_focus, cursor) {
				(Some(_), _, _) | (None, true, _) | (None, false, None) => {}
				(None, false, Some(position)) => frame.set_cursor_position(position),
			}
		})?;
		crossterm::queue!(terminal.backend_mut(), EndSynchronizedUpdate)?;
		terminal.backend_mut().flush()
	}

	fn key(&mut self, key: KeyEvent) {
		let press = matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat);
		if self.shell.asking() {
			if press {
				let reply = self.shell.question_key(key);
				self.deliver(&reply);
			}
			return;
		}
		if self.goto.is_some() {
			if press {
				self.goto_key(&key);
			}
			return;
		}
		if !press {
			// A release of a key the shell took stays with the shell.
			if let Some(at) = self.swallowed.iter().position(|code| *code == key.code) {
				self.swallowed.remove(at);
				return;
			}
			return self.send_to_pane(&key);
		}
		let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
		let alt = key.modifiers.contains(KeyModifiers::ALT);
		if self.prefix {
			self.prefix = false;
			self.swallowed.push(key.code);
			return self.chord(&key, ctrl, alt);
		}
		if ctrl && !alt && key.code == KeyCode::Char('b') {
			self.prefix = true;
			self.swallowed.push(key.code);
			return;
		}
		if self.dock_focus && self.laid.dock.is_some() {
			let reply = self.shell.dock_key(key);
			self.deliver(&reply);
			if reply.release {
				self.dock_focus = false;
			}
			if !reply.pass {
				self.swallowed.push(key.code);
				return;
			}
		}
		if alt && !ctrl {
			let now = now_ms();
			match key.code {
				KeyCode::Char(digit @ '1'..='9') => {
					self.hub.switch_row(usize::from(digit as u8 - b'1'), now);
					self.swallowed.push(key.code);
					return;
				}
				KeyCode::Char(']') => {
					self.hub.cycle(1, now);
					self.swallowed.push(key.code);
					return;
				}
				KeyCode::Char('[') => {
					self.hub.cycle(-1, now);
					self.swallowed.push(key.code);
					return;
				}
				KeyCode::Char('g') => {
					self.goto = Some(String::new());
					self.swallowed.push(key.code);
					return;
				}
				_ => {}
			}
		}
		self.send_to_pane(&key)
	}

	/// The key after Ctrl+B.
	fn chord(&mut self, key: &KeyEvent, ctrl: bool, alt: bool) {
		let now = now_ms();
		if ctrl {
			if key.code == KeyCode::Char('b')
				&& let Some(front) = self.front()
			{
				front.pane.send(&[0x02]);
			}
			return;
		}
		match key.code {
			KeyCode::Char('t') if alt => {
				if self.laid.dock.is_some() {
					self.dock_focus = true;
				}
			}
			KeyCode::Char('f') if alt => self.action(PluginAction::Finish),
			KeyCode::Char('s') if alt => self.action(PluginAction::Stop),
			KeyCode::Char('r') if alt => self.action(PluginAction::Review),
			KeyCode::Char('n') if alt => self.action(PluginAction::NewThread),
			KeyCode::Char('q') if alt => self.action(PluginAction::Quit),
			KeyCode::Char('g') => self.goto = Some(String::new()),
			KeyCode::Char(shifted) => {
				if let Some(index) = "!@#$%^&*(".find(shifted) {
					self.hub.switch_row(index, now);
				}
			}
			_ => {}
		}
	}

	fn send_to_pane(&mut self, key: &KeyEvent) {
		let Some(front) = self.front() else { return };
		let mode = *front.pane.term.lock().mode();
		if let Some(bytes) = keys::encode_key(key, mode) {
			front.pane.term.lock().selection = None;
			front.pane.send(&bytes);
		}
	}

	/// A key while the goto prompt is open: type to name an agent, Enter goes to the first whose name has the text,
	/// Esc cancels (`goto` of 0404, in tabs).
	fn goto_key(&mut self, key: &KeyEvent) {
		let Some(typed) = self.goto.as_mut() else { return };
		match key.code {
			KeyCode::Esc => self.goto = None,
			KeyCode::Backspace => {
				typed.pop();
			}
			KeyCode::Enter => {
				let wanted = typed.to_lowercase();
				self.goto = None;
				if let Some(row) = self.hub.rows(now_ms()).iter().find(|row| row.label.to_lowercase().contains(&wanted))
				{
					self.hub.focus(&row.pane_id);
				}
			}
			KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => typed.push(c),
			_ => {}
		}
	}

	fn mouse(&mut self, event: MouseEvent) {
		if self.shell.asking() {
			return;
		}
		let (column, row) = (event.column, event.row);
		let down = matches!(event.kind, MouseEventKind::Down(MouseButton::Left));
		if self.laid.sidebar.width > 0 && inside(self.laid.sidebar, column, row) {
			if down {
				let rows = self.hub.rows(now_ms());
				if let Some(pane_id) = sidebar::pane_at(&rows, row - self.laid.sidebar.y) {
					self.hub.focus(pane_id);
				}
			}
			return;
		}
		if let Some(dock) = self.laid.dock.filter(|dock| inside(*dock, column, row)) {
			let reply = match event.kind {
				MouseEventKind::Down(MouseButton::Left) => {
					self.dock_focus = true;
					self.shell.dock_mouse(column, row)
				}
				MouseEventKind::ScrollUp => self.shell.dock_scroll(column, row, -1),
				MouseEventKind::ScrollDown => self.shell.dock_scroll(column, row, 1),
				_ => Reply::default(),
			};
			let _ = dock;
			self.deliver(&reply);
			return;
		}
		if !inside(self.laid.pane, column, row) {
			return;
		}
		if down {
			self.dock_focus = false;
		}
		let (column, row) = (column - self.laid.pane.x, row - self.laid.pane.y);
		let Some(front) = self.front() else { return };
		let mode = *front.pane.term.lock().mode();
		let shift = event.modifiers.contains(KeyModifiers::SHIFT);
		if !shift && let Some(bytes) = keys::encode_mouse(&event, column, row, mode) {
			front.pane.send(&bytes);
			return;
		}
		if mode.intersects(TermMode::MOUSE_MODE) && !shift {
			return;
		}
		let mut term = front.pane.term.lock();
		let offset = term.grid().display_offset();
		let point = viewport_to_point(offset, Point::new(usize::from(row), Column(usize::from(column))));
		match event.kind {
			MouseEventKind::Down(MouseButton::Left) => {
				term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
				self.selecting = true;
			}
			MouseEventKind::Drag(MouseButton::Left) if self.selecting => {
				if let Some(selection) = term.selection.as_mut() {
					selection.update(point, Side::Right);
				}
			}
			MouseEventKind::Up(MouseButton::Left) if self.selecting => {
				self.selecting = false;
				if let Some(text) = term.selection_to_string().filter(|text| !text.is_empty()) {
					copy(&text);
				}
			}
			MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
				let up = event.kind == MouseEventKind::ScrollUp;
				if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
					let arrow = match (mode.contains(TermMode::APP_CURSOR), up) {
						(true, true) => "\x1bOA",
						(true, false) => "\x1bOB",
						(false, true) => "\x1b[A",
						(false, false) => "\x1b[B",
					};
					drop(term);
					front.pane.send(arrow.repeat(3).as_bytes());
				} else {
					term.scroll_display(Scroll::Delta(if up { 3 } else { -3 }));
				}
			}
			_ => {}
		}
	}

	fn paste(&mut self, text: &str) {
		if self.shell.asking() {
			let reply = self.shell.question_paste(text);
			self.deliver(&reply);
			return;
		}
		if self.dock_focus && self.laid.dock.is_some() {
			return;
		}
		let Some(front) = self.front() else { return };
		let mode = *front.pane.term.lock().mode();
		front.pane.send(&keys::encode_paste(text, mode));
	}

	/// Returns whether the frame needs drawing.
	fn input(&mut self, event: Event) -> bool {
		match event {
			Event::Key(key) => {
				let shown = self.shell.asking() || self.dock_focus || self.prefix || self.goto.is_some();
				self.key(key);
				// A key shows only what it changes in the shell itself: the dock, a question, the front tab.
				shown || self.shell.asking() || self.dock_focus || self.prefix || self.goto.is_some()
			}
			Event::Mouse(mouse) => {
				self.mouse(mouse);
				true
			}
			Event::Paste(text) => {
				self.paste(&text);
				self.shell.asking()
			}
			Event::Resize(..) => true,
			Event::FocusGained | Event::FocusLost => {
				let gained = matches!(event, Event::FocusGained);
				if let Some(front) = self.front()
					&& front.pane.term.lock().mode().contains(TermMode::FOCUS_IN_OUT)
				{
					front.pane.send(if gained { b"\x1b[I" } else { b"\x1b[O" });
				}
				false
			}
		}
	}
}

/// OSC 52: the terminal puts `text` on his clipboard.
fn copy(text: &str) {
	let osc = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
	let mut out = io::stdout();
	let _ = out.write_all(osc.as_bytes());
	let _ = out.flush();
}

/// Runs the shell until his window closes (the console input ends), the service says to stop (`Ui::Quit`), or the
/// terminal fails. The terminal is put back as it was.
pub fn run(parts: Parts) -> io::Result<()> {
	let Parts { hub, shell, native, tx, rx } = parts;
	let (width, height) = ratatui::crossterm::terminal::size()?;
	// crossterm honours NO_COLOR by dropping every colour command; a terminal that hosts other programs must not.
	crossterm::style::force_color_output(true);
	enable_raw_mode()?;
	let mut stdout = io::stdout();
	crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
	let _ = crossterm::execute!(stdout, EnableBracketedPaste, EnableFocusChange);
	let enhanced = supports_keyboard_enhancement().unwrap_or(false);
	if enhanced {
		crossterm::execute!(
			stdout,
			PushKeyboardEnhancementFlags(
				KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
					| KeyboardEnhancementFlags::REPORT_EVENT_TYPES
					| KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
			)
		)?;
	}
	let mut terminal = Terminal::new(CrosstermBackend::new(io::BufWriter::with_capacity(1 << 16, io::stdout())))?;
	let input_tx = tx.clone();
	std::thread::Builder::new().name("shell-input".into()).spawn(move || {
		input_thread(&input_tx);
		let _ = input_tx.send(Ui::Quit);
	})?;
	// A closed window ends no read on Windows; its console's close event is the word to leave.
	#[cfg(windows)]
	{
		let closing = tx.clone();
		super::winin::on_close(move || {
			let _ = closing.send(Ui::Quit);
		});
	}
	let first = layout(Rect { x: 0, y: 0, width, height }, false);
	hub.set_area(first.pane.width, first.pane.height);
	native.joined(CLIENT, None);
	let mut app = App {
		hub,
		shell,
		native: Arc::clone(&native),
		dock_focus: false,
		prefix: false,
		swallowed: Vec::new(),
		selecting: false,
		laid: first,
		goto: None,
		title: String::new(),
	};
	let result = event_loop(&mut app, &mut terminal, &rx);
	native.left(CLIENT);
	if enhanced {
		let _ = crossterm::execute!(stdout, PopKeyboardEnhancementFlags);
	}
	let _ = crossterm::execute!(
		stdout,
		DisableFocusChange,
		DisableBracketedPaste,
		DisableMouseCapture,
		LeaveAlternateScreen
	);
	let _ = disable_raw_mode();
	result
}

fn event_loop(app: &mut App, terminal: &mut Term, rx: &Receiver<Ui>) -> io::Result<()> {
	let mut dirty = true;
	let mut dirty_since = Instant::now();
	let mut last_output = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
	let mut slow = Instant::now();
	let mut signature = String::new();
	loop {
		let timeout = if dirty {
			QUIET.saturating_sub(last_output.elapsed()).min(MAX_WAIT.saturating_sub(dirty_since.elapsed()))
		} else {
			SLOW_TICK.saturating_sub(slow.elapsed())
		};
		// std's sleep is high-resolution on Windows; `recv_timeout` is not (about 15 ms steps).
		let timeout = if dirty && !timeout.is_zero() {
			std::thread::sleep(timeout);
			Duration::ZERO
		} else {
			timeout
		};
		match rx.recv_timeout(timeout) {
			Ok(message) => {
				let mut next = Some(message);
				while let Some(message) = next {
					let mut touched = false;
					match message {
						Ui::Wake(uid) => {
							if app.front().is_some_and(|front| front.pane.uid == uid) {
								last_output = Instant::now();
								touched = true;
							}
						}
						Ui::Changed => touched = true,
						Ui::Exit(uid) => {
							app.hub.exited(uid);
							touched = true;
						}
						Ui::Clip(text) => copy(&text),
						Ui::Service(kind, data) => {
							let reply = app.shell.message(&kind, &data);
							app.deliver(&reply);
							touched = reply.redraw;
						}
						Ui::Quit => return Ok(()),
						Ui::Input(event) => {
							let shown = app.input(event);
							touched = shown;
						}
					}
					if touched {
						if !dirty {
							dirty_since = Instant::now();
						}
						dirty = true;
					}
					next = rx.try_recv().ok();
				}
			}
			Err(RecvTimeoutError::Timeout) => {}
			Err(RecvTimeoutError::Disconnected) => return Ok(()),
		}
		if slow.elapsed() >= SLOW_TICK {
			slow = Instant::now();
			let now = now_ms();
			let seen: String = app
				.hub
				.rows(now)
				.iter()
				.map(|row| {
					format!(
						"{}|{:?}|{}|{:?}{:?}{:?};",
						row.pane_id, row.state, row.text, row.cost, row.model, row.running
					)
				})
				.collect();
			let seen = format!("{seen}{}", chrono::Local::now().format("%H:%M"));
			if seen != signature || app.shell.tick() {
				signature = seen;
				if !dirty {
					dirty_since = Instant::now();
				}
				dirty = true;
			}
		}
		if dirty && (last_output.elapsed() >= QUIET || dirty_since.elapsed() >= MAX_WAIT) {
			app.draw(terminal)?;
			dirty = false;
		}
	}
}

/// crossterm on Windows reads the console API, which has no bracketed paste: a paste arrives as one key press and
/// release per character, newlines as Enter. [`PASTE_BURST`] or more text presses with no gap are taken for a paste.
fn input_thread(tx: &Sender<Ui>) {
	fn text_of(event: &Event) -> Option<Option<char>> {
		match event {
			Event::Key(KeyEvent { code, modifiers, kind, .. })
				if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
			{
				let ch = match code {
					KeyCode::Char(c) => Some(*c),
					KeyCode::Enter => Some('\n'),
					KeyCode::Tab => Some('\t'),
					_ => return None,
				};
				// `Some(None)` is a release, which rides along in a burst.
				Some(if *kind == KeyEventKind::Release { None } else { ch })
			}
			_ => None,
		}
	}
	let mut stash: Option<Event> = None;
	let mut input = Input::new();
	loop {
		let event = match stash.take() {
			Some(event) => event,
			None => match input.blocking() {
				Some(event) => event,
				None => return,
			},
		};
		if cfg!(windows) && text_of(&event).is_some() {
			let mut held = vec![event];
			while let Some(next) = input.queued() {
				if text_of(&next).is_some() {
					held.push(next);
				} else {
					stash = Some(next);
					break;
				}
			}
			let text: String = held.iter().filter_map(|event| text_of(event).flatten()).collect();
			let burst: Vec<Ui> = if text.chars().count() >= PASTE_BURST {
				vec![Ui::Input(Event::Paste(text))]
			} else {
				held.into_iter().map(Ui::Input).collect()
			};
			for message in burst {
				if tx.send(message).is_err() {
					return;
				}
			}
			continue;
		}
		if tx.send(Ui::Input(event)).is_err() {
			return;
		}
	}
}

/// Console input: on Windows our own reader (crossterm's drops characters outside the BMP, see `winin.rs`), elsewhere
/// crossterm's.
struct Input {
	#[cfg(windows)]
	console: super::winin::Console,
}

impl Input {
	fn new() -> Self {
		Self {
			#[cfg(windows)]
			console: super::winin::Console::new(),
		}
	}

	fn blocking(&mut self) -> Option<Event> {
		#[cfg(windows)]
		{
			self.console.next(None)
		}
		#[cfg(not(windows))]
		{
			crossterm::event::read().ok()
		}
	}

	fn queued(&mut self) -> Option<Event> {
		#[cfg(windows)]
		{
			self.console.try_next()
		}
		#[cfg(not(windows))]
		{
			if crossterm::event::poll(Duration::ZERO).unwrap_or(false) { crossterm::event::read().ok() } else { None }
		}
	}
}
