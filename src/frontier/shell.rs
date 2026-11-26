//! Jocasta's own shell (0683): the `today` dock beside the boss's tab (0551, 0555, 0581) and his questions over
//! everything (0553, 0558, 0568, 0578), drawn from the service's messages (`protocol.rs`'s `SHELL_*` kinds, in-process);
//! no pane, popup or second program runs for them. `shell/` holds the terminal side: panes on `alacritty_terminal`, tabs,
//! the sidebar and the event loop.
//!
//! The dock draws the service's feed and takes his keys once he clicks it or presses its key: up/down pick a row (at the
//! NEWS box's edge they scroll it, as does the wheel over it), Enter goes to a thread's pane or opens a link row, f, s, r
//! and n Finish, Stop, Review and start a New thread (0544, 0554, 0581), x removes a picked leftover worktree behind his
//! Allow (0670), Esc gives the keys back. A question takes every key while it is up; its answer goes to the service,
//! whose reply says whether it was taken.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Widget};

use crate::frontier::ask::view::{AskState, Key, ask_lines, ask_state, exhausted_note, paste, press, wrong_note};
use crate::frontier::protocol::{
	AnswerReply, ClientMessage, NeedsYouKind, PluginAction, PopupAnswer, SHELL_ACTION, SHELL_ANSWER, SHELL_ANSWERED,
	SHELL_BOSS, SHELL_FEED, SHELL_GONE, SHELL_QUESTION, ServerMessage, ShellAction, ShellAnswer, ShellAnswered,
	ShellQuestion,
};
use crate::frontier::server::wording::money;
use crate::frontier::today::view::{NewsBox, Pick, SAID_MS, Said, TodayState, not_answering, render};
use crate::frontier::tui::{palette, render_lines};
use crate::shared::clock::now_ms;
use crate::shared::service_file::NOT_RUNNING;

/// What a shell call asks of its caller: messages for the service, whether to redraw, whether the dock gives the keys
/// back, and whether the key goes on to the pane.
#[derive(Debug, Clone, Default)]
pub struct Reply {
	pub send: Vec<(String, String)>,
	pub redraw: bool,
	pub release: bool,
	pub pass: bool,
}

/// Keys this soon after a question first shows are dropped: typing meant for a pane spends no tries (0578).
const KEY_GRACE_MS: i64 = 750;
/// How long the third wrong code's note shows before the question goes (0578).
const EXHAUSTED_MS: i64 = 2_000;
/// How often the dock redraws for its clock.
const CLOCK_MS: i64 = 30_000;
/// With no message from the service for this long it is shown as not answering, its picture dimmed (0662).
const SILENT_MS: i64 = 30_000;
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
	/// When the service last sent any message (epoch ms), and whether it went silent past [`SILENT_MS`] (0662).
	heard: i64,
	silent: bool,
	/// The dock's text area as last drawn, and the line and key of each pickable row in it (0660).
	inner: Rect,
	rows: Vec<(usize, String)>,
	/// The NEWS box as last drawn (0668).
	news: Option<NewsBox>,
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
		Self {
			now,
			today: TodayState::default(),
			picks: Vec::new(),
			boss: None,
			question: None,
			ticked: (0, false),
			heard: 0,
			silent: false,
			inner: Rect::default(),
			rows: Vec::new(),
			news: None,
		}
	}

	fn say(&mut self, text: &str, is_error: bool) {
		self.today.said = Some(Said { text: text.to_owned(), is_error, at: (self.now)() });
	}

	/// One feed message into the dock's state; false for those not the dock's to draw.
	fn apply(&mut self, message: ServerMessage) -> bool {
		// Any message from a service that was down, or not heard from, brings the dock back (0662).
		let revived = !self.today.up;
		self.today.up = true;
		self.today.seen = true;
		self.silent = false;
		self.heard = (self.now)();
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
			ServerMessage::Toast { .. } => return revived,
			// The service's heartbeat: only a dock that was down has anything to redraw.
			ServerMessage::Alive => return revived,
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

	/// One pick down (`step` 1) or up (-1); the selection stays when there is none to move to. In the NEWS box it moves
	/// through every headline, scrolling the box at its edges (0668).
	fn step(&mut self, step: isize) {
		if self.step_news(step) {
			return;
		}
		let at = self.picks.iter().position(|pick| Some(&pick.key) == self.today.selected.as_ref());
		let last = self.picks.len().saturating_sub(1);
		let to = at.map_or(0, |at| at.saturating_add_signed(step).min(last));
		if let Some(pick) = self.picks.get(to) {
			self.today.selected = Some(pick.key.clone());
		}
	}

	/// A step from a picked headline to the next or previous one, the box scrolling to keep it in view; false when
	/// the pick isn't a headline, or is the first one going up (the step then leaves the box).
	fn step_news(&mut self, step: isize) -> bool {
		let Some(news) = &self.news else { return false };
		if !self.picked().is_some_and(|pick| pick.news) {
			return false;
		}
		let Some(at) = news.keys.iter().position(|key| Some(key) == self.today.selected.as_ref()) else {
			return false;
		};
		let target = if step > 0 {
			news.keys.iter().enumerate().skip(at + 1).find(|(_, key)| !key.is_empty()).map(|(i, _)| i)
		} else {
			news.keys.iter().enumerate().take(at).rev().find(|(_, key)| !key.is_empty()).map(|(i, _)| i)
		};
		let Some(target) = target else { return step > 0 };
		let shown = news.lines.len();
		let top = if target < news.top {
			target
		} else if target >= news.top + shown {
			target + 1 - shown
		} else {
			news.top
		};
		self.today.selected = Some(news.keys[target].clone());
		self.today.news_top = top;
		true
	}

	/// The wheel over the NEWS box: one headline up or down (0668). A picked headline pushed out of view moves to the
	/// box's edge, so the pick never leaves the screen.
	fn scroll_news(&mut self, column: u16, row: u16, step: isize) -> Reply {
		let area = self.inner;
		let Some(news) = &self.news else { return Reply::default() };
		if column < area.x || column >= area.x + area.width || row < area.y {
			return Reply::default();
		}
		if !news.lines.contains(&usize::from(row - area.y)) {
			return Reply::default();
		}
		let shown = news.lines.len();
		let last = news.keys.len().saturating_sub(shown);
		let top = news.top.saturating_add_signed(step).min(last);
		if top == news.top {
			return Reply::default();
		}
		self.today.news_top = top;
		let picked = news.keys.iter().position(|key| Some(key) == self.today.selected.as_ref());
		if let Some(picked) = picked.filter(|_| self.picked().is_some_and(|pick| pick.news)) {
			let edge = picked.clamp(top, top + shown - 1);
			if edge != picked {
				self.today.selected = Some(news.keys[edge].clone());
			}
		}
		Reply { redraw: true, ..Reply::default() }
	}

	/// `R` on a review request or `F` on a red pipeline (0680): asks the service to start the review or the fix thread
	/// of the picked row's link; any other row says what to pick.
	fn start_from_row(&mut self, kind: NeedsYouKind) -> Reply {
		let url = self.picked().filter(|pick| pick.kind == Some(kind)).and_then(|pick| pick.url.clone());
		let Some(url) = url else {
			let wanted = if kind == NeedsYouKind::Review { "a review request" } else { "a red pipeline" };
			self.say(&format!("Pick {wanted} first."), true);
			return Reply { redraw: true, ..Reply::default() };
		};
		let message = if kind == NeedsYouKind::Review {
			self.say("Starting the review…", false);
			ClientMessage::PrReview { url }
		} else {
			self.say("Starting the fix…", false);
			ClientMessage::CiFix { url }
		};
		Reply { send: vec![sent(SHELL_FEED, &message)], redraw: true, ..Reply::default() }
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

	/// Enter: goes to the picked thread's pane, or opens the picked link row; a leftover has nothing to open.
	fn enter(&self) -> Reply {
		let Some(pick) = self.picked().cloned() else { return Reply::default() };
		let message = match (pick.pane_id.filter(|id| !id.is_empty()), pick.url) {
			(Some(pane_id), _) => ClientMessage::PaneFocus { pane_id },
			(None, Some(url)) => ClientMessage::LinkOpen { url },
			(None, None) => return Reply::default(),
		};
		Reply { send: vec![sent(SHELL_FEED, &message)], ..Reply::default() }
	}

	/// `x` on a picked leftover: asks the service to remove its folder, which it does only behind his Allow (0670).
	fn remove_leftover(&mut self) -> Reply {
		let Some(folder) = self.picked().and_then(|pick| pick.leftover.clone()) else {
			self.say("No leftover to remove.", true);
			return Reply { redraw: true, ..Reply::default() };
		};
		let message = ClientMessage::LeftoverRemove { folder: PathBuf::from(folder) };
		Reply { send: vec![sent(SHELL_FEED, &message)], redraw: true, ..Reply::default() }
	}

	/// A left click on a row: picks it and does what Enter does on it (0660).
	fn click(&mut self, column: u16, row: u16) -> Reply {
		let area = self.inner;
		if column < area.x || column >= area.x + area.width || row < area.y {
			return Reply::default();
		}
		let line = usize::from(row - area.y);
		let Some(key) = self.rows.iter().find(|(at, _)| *at == line).map(|(_, key)| key.clone()) else {
			return Reply::default();
		};
		self.today.selected = Some(key);
		Reply { redraw: true, ..self.enter() }
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

impl JocastaShell {
	pub fn message(&mut self, kind: &str, data: &str) -> Reply {
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

	pub fn docks(&self, workspace_id: &str) -> bool {
		self.boss.as_deref() == Some(workspace_id)
	}

	pub fn draw_dock(&mut self, buffer: &mut Buffer, area: Rect, focused: bool) {
		let bar = if focused { palette().blue } else { palette().bg3 };
		buffer.set_style(Rect { width: 1, ..area }, Style::new().fg(bar).bg(palette().bg0));
		for y in area.y..area.y + area.height {
			buffer.set_string(area.x, y, "▎", Style::new().fg(bar).bg(palette().bg0));
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
		self.rows.clone_from(&drawn.rows);
		self.news = drawn.news.clone();
		self.inner = inner;
		render_lines(buffer, inner, &drawn.lines);
		if self.silent {
			buffer.set_style(inner, Style::new().add_modifier(Modifier::DIM));
		}
	}

	pub fn dock_key(&mut self, key: KeyEvent) -> Reply {
		let key = Key::from(key);
		if key.ctrl || key.meta {
			return Reply { pass: true, ..Reply::default() };
		}
		// Uppercase `X` (0686) before `x`, whose name it shares: Stop all needs no picked thread.
		if key.sequence == "X" {
			let action = ShellAction { action: PluginAction::StopAll, pane_id: None };
			return Reply { send: vec![sent(SHELL_ACTION, &action)], redraw: true, ..Reply::default() };
		}
		// Uppercase `R` and `F` (0680) before `r` and `f`, whose names they share (`r` Review and `f` Finish act on a
		// thread): the service starts a review thread on a picked review request, a fix thread on a picked red pipeline.
		if key.sequence == "R" {
			return self.start_from_row(NeedsYouKind::Review);
		}
		if key.sequence == "F" {
			return self.start_from_row(NeedsYouKind::Pipeline);
		}
		match key.name.as_str() {
			// `b` (0680): the service builds today's morning brief again and opens it.
			"b" => {
				self.say("Building the morning brief…", false);
				return Reply {
					send: vec![sent(SHELL_FEED, &ClientMessage::BriefMorning)],
					redraw: true,
					..Reply::default()
				};
			}
			"up" | "k" => self.step(-1),
			"down" | "j" => self.step(1),
			"return" | "enter" => return self.enter(),
			"escape" => return Reply { release: true, redraw: true, ..Reply::default() },
			"f" => return self.act(PluginAction::Finish),
			"s" => return self.act(PluginAction::Stop),
			"r" => return self.act(PluginAction::Review),
			"n" => return self.act(PluginAction::NewThread),
			"x" => return self.remove_leftover(),
			_ => return Reply { pass: true, ..Reply::default() },
		}
		Reply { redraw: true, ..Reply::default() }
	}

	pub fn dock_mouse(&mut self, column: u16, row: u16) -> Reply {
		self.click(column, row)
	}

	pub fn dock_scroll(&mut self, column: u16, row: u16, step: isize) -> Reply {
		self.scroll_news(column, row, step)
	}

	pub fn asking(&self) -> bool {
		self.question.is_some()
	}

	pub fn draw_question(&mut self, buffer: &mut Buffer, area: Rect) -> Rect {
		let Some(asking) = self.question.as_mut() else { return Rect::default() };
		let width = (u32::from(area.width) * u32::from(QUESTION_PERCENT) / 100).max(44).min(u32::from(area.width));
		let width = u16::try_from(width).unwrap_or(area.width);
		let height = (QUESTION_ROWS + 2).min(area.height);
		let outer =
			Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height };
		Clear.render(outer, buffer);
		Block::default()
			.borders(Borders::ALL)
			.border_style(Style::new().fg(palette().blue))
			.title(" Jocasta asks ")
			.style(Style::new().bg(palette().bg0))
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

	pub fn question_key(&mut self, key: KeyEvent) -> Reply {
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

	pub fn question_paste(&mut self, text: &str) -> Reply {
		let Some(asking) = self.question.as_mut() else { return Reply::default() };
		paste(&mut asking.state, &crate::frontier::tui::strip_ansi(text));
		Reply { redraw: true, ..Reply::default() }
	}

	pub fn tick(&mut self) -> bool {
		let now = (self.now)();
		let mut redraw = false;
		if self.question.as_ref().and_then(|asking| asking.closing_at).is_some_and(|at| now >= at) {
			self.question = None;
			redraw = true;
		}
		// A service heard from, then not for 30 s, is shown as not answering since its last word (0662).
		if self.today.up && now - self.heard >= SILENT_MS {
			self.today.up = false;
			self.silent = true;
			self.today.down_since = Some(self.heard);
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
	pub fn status(&self) -> Option<String> {
		if !self.today.up {
			if self.silent {
				return Some(not_answering(self.today.down_since));
			}
			return self.today.seen.then(|| NOT_RUNNING.to_owned());
		}
		let hud = self.today.hud.as_ref()?;
		let spend = hud.spend_today.map_or_else(|| "…".to_owned(), money);
		Some(format!("{} awaiting you · {spend} today", hud.awaiting))
	}
}

/// The shell's frame, panes and tabs (0683).
mod app;
mod keys;
mod pty;
mod sidebar;
mod tabs;
mod term;
#[cfg(windows)]
mod winin;

pub use app::{Parts, run};
pub use keys::bytes_for_names;
pub use pty::{Pane, Ui, Wake};
pub use tabs::{AgentInfo, AgentLookup, Hub, Start};
pub use term::screen_text;

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
