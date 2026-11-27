//! What the `ask` popup shows and how his keys answer it (0553, 0558, 0563, 0568, 0578, 0634), pure so it is tested
//! apart from the terminal: the arrows or Tab pick a choice; a coded question's choices other than its fallback (Allow,
//! Merge, PR, Stop everything) take its three letters typed, never Enter; the fallback takes Enter when picked, and Esc
//! always; PgUp/PgDn read a question taller than the popup; New thread picks a project and takes a brief. `main.rs`
//! draws the lines and posts the answer.

use std::sync::LazyLock;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;

use crate::frontier::protocol::{PopupAnswer, PopupForm, PopupView};
use crate::frontier::tui::{Line, Segment, fit, palette, wrap};

#[derive(Debug, Clone)]
pub struct AskState {
	pub view: PopupView,
	/// The picked choice's index: a coded question's first choice at first, otherwise the fallback's.
	pub picked: usize,
	/// The code's letters typed so far for the picked choice.
	pub typed: String,
	/// New thread: the picked project's index, the brief, and the field the keys go to.
	pub project: usize,
	pub brief: String,
	pub field: Field,
	/// The first line of the question's text shown, when it is taller than the popup (PgUp/PgDn move it).
	pub scroll: usize,
	/// One line above the hint: a wrong code, a refusal.
	pub note: Option<String>,
}

/// The field of New thread's form the keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
	Project,
	Brief,
}

/// A key as the popup reads it, the fields it needs: the names are OpenTUI's (`return`, `escape`, `pageup`, …), which
/// the popup's logic was written against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Key {
	pub name: String,
	pub sequence: String,
	pub ctrl: bool,
	pub meta: bool,
	pub shift: bool,
}

impl Key {
	/// A key by its name and the text it types.
	pub fn new(name: &str, sequence: &str) -> Self {
		Self { name: name.to_string(), sequence: sequence.to_string(), ..Self::default() }
	}
}

impl From<KeyEvent> for Key {
	fn from(event: KeyEvent) -> Self {
		let mut key = match event.code {
			KeyCode::Char(' ') => Key::new("space", " "),
			KeyCode::Char(c) => Key::new(&c.to_lowercase().collect::<String>(), &c.to_string()),
			KeyCode::Enter => Key::new("return", "\r"),
			KeyCode::Esc => Key::new("escape", "\u{1b}"),
			KeyCode::Tab => Key::new("tab", "\t"),
			KeyCode::BackTab => Key { shift: true, ..Key::new("tab", "\t") },
			KeyCode::Backspace => Key::new("backspace", "\u{7f}"),
			KeyCode::Left => Key::new("left", ""),
			KeyCode::Right => Key::new("right", ""),
			KeyCode::Up => Key::new("up", ""),
			KeyCode::Down => Key::new("down", ""),
			KeyCode::PageUp => Key::new("pageup", ""),
			KeyCode::PageDown => Key::new("pagedown", ""),
			_ => Key::default(),
		};
		key.ctrl |= event.modifiers.contains(KeyModifiers::CONTROL);
		key.meta |= event.modifiers.contains(KeyModifiers::ALT);
		key.shift |= event.modifiers.contains(KeyModifiers::SHIFT);
		key
	}
}

/// The state a popup opens in.
pub fn ask_state(view: PopupView) -> AskState {
	let projects = view.form.as_ref().map(|f| f.projects.as_slice()).unwrap_or_default();
	let project = view
		.form
		.as_ref()
		.and_then(|f| f.project.as_ref())
		.map_or(0, |p| projects.iter().position(|x| x == p).unwrap_or(0));
	let picked =
		if view.code.is_some() { 0 } else { view.choices.iter().position(|c| *c == view.fallback).unwrap_or(0) };
	AskState {
		view,
		picked,
		typed: String::new(),
		project,
		brief: String::new(),
		// The form opens on the brief, with a project always picked (0581, UX-15).
		field: Field::Brief,
		scroll: 0,
		note: None,
	}
}

/// A printable character the key types, if any.
fn typed_char(key: &Key) -> Option<char> {
	if key.ctrl || key.meta {
		return None;
	}
	let mut chars = key.sequence.chars();
	let c = chars.next()?;
	(chars.next().is_none() && u32::from(c) <= 0xffff && c >= ' ' && c != '\u{7f}').then_some(c)
}

/// Whether `state`'s picked choice takes the code.
fn coded(state: &AskState) -> bool {
	state.view.code.is_some() && state.view.choices.get(state.picked) != Some(&state.view.fallback)
}

/// The note for a letter typed on a coded choice that isn't A–Z, such as a Cyrillic layout's (0578).
pub fn latin_note(code: &str) -> String {
	format!("Type the Latin letters {code} (switch the keyboard layout).")
}

/// The note after a wrong code (0578): a warning when one try is left, `fallback` being what the third one takes.
pub fn wrong_note(left: Option<u32>, fallback: &str) -> String {
	if left == Some(1) {
		format!("Wrong code; one try left, then it counts as {fallback}.")
	} else {
		"Wrong code; nothing was done.".to_string()
	}
}

/// The note when the third wrong code took the fallback (0578), shown in red before the popup closes.
pub fn exhausted_note(fallback: &str) -> String {
	format!("Wrong code three times; {fallback} was taken.")
}

fn is_enter(key: &Key) -> bool {
	key.name == "return" || key.name == "enter"
}

/// New thread's form: Tab swaps field; the project list picks by arrows or first letter, the brief takes text.
fn press_form(state: &mut AskState, key: &Key, form: &PopupForm) -> Option<PopupAnswer> {
	let projects = &form.projects;
	if key.name == "tab" {
		state.field = if state.field == Field::Project { Field::Brief } else { Field::Project };
		return None;
	}
	if state.field == Field::Project {
		let char = typed_char(key);
		let count = projects.len();
		if count == 0 {
			return None;
		}
		if key.name == "up" {
			state.project = (state.project + count - 1) % count;
		} else if key.name == "down" {
			state.project = (state.project + 1) % count;
		} else if is_enter(key) {
			state.field = Field::Brief;
		} else if let Some(char) = char.filter(|c| *c != ' ') {
			// Type-to-find (UX-15): the next project, after the picked one, that starts with the letter.
			let letter = char.to_lowercase().to_string();
			let found = (1..=count)
				.map(|step| (state.project + step) % count)
				.find(|at| projects[*at].to_lowercase().starts_with(&letter));
			if let Some(at) = found {
				state.project = at;
			}
		}
		return None;
	}
	if key.name == "backspace" {
		state.brief.pop();
	} else if is_enter(key) {
		let brief = state.brief.trim();
		if brief.is_empty() {
			state.note = Some("Write the brief first.".to_string());
			return None;
		}
		return Some(PopupAnswer {
			choice: state.view.choices.first().cloned().unwrap_or_default(),
			typed: None,
			project: Some(projects.get(state.project).cloned().unwrap_or_default()),
			brief: Some(brief.to_string()),
		});
	} else if let Some(char) = typed_char(key) {
		state.brief.push(char);
	}
	None
}

/// Whether `c` is a letter in any script (`\p{L}`).
fn is_letter(c: char) -> bool {
	static LETTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{L}$").expect("a fixed pattern"));
	LETTER.is_match(c.encode_utf8(&mut [0; 4]))
}

/// A plain choice popup: arrows and Tab move, Enter takes, and a coded choice takes the letters of its code.
fn press_choices(state: &mut AskState, key: &Key) -> Option<PopupAnswer> {
	let count = state.view.choices.len();
	let choice = state.view.choices.get(state.picked)?.clone();
	if key.name == "left" || key.name == "up" || (key.name == "tab" && key.shift) {
		state.picked = (state.picked + count - 1) % count;
		state.typed.clear();
	} else if key.name == "right" || key.name == "down" || key.name == "tab" {
		state.picked = (state.picked + 1) % count;
		state.typed.clear();
	} else if is_enter(key) {
		if !coded(state) {
			return Some(PopupAnswer { choice, typed: None, project: None, brief: None });
		}
	} else if coded(state) {
		let code = state.view.code.clone()?;
		if key.name == "backspace" {
			state.typed.pop();
		}
		let char = typed_char(key);
		if let Some(char) =
			char.filter(|c| c.is_ascii_alphabetic() && state.typed.chars().count() < code.chars().count())
		{
			state.typed.push(char.to_ascii_uppercase());
			state.note = None;
			if state.typed.chars().count() == code.chars().count() {
				return Some(PopupAnswer { choice, typed: Some(state.typed.clone()), project: None, brief: None });
			}
		} else if char.is_some_and(is_letter) {
			state.note = Some(latin_note(&code));
		}
	}
	None
}

/// Applies one key; the answer it gives, if any.
pub fn press(state: &mut AskState, key: &Key) -> Option<PopupAnswer> {
	if key.name == "escape" || (key.ctrl && key.name == "c") {
		return Some(PopupAnswer { choice: state.view.fallback.clone(), typed: None, project: None, brief: None });
	}
	if key.name == "pageup" || key.name == "pagedown" {
		// The layout clamps it to the text's length.
		state.scroll = if key.name == "pageup" { state.scroll.saturating_sub(PAGE) } else { state.scroll + PAGE };
		return None;
	}
	match state.view.form.clone() {
		Some(form) => press_form(state, key, &form),
		None => press_choices(state, key),
	}
}

/// Pasted text (already stripped of escape sequences): into New thread's brief, one line.
pub fn paste(state: &mut AskState, text: &str) {
	if state.view.form.is_some() && state.field == Field::Brief {
		let mut spaced = false;
		for c in text.chars() {
			if c.is_whitespace() {
				if !spaced {
					state.brief.push(' ');
				}
				spaced = true;
			} else {
				state.brief.push(c);
				spaced = false;
			}
		}
	}
}

/// Lines PgUp and PgDn move the question's text by.
const PAGE: usize = 5;

/// A hint's parts joined with " · " in the separator colour (0581, UX-12).
fn hint(parts: &[String]) -> Line {
	let mut line = Vec::new();
	for (i, text) in parts.iter().enumerate() {
		if i > 0 {
			line.push(Segment::new(" · ", palette().comment));
		}
		line.push(Segment::new(text.as_str(), palette().fg3));
	}
	line
}

fn strings(parts: &[&str]) -> Vec<String> {
	parts.iter().map(ToString::to_string).collect()
}

/// New thread shows this many rows of the brief being typed, the last ones (0717).
const BRIEF_ROWS: usize = 3;

/// New thread's form block: the project, the brief (its tail, caret in the field the keys go to), the note and the hint.
fn form_foot(state: &AskState, form: &PopupForm, width: usize, reading: &[&str]) -> Vec<Line> {
	let project = form.projects.get(state.project).map_or("", String::as_str);
	let on = |field: Field, segment: Segment| if state.field == field { segment.on(palette().bg2) } else { segment };
	let mut lines: Vec<Line> = vec![vec![
		Segment::new("Project  ", palette().fg3),
		on(Field::Project, Segment::new(format!("‹ {project} ›"), palette().cyan).bold()),
	]];
	// The brief's last rows wrapped, so its end and the caret show, `…` when rows above are hidden; the caret only in
	// the field the keys go to (UX-15, 0717).
	let caret = if state.field == Field::Brief { "▏" } else { "" };
	let wrapped = wrap(&format!("{}{caret}", state.brief), width.saturating_sub(9).max(10));
	let hidden = wrapped.len().saturating_sub(BRIEF_ROWS);
	for (index, text) in wrapped.iter().skip(hidden).enumerate() {
		let label = match (index, hidden) {
			(0, 0) => "Brief    ",
			(0, _) => "Brief  … ",
			_ => "         ",
		};
		lines.push(vec![
			Segment::new(label, palette().fg3),
			on(Field::Brief, Segment::new(text.as_str(), palette().fg)),
		]);
	}
	lines.push(Vec::new());
	lines.extend(state.note.iter().map(|n| vec![Segment::new(n.as_str(), palette().red)]));
	let mut parts = if state.field == Field::Brief {
		strings(&["Tab project", "Enter start"])
	} else {
		strings(&["Type to find", "↑↓ project", "Enter brief"])
	};
	parts.extend(strings(reading));
	parts.push("Esc cancel".to_string());
	lines.push(hint(&parts));
	lines.iter().map(|l| fit(l, width)).collect()
}

/// The block at the popup's foot, laid out first so it always shows (0578): the choices or the form, the code to type
/// and the letters typed, the note, and the key hint (`read` when the question's text scrolls).
fn foot(state: &AskState, width: usize, read: bool) -> Vec<Line> {
	let view = &state.view;
	let note: Vec<Line> = state.note.iter().map(|n| vec![Segment::new(n.as_str(), palette().red)]).collect();
	let reading: &[&str] = if read { &["PgUp/PgDn read"] } else { &[] };
	if let Some(form) = &view.form {
		return form_foot(state, form, width, reading);
	}
	let mut buttons: Line = Vec::new();
	for (index, choice) in view.choices.iter().enumerate() {
		if index > 0 {
			buttons.push(Segment::plain("  "));
		}
		buttons.push(if index == state.picked {
			Segment::new(format!(" {choice} "), palette().bg0).on(palette().blue).bold()
		} else {
			Segment::new(format!(" {choice} "), palette().fg).on(palette().bg2)
		});
	}
	let mut lines: Vec<Line> = vec![buttons, Vec::new()];
	let picked = view.choices.get(state.picked).map_or("", String::as_str);
	let mut parts = strings(&["←→ choose"]);
	match &view.code {
		Some(code) if coded(state) => {
			lines.push(vec![
				Segment::new("Type ", palette().fg),
				Segment::new(code.as_str(), palette().yellow).bold(),
				Segment::new(format!(" for {picked}"), palette().fg),
			]);
			let echo = format!("{:_<width$}", state.typed, width = code.chars().count());
			lines.push(vec![Segment::new("> ", palette().fg3), Segment::new(echo, palette().yellow).bold()]);
			lines.extend(note);
			lines.push(Vec::new());
		}
		_ => {
			if !note.is_empty() {
				lines.extend(note);
				lines.push(Vec::new());
			}
			parts.push(format!("Enter {picked}"));
		}
	}
	parts.extend(strings(reading));
	parts.push(format!("Esc {}", view.fallback));
	lines.push(hint(&parts));
	lines.iter().map(|l| fit(l, width)).collect()
}

/// The popup's lines for `width` × `height` (0578): the foot first, then the title, and the message and detail in the
/// rows left. A text taller than that shows from `state.scroll` (clamped here) and ends with `… N more lines · PgDn
/// reads on`; long words are split, never cut.
pub fn ask_lines(state: &mut AskState, width: usize, height: usize) -> Vec<Line> {
	let view = &state.view;
	let mut text: Vec<Line> =
		wrap(&view.message, width).into_iter().map(|l| vec![Segment::new(l, palette().fg)]).collect();
	if let Some(detail) = view.detail.as_deref().filter(|d| !d.is_empty()) {
		text.push(Vec::new());
		text.extend(wrap(detail, width).into_iter().map(|l| vec![Segment::new(l, palette().fg2)]));
	}
	let head: Vec<Line> = vec![fit(&[Segment::new(view.title.as_str(), palette().blue).bold()], width), Vec::new()];
	let mut bottom = foot(state, width, false);
	// The rows for the text: everything but the title, its blank line, the blank line above the foot and the foot.
	let rows_for = |bottom: &[Line]| height as i64 - head.len() as i64 - 1 - bottom.len() as i64;
	let mut room = rows_for(&bottom);
	if text.len() as i64 > room {
		bottom = foot(state, width, true);
		room = rows_for(&bottom);
	}
	let mut shown = text.clone();
	if text.len() as i64 > room {
		let last = (text.len() as i64 - room).max(0) as usize;
		state.scroll = state.scroll.min(last);
		if state.scroll >= last {
			shown = text.split_off(last.min(text.len()));
		} else {
			let rows = (room - 1).max(0) as usize;
			let more = text.len() - state.scroll - rows.min(text.len() - state.scroll);
			shown = text.iter().skip(state.scroll).take(rows).cloned().collect();
			shown.push(fit(
				&[
					Segment::new(format!("… {more} more {}", if more == 1 { "line" } else { "lines" }), palette().fg3),
					Segment::new(" · ", palette().comment),
					Segment::new("PgDn reads on", palette().fg3),
				],
				width,
			));
		}
	} else {
		state.scroll = 0;
	}
	let mut lines = head;
	lines.extend(shown);
	lines.push(Vec::new());
	lines.extend(bottom);
	// Too short even for the foot: the foot wins (the code he must type, then the hint).
	if lines.len() > height {
		lines.drain(..lines.len() - height);
	}
	lines
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
