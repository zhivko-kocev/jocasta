//! The terminal programs' shared shell (0551, 0553, 0634): the `today` pane and the `ask` popup are ratatui programs
//! that draw plain lines of coloured runs. This module owns what both need: the line model and its column math
//! (display columns, so wide characters count twice), the lines drawn into a ratatui buffer, and [`run_pane`], the one
//! lifecycle: no signal left to the terminal library, a redraw on resize, draws that do nothing once torn down, and one
//! teardown path (the program's cleanup, then the terminal restored).

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEvent, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::shared::errors::{Error, Result};

/// Tokyo Night as `palette.css` holds it (0474): the one palette file's hex values, as terminal colours.
pub struct Palette {
	pub bg0: Color,
	pub bg1: Color,
	pub bg2: Color,
	pub bg3: Color,
	pub fg: Color,
	pub fg2: Color,
	pub fg3: Color,
	pub mark3: Color,
	pub comment: Color,
	pub bright_black: Color,
	pub blue: Color,
	pub cyan: Color,
	pub yellow: Color,
	pub red: Color,
	pub green: Color,
	pub purple: Color,
	pub orange: Color,
}

/// Tokyo Night; a test keeps these equal to `src/frontier/canvas/palette.css`.
pub const TOKYO_NIGHT: Palette = Palette {
	bg0: Color::Rgb(0x1a, 0x1b, 0x26),
	bg1: Color::Rgb(0x23, 0x26, 0x36),
	bg2: Color::Rgb(0x24, 0x28, 0x3b),
	bg3: Color::Rgb(0x2d, 0x36, 0x50),
	fg: Color::Rgb(0xc0, 0xca, 0xf5),
	fg2: Color::Rgb(0xa9, 0xb1, 0xd6),
	fg3: Color::Rgb(0x8a, 0x93, 0xbf),
	mark3: Color::Rgb(0x69, 0x71, 0x96),
	comment: Color::Rgb(0x56, 0x5f, 0x89),
	bright_black: Color::Rgb(0x41, 0x48, 0x68),
	blue: Color::Rgb(0x7a, 0xa2, 0xf7),
	cyan: Color::Rgb(0x7d, 0xcf, 0xff),
	yellow: Color::Rgb(0xe0, 0xaf, 0x68),
	red: Color::Rgb(0xf7, 0x76, 0x8e),
	green: Color::Rgb(0x9e, 0xce, 0x6a),
	purple: Color::Rgb(0xbb, 0x9a, 0xf7),
	orange: Color::Rgb(0xff, 0x9e, 0x64),
};

/// A run of text in one colour.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Segment {
	pub text: String,
	pub fg: Option<Color>,
	pub bg: Option<Color>,
	pub bold: bool,
}

impl Segment {
	/// Text in the default colour.
	pub fn plain(text: impl Into<String>) -> Self {
		Self { text: text.into(), ..Self::default() }
	}

	/// Text in `fg`.
	pub fn new(text: impl Into<String>, fg: Color) -> Self {
		Self { text: text.into(), fg: Some(fg), ..Self::default() }
	}

	/// The same run in bold.
	#[must_use]
	pub fn bold(mut self) -> Self {
		self.bold = true;
		self
	}

	/// The same run on `bg`.
	#[must_use]
	pub fn on(mut self, bg: Color) -> Self {
		self.bg = Some(bg);
		self
	}
}

/// One row of coloured runs.
pub type Line = Vec<Segment>;

/// The columns `text` takes in a terminal: wide (CJK) characters take two.
pub fn columns(text: &str) -> usize {
	Span::raw(text).width()
}

/// The columns a line takes.
pub fn line_columns(line: &[Segment]) -> usize {
	line.iter().map(|s| columns(&s.text)).sum()
}

/// `text` as grapheme-like clusters: a character that takes no column (a combining mark, a variation selector) or
/// follows a zero-width joiner stays with the one before it.
fn clusters(text: &str) -> Vec<&str> {
	let mut out = Vec::new();
	let mut start = 0;
	let mut joined = false;
	for (at, c) in text.char_indices() {
		let mut buf = [0u8; 4];
		let zero = columns(c.encode_utf8(&mut buf)) == 0;
		if at > start && !(zero || joined) {
			out.push(&text[start..at]);
			start = at;
		}
		joined = c == '\u{200d}';
	}
	if start < text.len() {
		out.push(&text[start..]);
	}
	out
}

/// `text` split into pieces of at most `width` columns, every character kept (a wide one never split).
fn chop(text: &str, width: usize) -> Vec<String> {
	let mut out = Vec::new();
	let mut piece = String::new();
	let mut used = 0;
	for segment in clusters(text) {
		let size = columns(segment);
		if used > 0 && used + size > width {
			out.push(std::mem::take(&mut piece));
			used = 0;
		}
		piece.push_str(segment);
		used += size;
	}
	if !piece.is_empty() || out.is_empty() {
		out.push(piece);
	}
	out
}

/// Text cut to `width` columns, ending in "…" when it was longer.
pub fn cut(text: &str, width: usize) -> String {
	if width == 0 {
		return String::new();
	}
	if columns(text) <= width {
		return text.to_string();
	}
	let head = if width > 1 { chop(text, width - 1).into_iter().next().unwrap_or_default() } else { String::new() };
	if columns(&head) < width { format!("{head}…") } else { "…".to_string() }
}

/// A line's segments cut to `width` columns in all.
pub fn fit(line: &[Segment], width: usize) -> Line {
	let mut out = Vec::new();
	let mut left = width;
	for segment in line {
		if left == 0 {
			break;
		}
		let text = cut(&segment.text, left);
		left = left.saturating_sub(columns(&text));
		out.push(Segment { text, ..segment.clone() });
	}
	out
}

/// `text` wrapped at word boundaries to `width` columns; a word longer than the line is split, never cut.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
	let mut out = Vec::new();
	for paragraph in text.split('\n') {
		let mut line = String::new();
		let mut used = 0;
		for word in paragraph.split_whitespace() {
			let size = columns(word);
			if !line.is_empty() && used + 1 + size <= width {
				line.push(' ');
				line.push_str(word);
				used += 1 + size;
				continue;
			}
			if !line.is_empty() {
				out.push(std::mem::take(&mut line));
			}
			let mut pieces = chop(word, width);
			line = pieces.pop().unwrap_or_default();
			out.extend(pieces);
			used = columns(&line);
		}
		out.push(line);
	}
	out
}

/// The style a segment draws in: its colours over the pane's, bold when it says so.
fn style_of(segment: &Segment) -> Style {
	let mut style = Style::new().fg(segment.fg.unwrap_or(TOKYO_NIGHT.fg));
	if let Some(bg) = segment.bg {
		style = style.bg(bg);
	}
	if segment.bold {
		style = style.add_modifier(Modifier::BOLD);
	}
	style
}

/// Draws `lines` into `area` of `buf`: the area filled with the pane's background, one line a row from the top, no
/// wrapping (a miscounted line clips at the edge instead of pushing the bottom rows off, FE-10), rows past the area
/// dropped.
pub fn render_lines(buf: &mut Buffer, area: Rect, lines: &[Line]) {
	buf.set_style(area, Style::new().fg(TOKYO_NIGHT.fg).bg(TOKYO_NIGHT.bg0));
	for (row, line) in lines.iter().take(usize::from(area.height)).enumerate() {
		let y = area.y + u16::try_from(row).unwrap_or(u16::MAX);
		let mut x = area.x;
		for segment in line {
			let room = usize::from(area.x + area.width - x);
			if room == 0 {
				break;
			}
			let (next_x, _) = buf.set_stringn(x, y, &segment.text, room, style_of(segment));
			x = next_x;
		}
	}
}

/// What a running program can ask of its pane; clones act on the same pane, from any thread or task.
#[derive(Clone)]
pub struct Pane {
	draw: Arc<dyn Fn() + Send + Sync>,
	end: Arc<dyn Fn(i32) + Send + Sync>,
}

impl Pane {
	/// A pane over `draw` and `end`; [`run_pane`] makes the real one, a test makes its own.
	pub fn new(draw: impl Fn() + Send + Sync + 'static, end: impl Fn(i32) + Send + Sync + 'static) -> Self {
		Self { draw: Arc::new(draw), end: Arc::new(end) }
	}

	/// Draws the program's lines now; nothing once the program is ending.
	pub fn draw(&self) {
		(self.draw)();
	}

	/// Ends the program with `code` (the first call counts).
	pub fn end(&self, code: i32) {
		(self.end)(code);
	}
}

/// A program [`run_pane`] runs.
pub trait PaneProgram {
	/// The lines for `width` × `height`.
	fn lines(&mut self, width: u16, height: u16) -> Vec<Line>;

	/// A key pressed (releases never arrive).
	fn on_key(&mut self, _key: KeyEvent) {}

	/// Pasted text, stripped of escape sequences (FE-11: a terminal's colour codes never reach a brief).
	fn on_paste(&mut self, _text: &str) {}

	/// Runs once when the program ends, before the terminal is restored: release held requests, close feeds, clear
	/// timers.
	fn cleanup(&mut self) {}
}

/// `text` without terminal escape sequences: CSI (`ESC [ … final`), OSC (`ESC ] … BEL` or `ESC \`) and two-character
/// escapes.
pub fn strip_ansi(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut chars = text.chars().peekable();
	while let Some(c) = chars.next() {
		if c != '\u{1b}' {
			out.push(c);
			continue;
		}
		match chars.next() {
			Some('[') => {
				for next in chars.by_ref() {
					if ('\u{40}'..='\u{7e}').contains(&next) {
						break;
					}
				}
			}
			Some(']') => {
				while let Some(next) = chars.next() {
					if next == '\u{7}' {
						break;
					}
					if next == '\u{1b}' {
						chars.next_if_eq(&'\\');
						break;
					}
				}
			}
			_ => {}
		}
	}
	out
}

/// What the loop reacts to.
enum Msg {
	Draw,
	End(i32),
	Event(Event),
}

/// One draw of `program`'s lines over the whole terminal.
fn draw_program<B: Backend, P: PaneProgram>(terminal: &mut Terminal<B>, program: &mut P) -> Result<()> {
	terminal
		.draw(|frame| {
			let area = frame.area();
			let lines = program.lines(area.width, area.height);
			render_lines(frame.buffer_mut(), area, &lines);
		})
		.map(|_| ())
		.map_err(|e| Error::other(format!("drawing the pane failed: {e}")))
}

/// The loop over `terminal`: draws at once, then on every `Draw` and resize, hands keys and paste to the program, and
/// ends on the first `End`; resolves its code once the program's cleanup ran.
async fn drive<B: Backend, P: PaneProgram>(
	terminal: &mut Terminal<B>,
	tx: UnboundedSender<Msg>,
	mut rx: UnboundedReceiver<Msg>,
	setup: impl FnOnce(Pane) -> P,
) -> Result<i32> {
	let draw_tx = tx.clone();
	let pane = Pane::new(
		move || {
			let _ = draw_tx.send(Msg::Draw);
		},
		move |code| {
			let _ = tx.send(Msg::End(code));
		},
	);
	let mut program = setup(pane);
	let ran: Result<i32> = async {
		draw_program(terminal, &mut program)?;
		loop {
			let Some(msg) = rx.recv().await else { return Ok(0) };
			match msg {
				Msg::End(code) => return Ok(code),
				Msg::Draw | Msg::Event(Event::Resize(..)) => draw_program(terminal, &mut program)?,
				Msg::Event(Event::Key(key)) if key.kind != KeyEventKind::Release => program.on_key(key),
				Msg::Event(Event::Paste(text)) => program.on_paste(&strip_ansi(&text)),
				Msg::Event(_) => {}
			}
		}
	}
	.await;
	program.cleanup();
	ran
}

/// Reads terminal events on a thread of its own until `stop`, handing them to the loop.
fn read_events(tx: UnboundedSender<Msg>, stop: Arc<AtomicBool>) {
	while !stop.load(Ordering::Relaxed) {
		match event::poll(Duration::from_millis(100)) {
			Ok(true) => match event::read() {
				Ok(event) => {
					if tx.send(Msg::Event(event)).is_err() {
						return;
					}
				}
				Err(_) => return,
			},
			Ok(false) => {}
			Err(_) => return,
		}
	}
}

/// Resolves when a signal that ends a pane arrives (FE-3: a popup that only left its terminal kept its held request
/// open); the program's cleanup runs on every one.
async fn end_signal() {
	#[cfg(windows)]
	{
		use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close};
		let (Ok(mut c), Ok(mut b), Ok(mut close)) = (ctrl_c(), ctrl_break(), ctrl_close()) else {
			return std::future::pending().await;
		};
		tokio::select! { _ = c.recv() => {}, _ = b.recv() => {}, _ = close.recv() => {} }
	}
	#[cfg(unix)]
	{
		use tokio::signal::unix::{SignalKind, signal};
		let kinds = [
			SignalKind::interrupt(),
			SignalKind::terminate(),
			SignalKind::hangup(),
			SignalKind::quit(),
			SignalKind::pipe(),
		];
		let mut streams: Vec<_> = kinds.into_iter().filter_map(|k| signal(k).ok()).collect();
		if streams.is_empty() {
			return std::future::pending().await;
		}
		let waits = streams.iter_mut().map(|s| Box::pin(s.recv())).collect::<Vec<_>>();
		futures_util::future::select_all(waits).await;
	}
}

/// Gives the terminal back when dropped, on every way out.
struct Restore;

impl Drop for Restore {
	fn drop(&mut self) {
		let _ = execute!(io::stdout(), DisableBracketedPaste);
		ratatui::restore();
	}
}

/// Runs one terminal program until it ends (its own `end` or a signal); resolves its exit code once the terminal is
/// restored. `setup` gets the pane and returns the program; it may draw at once.
pub async fn run_pane<P: PaneProgram>(setup: impl FnOnce(Pane) -> P) -> Result<i32> {
	let mut terminal = ratatui::try_init()?;
	let _restore = Restore;
	execute!(io::stdout(), EnableBracketedPaste)?;
	let (tx, rx) = unbounded_channel();
	let stop = Arc::new(AtomicBool::new(false));
	let reader = {
		let (tx, stop) = (tx.clone(), Arc::clone(&stop));
		std::thread::spawn(move || read_events(tx, stop))
	};
	let signals = {
		let tx = tx.clone();
		tokio::spawn(async move {
			end_signal().await;
			let _ = tx.send(Msg::End(0));
		})
	};
	let code = drive(&mut terminal, tx, rx, setup).await;
	signals.abort();
	stop.store(true, Ordering::Relaxed);
	let _ = reader.join();
	code
}

#[cfg(test)]
#[path = "tui_tests.rs"]
pub(crate) mod tests;
