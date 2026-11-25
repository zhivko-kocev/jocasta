//! crossterm key and mouse events → the bytes a pane's program expects (legacy, kitty CSI-u, SGR mouse, paste).

use alacritty_terminal::term::TermMode;
use ratatui::crossterm::event::{
	KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

fn mod_field(m: KeyModifiers) -> u8 {
	let mut bits = 0;
	if m.contains(KeyModifiers::SHIFT) {
		bits |= 1;
	}
	if m.contains(KeyModifiers::ALT) {
		bits |= 2;
	}
	if m.contains(KeyModifiers::CONTROL) {
		bits |= 4;
	}
	if m.contains(KeyModifiers::SUPER) {
		bits |= 8;
	}
	if m.contains(KeyModifiers::HYPER) {
		bits |= 16;
	}
	if m.contains(KeyModifiers::META) {
		bits |= 32;
	}
	1 + bits
}

fn kind_number(kind: KeyEventKind) -> u8 {
	match kind {
		KeyEventKind::Press => 1,
		KeyEventKind::Repeat => 2,
		KeyEventKind::Release => 3,
	}
}

/// `CSI <body> <final>` with the modifier and event-type field kitty appends after `;`.
fn csi(lead: &str, mods: u8, event: Option<u8>, last: char) -> Vec<u8> {
	let lead = if mods == 1 && event.is_none() && last != '~' && last != 'u' { "" } else { lead };
	let tail = match (mods, event) {
		(1, None) => String::new(),
		(m, None) => format!(";{m}"),
		(m, Some(e)) => format!(";{m}:{e}"),
	};
	format!("\x1b[{lead}{tail}{last}").into_bytes()
}

/// The function-key / navigation keys: (CSI lead number, final).
fn functional(code: KeyCode) -> Option<(&'static str, char)> {
	Some(match code {
		KeyCode::Up => ("1", 'A'),
		KeyCode::Down => ("1", 'B'),
		KeyCode::Right => ("1", 'C'),
		KeyCode::Left => ("1", 'D'),
		KeyCode::Home => ("1", 'H'),
		KeyCode::End => ("1", 'F'),
		KeyCode::Insert => ("2", '~'),
		KeyCode::Delete => ("3", '~'),
		KeyCode::PageUp => ("5", '~'),
		KeyCode::PageDown => ("6", '~'),
		KeyCode::F(1) => ("1", 'P'),
		KeyCode::F(2) => ("1", 'Q'),
		KeyCode::F(3) => ("1", 'R'),
		KeyCode::F(4) => ("1", 'S'),
		KeyCode::F(5) => ("15", '~'),
		KeyCode::F(6) => ("17", '~'),
		KeyCode::F(7) => ("18", '~'),
		KeyCode::F(8) => ("19", '~'),
		KeyCode::F(9) => ("20", '~'),
		KeyCode::F(10) => ("21", '~'),
		KeyCode::F(11) => ("23", '~'),
		KeyCode::F(12) => ("24", '~'),
		_ => return None,
	})
}

/// Bytes for a key event; `None` when the pane's mode doesn't want it (a release in legacy mode, say).
pub fn encode_key(ev: &KeyEvent, mode: TermMode) -> Option<Vec<u8>> {
	if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) { kitty(ev, mode) } else { legacy(ev, mode) }
}

fn kitty(ev: &KeyEvent, mode: TermMode) -> Option<Vec<u8>> {
	let all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
	let events = mode.contains(TermMode::REPORT_EVENT_TYPES);
	let release = ev.kind == KeyEventKind::Release;
	let event = (events && ev.kind != KeyEventKind::Press).then(|| kind_number(ev.kind));
	let mut modifiers = ev.modifiers;
	let mods = |m: KeyModifiers| mod_field(m);

	if let Some((lead, last)) = functional(ev.code) {
		if release && !events {
			return None;
		}
		return Some(csi(lead, mods(modifiers), event, last));
	}
	match ev.code {
		KeyCode::Char(c) => {
			let plain = !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
			if plain && !all {
				// Text is sent as text, with no release.
				return (!release).then(|| c.to_string().into_bytes());
			}
			if release && !events {
				return None;
			}
			if c.is_uppercase() {
				modifiers |= KeyModifiers::SHIFT;
			}
			let code = c.to_lowercase().next().unwrap_or(c) as u32;
			let m = mods(modifiers);
			let text = if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) && plain && !release {
				format!(";{}", c as u32)
			} else {
				String::new()
			};
			let tail = match (m, event, text.is_empty()) {
				(1, None, true) => String::new(),
				(m, None, true) => format!(";{m}"),
				(m, Some(e), true) => format!(";{m}:{e}"),
				(m, None, false) => format!(";{m}{text}"),
				(m, Some(e), false) => format!(";{m}:{e}{text}"),
			};
			Some(format!("\x1b[{code}{tail}u").into_bytes())
		}
		KeyCode::Enter | KeyCode::Tab | KeyCode::Backspace | KeyCode::BackTab => {
			let (code, legacy_byte): (u32, &[u8]) = match ev.code {
				KeyCode::Enter => (13, b"\r"),
				KeyCode::Tab => (9, b"\t"),
				KeyCode::BackTab => (9, b"\x1b[Z"),
				_ => (127, b"\x7f"),
			};
			if ev.code == KeyCode::BackTab {
				modifiers |= KeyModifiers::SHIFT;
			}
			if release && !(all && events) {
				return None;
			}
			if !all && modifiers.is_empty() {
				return Some(legacy_byte.to_vec());
			}
			let m = mods(modifiers);
			let lead = code.to_string();
			Some(csi(&lead, m, event, 'u').into_iter().collect())
		}
		KeyCode::Esc => {
			if release && !events {
				return None;
			}
			Some(csi("27", mods(modifiers), event, 'u'))
		}
		_ => None,
	}
}

fn legacy(ev: &KeyEvent, mode: TermMode) -> Option<Vec<u8>> {
	if ev.kind == KeyEventKind::Release {
		return None;
	}
	let m = ev.modifiers;
	let ctrl = m.contains(KeyModifiers::CONTROL);
	let alt = m.contains(KeyModifiers::ALT);
	let app = mode.contains(TermMode::APP_CURSOR);
	let mut out = Vec::new();
	let esc = |out: &mut Vec<u8>| {
		if alt {
			out.push(0x1b)
		}
	};
	if let Some((lead, last)) = functional(ev.code) {
		let field = mod_field(m);
		let ss3 = matches!(ev.code, KeyCode::F(1..=4)) || (app && "ABCDHF".contains(last));
		return Some(if field == 1 && ss3 {
			format!("\x1bO{last}").into_bytes()
		} else if field == 1 && last != '~' {
			format!("\x1b[{last}").into_bytes()
		} else if field == 1 {
			format!("\x1b[{lead}~").into_bytes()
		} else {
			format!("\x1b[{lead};{field}{last}").into_bytes()
		});
	}
	match ev.code {
		KeyCode::Char(c) => {
			esc(&mut out);
			if ctrl {
				let byte = match c {
					'a'..='z' => c as u8 - b'a' + 1,
					'A'..='Z' => c as u8 - b'A' + 1,
					' ' | '@' | '2' => 0,
					'[' | '3' => 0x1b,
					'\\' | '4' => 0x1c,
					']' | '5' => 0x1d,
					'^' | '6' => 0x1e,
					'_' | '7' | '/' => 0x1f,
					'?' | '8' => 0x7f,
					_ => return None,
				};
				out.push(byte);
			} else {
				out.extend(c.to_string().bytes());
			}
		}
		KeyCode::Enter => {
			esc(&mut out);
			out.push(b'\r');
		}
		KeyCode::Tab => {
			esc(&mut out);
			out.push(b'\t');
		}
		KeyCode::BackTab => out.extend(b"\x1b[Z"),
		KeyCode::Backspace => {
			esc(&mut out);
			out.push(if ctrl { 0x08 } else { 0x7f });
		}
		KeyCode::Esc => out.push(0x1b),
		_ => return None,
	}
	Some(out)
}

/// A mouse event for the pane's program, if its mode asks for that kind; `col` and `row` are 0-based in the pane.
pub fn encode_mouse(ev: &MouseEvent, col: u16, row: u16, mode: TermMode) -> Option<Vec<u8>> {
	if !mode.intersects(TermMode::MOUSE_MODE) {
		return None;
	}
	let button = |b: MouseButton| match b {
		MouseButton::Left => 0,
		MouseButton::Middle => 1,
		MouseButton::Right => 2,
	};
	let sgr = mode.contains(TermMode::SGR_MOUSE);
	let (mut cb, release) = match ev.kind {
		MouseEventKind::Down(b) => (button(b), false),
		MouseEventKind::Up(b) => (if sgr { button(b) } else { 3 }, true),
		MouseEventKind::Drag(b) => {
			if !mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) {
				return None;
			}
			(button(b) + 32, false)
		}
		MouseEventKind::Moved => {
			if !mode.contains(TermMode::MOUSE_MOTION) {
				return None;
			}
			(35, false)
		}
		MouseEventKind::ScrollUp => (64, false),
		MouseEventKind::ScrollDown => (65, false),
		MouseEventKind::ScrollLeft => (66, false),
		MouseEventKind::ScrollRight => (67, false),
	};
	if ev.modifiers.contains(KeyModifiers::SHIFT) {
		cb += 4;
	}
	if ev.modifiers.contains(KeyModifiers::ALT) {
		cb += 8;
	}
	if ev.modifiers.contains(KeyModifiers::CONTROL) {
		cb += 16;
	}
	if sgr {
		Some(format!("\x1b[<{cb};{};{}{}", col + 1, row + 1, if release { 'm' } else { 'M' }).into_bytes())
	} else if col < 222 && row < 222 {
		Some(vec![0x1b, b'[', b'M', 32 + cb, 33 + col as u8, 33 + row as u8])
	} else {
		None
	}
}

/// A paste, in one write: wrapped in the bracketed-paste markers when the program asked, else with newlines as CR.
pub fn encode_paste(text: &str, mode: TermMode) -> Vec<u8> {
	if mode.contains(TermMode::BRACKETED_PASTE) {
		let clean = text.replace("\x1b[201~", "");
		let mut out = Vec::with_capacity(clean.len() + 12);
		out.extend(b"\x1b[200~");
		out.extend(clean.as_bytes());
		out.extend(b"\x1b[201~");
		out
	} else {
		text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
	}
}

/// The bytes for keys named as the service names them (`esc`, `enter`, `f9`, one character), each pressed and
/// released, as the pane's program has asked to hear keys (`mode`); the one input that goes through the terminal, the
/// push-to-talk key (0556, 0675). An unknown name is the error.
pub fn bytes_for_names(names: &[String], mode: TermMode) -> Result<Vec<u8>, String> {
	let mut out = Vec::new();
	for name in names {
		let lower = name.to_ascii_lowercase();
		let code = match lower.as_str() {
			"esc" | "escape" => KeyCode::Esc,
			"enter" | "return" => KeyCode::Enter,
			"tab" => KeyCode::Tab,
			"space" => KeyCode::Char(' '),
			other => match other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
				Some(n @ 1..=12) => KeyCode::F(n),
				_ => {
					let mut chars = other.chars();
					match (chars.next(), chars.next()) {
						(Some(c), None) => KeyCode::Char(c),
						_ => return Err(format!("unknown key `{name}`")),
					}
				}
			},
		};
		for kind in [KeyEventKind::Press, KeyEventKind::Release] {
			let event = KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind);
			out.extend(encode_key(&event, mode).unwrap_or_default());
		}
	}
	Ok(out)
}
