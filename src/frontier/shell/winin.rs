//! Windows console input without crossterm's reader.
//!
//! crossterm 0.29 treats every key record whose character is a UTF-16 surrogate as the next half of a pair, key-down
//! or key-up. Windows Terminal sends a down and an up record per half, so `[high-down, high-up]` decodes as a bad pair,
//! `[low-down, low-up]` the same, and every emoji or other character outside the BMP that is typed or pasted vanishes.
//! This reader turns console input records into the same `crossterm::event::Event`s, pairing surrogates on key-down
//! only. Nothing else about the console (raw mode, mouse capture) changes: crossterm still sets those.

use std::collections::VecDeque;
use std::time::Duration;

use ratatui::crossterm::event::{
	Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

type Handle = isize;

#[repr(C)]
#[derive(Clone, Copy)]
struct InputRecord {
	event_type: u16,
	_pad: u16,
	data: [u32; 4],
}

#[repr(C)]
#[derive(Default)]
struct BufferInfo {
	size: [i16; 2],
	cursor: [i16; 2],
	attrs: u16,
	window: [i16; 4], // left, top, right, bottom
	max: [i16; 2],
}

unsafe extern "system" {
	fn GetStdHandle(which: u32) -> Handle;
	fn ReadConsoleInputW(h: Handle, buf: *mut InputRecord, len: u32, read: *mut u32) -> i32;
	fn GetNumberOfConsoleInputEvents(h: Handle, n: *mut u32) -> i32;
	fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
	fn GetConsoleScreenBufferInfo(h: Handle, info: *mut BufferInfo) -> i32;
}

const KEY_EVENT: u16 = 1;
const MOUSE_EVENT: u16 = 2;
const RESIZE_EVENT: u16 = 4;
const FOCUS_EVENT: u16 = 0x10;

const SHIFT: u32 = 0x10;
const CTRL: u32 = 0x0C;
const ALT: u32 = 0x03;

pub struct Console {
	input: Handle,
	output: Handle,
	queue: VecDeque<Event>,
	high: Option<u16>,
	buttons: u32,
}

impl Console {
	pub fn new() -> Console {
		unsafe {
			Console {
				input: GetStdHandle(-10i32 as u32),
				output: GetStdHandle(-11i32 as u32),
				queue: VecDeque::new(),
				high: None,
				buttons: 0,
			}
		}
	}

	fn pending(&self) -> u32 {
		let mut n = 0;
		unsafe { GetNumberOfConsoleInputEvents(self.input, &mut n) };
		n
	}

	fn window(&self) -> BufferInfo {
		let mut info = BufferInfo::default();
		unsafe { GetConsoleScreenBufferInfo(self.output, &mut info) };
		info
	}

	/// Reads what is already queued in the console without waiting.
	fn fill(&mut self) {
		while self.queue.is_empty() {
			let n = self.pending().min(128);
			if n == 0 {
				return;
			}
			let mut records = [InputRecord { event_type: 0, _pad: 0, data: [0; 4] }; 128];
			let mut read = 0;
			if unsafe { ReadConsoleInputW(self.input, records.as_mut_ptr(), n, &mut read) } == 0 {
				return;
			}
			for record in &records[..read as usize] {
				self.translate(record);
			}
		}
	}

	/// The next event if one is queued; never blocks.
	pub fn try_next(&mut self) -> Option<Event> {
		self.fill();
		self.queue.pop_front()
	}

	/// Waits up to `timeout` (None = forever) for an event.
	pub fn next(&mut self, timeout: Option<Duration>) -> Option<Event> {
		loop {
			if let Some(event) = self.try_next() {
				return Some(event);
			}
			let ms = timeout.map_or(u32::MAX, |t| t.as_millis() as u32);
			if unsafe { WaitForSingleObject(self.input, ms) } != 0 {
				return None;
			}
		}
	}

	fn translate(&mut self, record: &InputRecord) {
		match record.event_type {
			KEY_EVENT => self.key(record),
			MOUSE_EVENT => self.mouse(record),
			RESIZE_EVENT => {
				let w = self.window().window;
				self.queue.push_back(Event::Resize((w[2] - w[0] + 1) as u16, (w[3] - w[1] + 1) as u16));
			}
			FOCUS_EVENT => {
				self.queue.push_back(if record.data[0] != 0 { Event::FocusGained } else { Event::FocusLost })
			}
			_ => {}
		}
	}

	fn key(&mut self, record: &InputRecord) {
		let down = record.data[0] != 0;
		let repeat = (record.data[1] & 0xFFFF).max(1);
		let vk = (record.data[1] >> 16) as u16;
		let ch = (record.data[2] >> 16) as u16;
		let state = record.data[3];
		let mut modifiers = KeyModifiers::empty();
		if state & SHIFT != 0 {
			modifiers |= KeyModifiers::SHIFT;
		}
		if state & CTRL != 0 {
			modifiers |= KeyModifiers::CONTROL;
		}
		if state & ALT != 0 {
			modifiers |= KeyModifiers::ALT;
		}
		let kind = if down { KeyEventKind::Press } else { KeyEventKind::Release };

		// Surrogates: pair on key-down only, ignore the key-up halves.
		if (0xD800..=0xDFFF).contains(&ch) {
			if !down {
				return;
			}
			match (self.high.take(), ch) {
				(None, 0xD800..=0xDBFF) => self.high = Some(ch),
				(Some(high), 0xDC00..=0xDFFF) => {
					if let Some(Ok(c)) = char::decode_utf16([high, ch]).next() {
						self.queue.push_back(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())));
						self.queue.push_back(Event::Key(KeyEvent::new_with_kind(
							KeyCode::Char(c),
							KeyModifiers::empty(),
							KeyEventKind::Release,
						)));
					}
				}
				_ => {}
			}
			return;
		}
		self.high = None;

		let code = match vk {
			0x08 => Some(KeyCode::Backspace),
			0x09 => Some(if modifiers.contains(KeyModifiers::SHIFT) { KeyCode::BackTab } else { KeyCode::Tab }),
			0x0D => Some(KeyCode::Enter),
			0x1B => Some(KeyCode::Esc),
			0x21 => Some(KeyCode::PageUp),
			0x22 => Some(KeyCode::PageDown),
			0x23 => Some(KeyCode::End),
			0x24 => Some(KeyCode::Home),
			0x25 => Some(KeyCode::Left),
			0x26 => Some(KeyCode::Up),
			0x27 => Some(KeyCode::Right),
			0x28 => Some(KeyCode::Down),
			0x2D => Some(KeyCode::Insert),
			0x2E => Some(KeyCode::Delete),
			0x70..=0x7B => Some(KeyCode::F((vk - 0x6F) as u8)),
			0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5 => None, // modifier and lock keys alone
			_ => {
				if ch == 0 {
					None
				} else if ch <= 0x1A && modifiers.contains(KeyModifiers::CONTROL) {
					Some(KeyCode::Char((ch as u8 + 0x60) as char))
				} else if ch >= 0x20 {
					// AltGr arrives as Ctrl+Alt with the character already made
					if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) {
						modifiers = KeyModifiers::empty();
					}
					char::from_u32(ch as u32).map(KeyCode::Char)
				} else {
					None
				}
			}
		};
		if let Some(code) = code {
			for _ in 0..repeat {
				self.queue.push_back(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)));
			}
		}
	}

	fn mouse(&mut self, record: &InputRecord) {
		let x = (record.data[0] & 0xFFFF) as i16;
		let y = (record.data[0] >> 16) as i16;
		let buttons = record.data[1];
		let state = record.data[2];
		let flags = record.data[3];
		let window = self.window().window;
		let (column, row) = (x.max(0) as u16, (y - window[1]).max(0) as u16);
		let mut modifiers = KeyModifiers::empty();
		if state & SHIFT != 0 {
			modifiers |= KeyModifiers::SHIFT;
		}
		if state & CTRL != 0 {
			modifiers |= KeyModifiers::CONTROL;
		}
		if state & ALT != 0 {
			modifiers |= KeyModifiers::ALT;
		}
		let button_of = |bits: u32| {
			if bits & 0x1 != 0 {
				Some(MouseButton::Left)
			} else if bits & 0x2 != 0 {
				Some(MouseButton::Right)
			} else if bits & 0x4 != 0 {
				Some(MouseButton::Middle)
			} else {
				None
			}
		};
		let kind = match flags {
			0 => {
				let changed = self.buttons ^ buttons;
				let kind = match button_of(changed) {
					Some(button) if buttons & changed != 0 => Some(MouseEventKind::Down(button)),
					Some(button) => Some(MouseEventKind::Up(button)),
					None => None,
				};
				self.buttons = buttons;
				kind
			}
			1 => {
				self.buttons = buttons;
				Some(match button_of(buttons) {
					Some(button) => MouseEventKind::Drag(button),
					None => MouseEventKind::Moved,
				})
			}
			4 => Some(if (buttons as i32) >> 16 > 0 { MouseEventKind::ScrollUp } else { MouseEventKind::ScrollDown }),
			_ => None,
		};
		if let Some(kind) = kind {
			self.queue.push_back(Event::Mouse(MouseEvent { kind, column, row, modifiers }));
		}
	}
}

type CloseHook = Box<dyn Fn() + Send + Sync>;

static ON_CLOSE: std::sync::OnceLock<CloseHook> = std::sync::OnceLock::new();

unsafe extern "system" {
	fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
}

const CTRL_CLOSE_EVENT: u32 = 2;
const CTRL_LOGOFF_EVENT: u32 = 5;
const CTRL_SHUTDOWN_EVENT: u32 = 6;

/// How long the handler holds the system back: Windows ends the process when the handler returns, and a closing window
/// gives it seconds at most anyway, so this only keeps the handler from ending the process before the shell did.
const CLOSE_GRACE: Duration = Duration::from_secs(30);

unsafe extern "system" fn closing(kind: u32) -> i32 {
	if !matches!(kind, CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
		return 0;
	}
	if let Some(hook) = ON_CLOSE.get() {
		hook();
	}
	std::thread::sleep(CLOSE_GRACE);
	1
}

/// Runs `hook` when the console is closing (his terminal window closed, he logged off, Windows shuts down). A closed
/// window sends no input record and ends no read, so this event is the only word the process gets; it then has seconds
/// to leave before Windows ends it. The handler waits (up to [`CLOSE_GRACE`]) so that the process ends by itself.
pub fn on_close(hook: impl Fn() + Send + Sync + 'static) {
	if ON_CLOSE.set(Box::new(hook)).is_ok() {
		unsafe { SetConsoleCtrlHandler(Some(closing), 1) };
	}
}
