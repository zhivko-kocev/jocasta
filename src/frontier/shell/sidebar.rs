//! The sidebar (0683): the agents list, one row of two lines per tab, the boss's row first. The state glyph comes from
//! the bridge (0675), the second line from the service's own wording (`review ready`, a question) with the pane's
//! model and cost beside it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::tabs::Row;
use crate::frontier::server::surface::AgentState;
use crate::frontier::tui::palette;

/// Rows a tab takes.
const ROW_LINES: u16 = 2;

fn glyph(state: AgentState, text: &str) -> (&'static str, Color) {
	match state {
		AgentState::Blocked => ("◆", palette().red),
		AgentState::Working => ("●", palette().yellow),
		AgentState::Done => ("✓", palette().green),
		AgentState::Idle if text != "idle" => ("✓", palette().green),
		AgentState::Idle => ("○", palette().green),
		AgentState::Unknown => ("·", palette().comment),
	}
}

/// Draws the list into `area`.
pub fn draw(buf: &mut Buffer, area: Rect, rows: &[Row]) {
	buf.set_style(area, Style::new().fg(palette().fg).bg(palette().bg1));
	let width = usize::from(area.width);
	buf.set_stringn(
		area.x + 1,
		area.y,
		"AGENTS",
		width.saturating_sub(1),
		Style::new().fg(palette().comment).bg(palette().bg1).add_modifier(Modifier::BOLD),
	);
	for (index, row) in rows.iter().enumerate() {
		let Ok(offset) = u16::try_from(index) else { break };
		let y = area.y + 1 + offset * ROW_LINES;
		if y + 1 >= area.y + area.height {
			break;
		}
		let background = if row.front { palette().bg3 } else { palette().bg1 };
		let fill = Style::new().fg(palette().fg).bg(background);
		buf.set_style(Rect { x: area.x, y, width: area.width, height: ROW_LINES }, fill);
		let (mark, color) = glyph(row.state, &row.text);
		buf.set_stringn(area.x + 1, y, mark, 1, Style::new().fg(color).bg(background));
		let title = if row.pinned { Style::new().fg(palette().blue).bg(background) } else { fill };
		// Row 1: the state icon, the workspace, its short model and cost (0402); row 2: the state words alone, with the
		// full width, so a question or "review ready" isn't cut by them (UX-04).
		let mut first = row.label.clone();
		for part in [&row.model, &row.cost].into_iter().flatten() {
			first.push(' ');
			first.push_str(part);
		}
		buf.set_stringn(area.x + 3, y, &first, width.saturating_sub(4), title.add_modifier(Modifier::BOLD));
		let mut second = row.text.clone();
		if let Some(running) = &row.running {
			second.push_str(" · ");
			second.push_str(running);
		}
		let dim = Style::new().fg(palette().fg3).bg(background);
		buf.set_stringn(area.x + 3, y + 1, second, width.saturating_sub(4), dim);
	}
}

/// The pane a click on sidebar line `line` (counted from the area's top) lands on.
pub fn pane_at(rows: &[Row], line: u16) -> Option<&str> {
	let index = usize::from(line.checked_sub(1)? / ROW_LINES);
	rows.get(index).map(|row| row.pane_id.as_str())
}
