//! Jocasta inside herdr's client shell (fork plan phase 5): the `today` dock beside the boss's panes (0551, 0555, 0581)
//! and his questions over everything (0553, 0558, 0568, 0578). herdr's own client draws both from the service's
//! messages, which come over herdr's client connection (`protocol.rs`'s `SHELL_*` kinds); no pane, popup or second
//! program runs for them.
//!
//! The dock draws the service's feed and takes his keys once he clicks it or presses its key: up/down pick a row, Enter
//! goes to a thread's pane or opens a link row, f, s, r and n Finish, Stop, Review and start a New thread (0544, 0554,
//! 0581), Esc gives the keys back. A question takes every key while it is up; its answer goes to the service, whose
//! reply says whether it was taken.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear, Widget};

use crate::frontier::ask::view::{AskState, Key, ask_lines, ask_state, exhausted_note, paste, press, wrong_note};
use crate::frontier::protocol::{
	AnswerReply, ClientMessage, PluginAction, PopupAnswer, SHELL_ACTION, SHELL_ANSWER, SHELL_ANSWERED, SHELL_BOSS,
	SHELL_FEED, SHELL_GONE, SHELL_QUESTION, ServerMessage, ShellAction, ShellAnswer, ShellAnswered, ShellQuestion,
};
use crate::frontier::server::wording::money;
use crate::frontier::today::view::{Pick, SAID_MS, Said, TodayState, render};
use crate::frontier::tui::{TOKYO_NIGHT, render_lines};
use crate::shared::clock::now_ms;
use crate::shared::service_file::NOT_RUNNING;
use herdr::embed::{Reply, ShellExtension};

/// Keys this soon after a question first shows are dropped: typing meant for a pane spends no tries (0578).
const KEY_GRACE_MS: i64 = 750;
/// How long the third wrong code's note shows before the question goes (0578).
const EXHAUSTED_MS: i64 = 2_000;
/// How often the dock redraws for its clock.
const CLOCK_MS: i64 = 30_000;
/// The question's box: this share of the terminal's width, and this many rows inside its border (0553).
const QUESTION_PERCENT: u16 = 70;
const QUESTION_ROWS: u16 = 18;

/// The clock, in ms since the epoch; a test's own.
pub type Now = fn() -> i64;

/// A question on screen.
struct Asking {
	code: String,
	state: AskState,
	shown_at: i64,
	/// An answer went out and its reply hasn't come.
	sending: bool,
	/// An Esc pressed while an answer was out: sent once that one wasn't taken (FE-11).
	queued: Option<PopupAnswer>,
	/// The third wrong code's note shows until then; the question goes after.
	closing_at: Option<i64>,
}

/// The shell extension herdr's client runs in bare `jocasta`'s terminal.
pub struct JocastaShell {
	now: Now,
	today: TodayState,
	picks: Vec<Pick>,
	boss: Option<String>,
	question: Option<Asking>,
	/// The clock's last drawn tick, and whether his last key's outcome was still showing then.
	ticked: (i64, bool),
}

impl Default for JocastaShell {
	fn default() -> Self {
		Self::new(now_ms)
	}
}

fn sent<T: serde::Serialize>(kind: &str, data: &T) -> (String, String) {
	(kind.to_owned(), serde_json::to_string(data).unwrap_or_default())
}

impl JocastaShell {
	/// A shell with nothing shown yet; `now` is its clock (a test's own).
	pub fn new(now: Now) -> Self {
		Self { now, today: TodayState::default(), picks: Vec::new(), boss: None, question: None, ticked: (0, false) }
	}

	fn say(&mut self, text: &str, is_error: bool) {
		self.today.said = Some(Said { text: text.to_owned(), is_error, at: (self.now)() });
	}

	/// One feed message into the dock's state; false for those not the dock's to draw.
	fn apply(&mut self, message: ServerMessage) -> bool {
		self.today.up = true;
		self.today.seen = true;
		match message {
			ServerMessage::Snapshot { hud, today, dashboard } => {
				self.today.hud = Some(hud);
				self.today.today = Some(today);
				self.today.dashboard = Some(dashboard);
			}
			ServerMessage::Today { today } => self.today.today = Some(today),
			ServerMessage::Hud { hud } => self.today.hud = Some(hud),
			ServerMessage::Dashboard { dashboard } => self.today.dashboard = Some(dashboard),
			ServerMessage::Action { text, is_error } => self.say(&text, is_error),
			ServerMessage::Error { message } => self.say(&message, true),
			// Not the dock's to draw: notifications (herdr raises them).
			ServerMessage::Toast { .. } => return false,
			ServerMessage::Bye => {
				self.today.up = false;
				self.today.down_since = Some((self.now)());
			}
		}
		true
	}

	fn picked(&self) -> Option<&Pick> {
		self.picks.iter().find(|pick| Some(&pick.key) == self.today.selected.as_ref())
	}

	/// Up or down one pick; the selection stays when there is none to move to.
	fn step(&mut self, down: bool) {
		let at = self.picks.iter().position(|pick| Some(&pick.key) == self.today.selected.as_ref());
		let to = match (at, down) {
			(None, _) => 0,
			(Some(at), true) => (at + 1).min(self.picks.len().saturating_sub(1)),
			(Some(at), false) => at.saturating_sub(1),
		};
		if let Some(pick) = self.picks.get(to) {
			self.today.selected = Some(pick.key.clone());
		}
	}

	/// His key on a thread: Finish, Stop, Review, or New thread (which needs no thread, C10).
	fn act(&mut self, action: PluginAction) -> Reply {
		let pane_id = self.picked().and_then(|pick| pick.pane_id.clone()).filter(|id| !id.is_empty());
		if pane_id.is_none() && action != PluginAction::NewThread {
			self.say("No thread to act on.", true);
			return Reply { redraw: true, ..Reply::default() };
		}
		Reply { send: vec![sent(SHELL_ACTION, &ShellAction { action, pane_id })], redraw: true, ..Reply::default() }
	}

	/// Enter: goes to the picked thread's pane, or opens the picked link row.
	fn enter(&self) -> Reply {
		let Some(pick) = self.picked().cloned() else { return Reply::default() };
		let message = match pick.pane_id.filter(|id| !id.is_empty()) {
			Some(pane_id) => ClientMessage::PaneFocus { pane_id },
			None => ClientMessage::LinkOpen { url: pick.url.unwrap_or_default() },
		};
		Reply { send: vec![sent(SHELL_FEED, &message)], ..Reply::default() }
	}

	fn question(&mut self, data: &str) -> bool {
		let Ok(ShellQuestion { code, view }) = serde_json::from_str(data) else { return false };
		let state = ask_state(view);
		let shown_at = (self.now)();
		self.question = Some(Asking { code, state, shown_at, sending: false, queued: None, closing_at: None });
		true
	}

	/// The service's reply to his answer: what shows next, and a queued Esc to send now.
	fn answered(&mut self, data: &str) -> Option<(String, String)> {
		let ShellAnswered { code, reply } = serde_json::from_str(data).ok()?;
		let now = (self.now)();
		let asking = self.question.as_mut().filter(|asking| asking.code == code)?;
		asking.sending = false;
		let fallback = asking.state.view.fallback.clone();
		asking.state.typed.clear();
		match reply {
			AnswerReply::Taken => {
				self.question = None;
				None
			}
			AnswerReply::Exhausted => {
				asking.state.note = Some(exhausted_note(&fallback));
				asking.closing_at = Some(now + EXHAUSTED_MS);
				None
			}
			AnswerReply::Wrong { left } => {
				asking.state.note = Some(wrong_note(left.and_then(|left| u32::try_from(left).ok()), &fallback));
				self.send_queued()
			}
			AnswerReply::Failed { text } => {
				asking.state.note = Some(text);
				self.send_queued()
			}
		}
	}

	fn send_queued(&mut self) -> Option<(String, String)> {
		let asking = self.question.as_mut()?;
		let answer = asking.queued.take()?;
		asking.sending = true;
		Some(sent(SHELL_ANSWER, &ShellAnswer { code: asking.code.clone(), answer }))
	}

	/// The question is over (answered elsewhere, withdrawn, Jocasta closing); the third wrong code's note stays its 2 s.
	fn gone(&mut self, data: &str) -> bool {
		let code: String = serde_json::from_str(data).unwrap_or_default();
		if self.question.as_ref().is_some_and(|asking| asking.code == code && asking.closing_at.is_none()) {
			self.question = None;
			return true;
		}
		false
	}
}

impl ShellExtension for JocastaShell {
	fn message(&mut self, kind: &str, data: &str) -> Reply {
		let redraw = |redraw| Reply { redraw, ..Reply::default() };
		match kind {
			SHELL_FEED => redraw(serde_json::from_str::<ServerMessage>(data).is_ok_and(|message| self.apply(message))),
			SHELL_BOSS => {
				let boss = serde_json::from_str::<String>(data).ok().filter(|id| !id.is_empty());
				let changed = boss != self.boss;
				self.boss = boss;
				redraw(changed)
			}
			SHELL_QUESTION => redraw(self.question(data)),
			// A queued Esc goes out once the answer before it wasn't taken.
			SHELL_ANSWERED => {
				Reply { send: self.answered(data).into_iter().collect(), redraw: true, ..Reply::default() }
			}
			SHELL_GONE => redraw(self.gone(data)),
			_ => Reply::default(),
		}
	}

	fn docks(&self, workspace_id: &str) -> bool {
		self.boss.as_deref() == Some(workspace_id)
	}

	fn draw_dock(&mut self, buffer: &mut Buffer, area: Rect, focused: bool) {
		let bar = if focused { TOKYO_NIGHT.blue } else { TOKYO_NIGHT.bg3 };
		buffer.set_style(Rect { width: 1, ..area }, Style::new().fg(bar).bg(TOKYO_NIGHT.bg0));
		for y in area.y..area.y + area.height {
			buffer.set_string(area.x, y, "▎", Style::new().fg(bar).bg(TOKYO_NIGHT.bg0));
		}
		let inner = Rect { x: area.x + 1, width: area.width.saturating_sub(1), ..area };
		let (width, height) = (usize::from(inner.width), usize::from(inner.height));
		let now = (self.now)();
		let mut drawn = render(&self.today, now, width, height);
		if !drawn.picks.iter().any(|pick| Some(&pick.key) == self.today.selected.as_ref()) {
			// The picked row went (or none was picked yet): the first thread, else the first row Enter acts on.
			let first = drawn.picks.iter().find(|pick| pick.pane_id.as_ref().is_some_and(|id| !id.is_empty()));
			let key = first.or(drawn.picks.first()).map(|pick| pick.key.clone());
			if key != self.today.selected {
				self.today.selected = key;
				drawn = render(&self.today, now, width, height);
			}
		}
		self.picks.clone_from(&drawn.picks);
		render_lines(buffer, inner, &drawn.lines);
	}

	fn dock_key(&mut self, key: KeyEvent) -> Reply {
		let key = Key::from(key);
		if key.ctrl || key.meta {
			return Reply { pass: true, ..Reply::default() };
		}
		match key.name.as_str() {
			"up" | "k" => self.step(false),
			"down" | "j" => self.step(true),
			"return" | "enter" => return self.enter(),
			"escape" => return Reply { release: true, redraw: true, ..Reply::default() },
			"f" => return self.act(PluginAction::Finish),
			"s" => return self.act(PluginAction::Stop),
			"r" => return self.act(PluginAction::Review),
			"n" => return self.act(PluginAction::NewThread),
			_ => return Reply { pass: true, ..Reply::default() },
		}
		Reply { redraw: true, ..Reply::default() }
	}

	fn asking(&self) -> bool {
		self.question.is_some()
	}

	fn draw_question(&mut self, buffer: &mut Buffer, area: Rect) -> Rect {
		let Some(asking) = self.question.as_mut() else { return Rect::default() };
		let width = (u32::from(area.width) * u32::from(QUESTION_PERCENT) / 100).max(44).min(u32::from(area.width));
		let width = u16::try_from(width).unwrap_or(area.width);
		let height = (QUESTION_ROWS + 2).min(area.height);
		let outer =
			Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height };
		Clear.render(outer, buffer);
		Block::default()
			.borders(Borders::ALL)
			.border_style(Style::new().fg(TOKYO_NIGHT.blue))
			.title(" Jocasta asks ")
			.style(Style::new().bg(TOKYO_NIGHT.bg0))
			.render(outer, buffer);
		let inner = Rect {
			x: outer.x + 2,
			y: outer.y + 1,
			width: outer.width.saturating_sub(4),
			height: outer.height.saturating_sub(2),
		};
		let lines = ask_lines(&mut asking.state, usize::from(inner.width).max(20), usize::from(inner.height));
		render_lines(buffer, inner, &lines);
		outer
	}

	fn question_key(&mut self, key: KeyEvent) -> Reply {
		let now = (self.now)();
		let Some(asking) = self.question.as_mut() else { return Reply::default() };
		if asking.closing_at.is_some() || now - asking.shown_at < KEY_GRACE_MS {
			return Reply::default();
		}
		let Some(answer) = press(&mut asking.state, &Key::from(key)) else {
			return Reply { redraw: true, ..Reply::default() };
		};
		if asking.sending {
			if answer.choice == asking.state.view.fallback && answer.typed.is_none() {
				asking.queued = Some(answer);
			}
			return Reply { redraw: true, ..Reply::default() };
		}
		asking.sending = true;
		let code = asking.code.clone();
		Reply { send: vec![sent(SHELL_ANSWER, &ShellAnswer { code, answer })], redraw: true, ..Reply::default() }
	}

	fn question_paste(&mut self, text: &str) -> Reply {
		let Some(asking) = self.question.as_mut() else { return Reply::default() };
		paste(&mut asking.state, &crate::frontier::tui::strip_ansi(text));
		Reply { redraw: true, ..Reply::default() }
	}

	fn tick(&mut self) -> bool {
		let now = (self.now)();
		let mut redraw = false;
		if self.question.as_ref().and_then(|asking| asking.closing_at).is_some_and(|at| now >= at) {
			self.question = None;
			redraw = true;
		}
		let saying = self.today.said.as_ref().is_some_and(|said| now - said.at < SAID_MS);
		let tick = now / CLOCK_MS;
		if (tick, saying) != self.ticked {
			self.ticked = (tick, saying);
			redraw |= self.boss.is_some();
		}
		redraw
	}

	/// The HUD's figures beside herdr's clock (0403): Awaiting you's rows and today's spend, `7 awaiting you · $48.73
	/// today`; the one text for a service that stopped (0581 UX-13); none before the first message.
	fn status(&self) -> Option<String> {
		if !self.today.up {
			return self.today.seen.then(|| NOT_RUNNING.to_owned());
		}
		let hud = self.today.hud.as_ref()?;
		let spend = hud.spend_today.map_or_else(|| "…".to_owned(), money);
		Some(format!("{} awaiting you · {spend} today", hud.awaiting))
	}
}

/// Registers the shell extension with herdr's client for this process.
pub fn register() {
	herdr::embed::set_shell_extension(Box::new(JocastaShell::default()));
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
