//! `Term`'s grid → ratatui cells, and the grid as plain text (`pane_text`).

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as AColor, CursorShape};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::pty::Proxy;

fn color(color: AColor) -> Color {
	match color {
		AColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
		AColor::Indexed(i) => Color::Indexed(i),
		AColor::Named(named) => match named as usize {
			n @ 0..16 => Color::Indexed(u8::try_from(n).unwrap_or(0)),
			n @ 259..=266 => Color::Indexed(u8::try_from(n - 259).unwrap_or(0)),
			_ => Color::Reset,
		},
	}
}

/// Draws the grid into `area` of `buf`; returns where the cursor goes (none when hidden or scrolled back).
pub fn draw_term(buf: &mut Buffer, area: Rect, term: &Term<Proxy>) -> Option<Position> {
	let content = term.renderable_content();
	let offset = i32::try_from(content.display_offset).unwrap_or(0);
	let selection = content.selection;
	let mut utf8 = [0u8; 4];
	for cell in content.display_iter {
		let flags = cell.flags;
		if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
			continue;
		}
		let line = cell.point.line.0 + offset;
		let (Ok(line), Ok(column)) = (u16::try_from(line), u16::try_from(cell.point.column.0)) else { continue };
		if column >= area.width || line >= area.height {
			continue;
		}
		let (x, y) = (area.x + column, area.y + line);
		let mut modifier = Modifier::empty();
		for (flag, m) in [
			(Flags::BOLD, Modifier::BOLD),
			(Flags::DIM, Modifier::DIM),
			(Flags::ITALIC, Modifier::ITALIC),
			(Flags::UNDERLINE, Modifier::UNDERLINED),
			(Flags::STRIKEOUT, Modifier::CROSSED_OUT),
			(Flags::INVERSE, Modifier::REVERSED),
		] {
			if flags.intersects(flag) {
				modifier |= m;
			}
		}
		if selection.is_some_and(|range| range.contains(cell.point)) {
			modifier ^= Modifier::REVERSED;
		}
		let style = Style::default().fg(color(cell.fg)).bg(color(cell.bg)).add_modifier(modifier);
		let out = &mut buf[(x, y)];
		match cell.zerowidth() {
			Some(extra) => {
				let mut text = String::from(cell.c);
				text.extend(extra);
				out.set_symbol(&text);
			}
			None => {
				out.set_symbol(if flags.contains(Flags::HIDDEN) { " " } else { cell.c.encode_utf8(&mut utf8) });
			}
		}
		out.set_style(style);
	}
	let cursor = content.cursor;
	if content.mode.contains(TermMode::SHOW_CURSOR) && offset == 0 && cursor.shape != CursorShape::Hidden {
		let column = u16::try_from(cursor.point.column.0).ok()?;
		let line = u16::try_from(cursor.point.line.0).ok()?;
		if column < area.width && line < area.height {
			return Some(Position::new(area.x + column, area.y + line));
		}
	}
	None
}

/// The visible grid as text: rows trimmed at the right, the empty rows at the bottom dropped.
pub fn screen_text(term: &Term<Proxy>) -> String {
	let grid = term.grid();
	let mut lines: Vec<String> = Vec::new();
	for line in 0..term.screen_lines() {
		let row = &grid[Line(i32::try_from(line).unwrap_or(0))];
		let mut text = String::new();
		for column in 0..term.columns() {
			let cell = &row[Column(column)];
			if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
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
