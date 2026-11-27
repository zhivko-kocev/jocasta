//! What the `today` dock and the question box share (0551, 0553, 0634; fork plan phase 5): both draw plain lines of
//! coloured runs into herdr's client buffer. This module owns the line model and its column math (display columns, so
//! wide characters count twice), the lines drawn into a ratatui buffer, and pasted text stripped of escape sequences.

use std::path::Path;
use std::sync::OnceLock;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use serde_json::Value;

/// Tokyo Night as herdr draws it, as terminal colours; the palette file may replace them (0698).
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Tokyo Night, the built-in palette of the terminal surfaces and the brief windows (0698, 0721).
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

/// The palette this run draws with and the font family the palette file names (0698, 0721). Set once by
/// `load_palette` at start, so tests, which never call it, draw with Tokyo Night whatever file the machine holds.
static ACTIVE: OnceLock<(Palette, Option<String>)> = OnceLock::new();

/// Reads the palette file at `path` once, at start (0698): each valid `colors` entry replaces the built-in colour of
/// its name; an unknown name, a value that isn't `#rrggbb`, a malformed file or no file keeps the built-in values. Its
/// `font`, a family name of at most 100 characters, is what the brief windows write in (0721).
pub fn load_palette(path: &Path) {
	let file = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok());
	let colours = file.as_ref().map(palette_colours).unwrap_or_default();
	let _ = ACTIVE.set((apply_colours(&colours), file.as_ref().and_then(palette_font)));
}

/// The palette this run draws with: Tokyo Night unless the palette file named other colours.
pub fn palette() -> &'static Palette {
	ACTIVE.get().map_or(&TOKYO_NIGHT, |(palette, _)| palette)
}

/// The font family the palette file names, if any (0721).
pub fn font_family() -> Option<&'static str> {
	ACTIVE.get().and_then(|(_, font)| font.as_deref())
}

/// A palette file's `font`: a family name, trimmed, of 1 to 100 characters and no control characters.
fn palette_font(file: &Value) -> Option<String> {
	let name = file.get("font")?.as_str()?.trim();
	(!name.is_empty() && name.chars().count() <= 100 && !name.chars().any(char::is_control)).then(|| name.to_owned())
}

/// The names the palette file may set.
const PALETTE_NAMES: [&str; 15] = [
	"bg-0", "bg-1", "bg-2", "bg-3", "fg", "fg-2", "fg-3", "comment", "blue", "cyan", "yellow", "red", "green",
	"purple", "orange",
];

/// The valid `colors` entries of a palette file, by name.
fn palette_colours(file: &Value) -> Vec<(&'static str, [u8; 3])> {
	let Some(colors) = file.get("colors").and_then(Value::as_object) else {
		return Vec::new();
	};
	PALETTE_NAMES
		.iter()
		.filter_map(|name| colors.get(*name).and_then(Value::as_str).and_then(hex_colour).map(|rgb| (*name, rgb)))
		.collect()
}

/// `#rrggbb` as its three bytes.
fn hex_colour(text: &str) -> Option<[u8; 3]> {
	let digits = text.strip_prefix('#')?;
	if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
		return None;
	}
	let n = u32::from_str_radix(digits, 16).ok()?;
	Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// Tokyo Night with `colours` in place.
fn apply_colours(colours: &[(&'static str, [u8; 3])]) -> Palette {
	let mut palette = TOKYO_NIGHT;
	for &(name, [r, g, b]) in colours {
		let slot = match name {
			"bg-0" => &mut palette.bg0,
			"bg-1" => &mut palette.bg1,
			"bg-2" => &mut palette.bg2,
			"bg-3" => &mut palette.bg3,
			"fg" => &mut palette.fg,
			"fg-2" => &mut palette.fg2,
			"fg-3" => &mut palette.fg3,
			"comment" => &mut palette.comment,
			"blue" => &mut palette.blue,
			"cyan" => &mut palette.cyan,
			"yellow" => &mut palette.yellow,
			"red" => &mut palette.red,
			"green" => &mut palette.green,
			"purple" => &mut palette.purple,
			_ => &mut palette.orange,
		};
		*slot = Color::Rgb(r, g, b);
	}
	palette
}

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
	let mut style = Style::new().fg(segment.fg.unwrap_or(palette().fg));
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
	buf.set_style(area, Style::new().fg(palette().fg).bg(palette().bg0));
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

#[cfg(test)]
#[path = "tui_tests.rs"]
pub(crate) mod tests;
