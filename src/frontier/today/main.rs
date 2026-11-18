//! `jocasta plugin today` (the Rust port of `main.ts`): the `today` pane (0551, 0555, 0581), a terminal program the
//! `jocasta` herdr plugin opens beside the boss. It draws the service's feed (`snapshot`, then `today`, `hud`,
//! `dashboard`), redraws every 30 s for the clock, and takes his keys: up/down pick a row, Enter goes to a thread's
//! pane or opens a link row, f, s, r and n Finish, Stop, Review and start a New thread (0544, 0554, 0581).

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ratatui::crossterm::event::KeyEvent;
use tokio::task::JoinHandle;

use crate::frontier::ask::view::Key;
use crate::frontier::client::{StateFeed, post_action, state_feed};
use crate::frontier::protocol::{ClientMessage, PluginAction, ServerMessage};
use crate::frontier::today::view::{Pick, SAID_MS, Said, TodayState, render};
use crate::frontier::tui::{Line, Pane, PaneProgram, run_pane};
use crate::shared::clock::now_ms;
use crate::shared::errors::Result;

/// How often the pane redraws for its clock.
const CLOCK_MS: u64 = 30_000;

/// Runs the pane until herdr closes it; resolves its exit code.
pub async fn run_today() -> Result<i32> {
	run_pane(TodayProgram::new).await
}

/// What the feed's callbacks and his keys share.
struct Shared {
	state: TodayState,
	picks: Vec<Pick>,
	say_timer: Option<JoinHandle<()>>,
}

impl Shared {
	/// His last key's outcome, drawn away once it is a minute old.
	fn say(&mut self, pane: &Pane, text: String, is_error: bool) {
		self.state.said = Some(Said { text, is_error, at: now_ms() });
		if let Some(timer) = self.say_timer.take() {
			timer.abort();
		}
		let pane = pane.clone();
		self.say_timer = Some(tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(SAID_MS.unsigned_abs())).await;
			pane.draw();
		}));
	}

	fn picked(&self) -> Option<&Pick> {
		self.picks.iter().find(|pick| Some(&pick.key) == self.state.selected.as_ref())
	}
}

struct Inner {
	pane: Pane,
	shared: Mutex<Shared>,
}

impl Inner {
	fn shared(&self) -> MutexGuard<'_, Shared> {
		self.shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	fn say(&self, text: String, is_error: bool) {
		self.shared().say(&self.pane, text, is_error);
	}

	/// One service message into the state; false for those not today's to draw.
	fn apply(&self, message: ServerMessage) -> bool {
		let mut shared = self.shared();
		match message {
			ServerMessage::Snapshot { hud, today, dashboard } => {
				shared.state.hud = Some(hud);
				shared.state.today = Some(today);
				shared.state.dashboard = Some(dashboard);
			}
			ServerMessage::Today { today } => shared.state.today = Some(today),
			ServerMessage::Hud { hud } => shared.state.hud = Some(hud),
			ServerMessage::Dashboard { dashboard } => shared.state.dashboard = Some(dashboard),
			ServerMessage::Action { text, is_error } => shared.say(&self.pane, text, is_error),
			ServerMessage::Error { message } => shared.say(&self.pane, message, true),
			// Not today's to draw: notifications (herdr raises them) and the service's goodbye (the feed going down
			// says it).
			ServerMessage::Toast { .. } | ServerMessage::Bye => return false,
		}
		true
	}

	/// His key on a thread: Finish, Stop, Review, or New thread (which needs no thread: the picked one's project goes
	/// first in the form when there is one, C10).
	fn act(self: &Arc<Self>, action: PluginAction) {
		let pane_id = self.shared().picked().and_then(|pick| pick.pane_id.clone()).filter(|id| !id.is_empty());
		if pane_id.is_none() && action != PluginAction::NewThread {
			self.say("No thread to act on.".to_owned(), true);
			self.pane.draw();
			return;
		}
		let inner = Arc::clone(self);
		tokio::spawn(async move {
			if let Some(refused) = post_action(action, pane_id.as_deref()).await {
				inner.say(refused, true);
			}
			inner.pane.draw();
		});
	}
}

/// The pane's program.
struct TodayProgram {
	inner: Arc<Inner>,
	feed: StateFeed,
	clock: JoinHandle<()>,
}

impl TodayProgram {
	fn new(pane: Pane) -> Self {
		let inner = Arc::new(Inner {
			pane,
			shared: Mutex::new(Shared { state: TodayState::default(), picks: Vec::new(), say_timer: None }),
		});
		let on_message = Arc::clone(&inner);
		let on_up = Arc::clone(&inner);
		let feed = state_feed(
			move |message| {
				if on_message.apply(message) {
					on_message.pane.draw();
				}
			},
			move |up| {
				{
					let mut shared = on_up.shared();
					if up {
						shared.state.seen = true;
					} else if shared.state.up {
						// It went down now (before the first connect, the pane says it is connecting instead).
						shared.state.down_since = Some(now_ms());
					}
					shared.state.up = up;
				}
				on_up.pane.draw();
			},
		);
		let pane = inner.pane.clone();
		let clock = tokio::spawn(async move {
			let mut ticks = tokio::time::interval_at(
				tokio::time::Instant::now() + Duration::from_millis(CLOCK_MS),
				Duration::from_millis(CLOCK_MS),
			);
			loop {
				ticks.tick().await;
				pane.draw();
			}
		});
		Self { inner, feed, clock }
	}

	/// Up (-1) or down (+1) one pick; the selection stays when there is none to move to.
	fn step(&self, down: bool) {
		let mut shared = self.inner.shared();
		let at = shared.picks.iter().position(|pick| Some(&pick.key) == shared.state.selected.as_ref());
		let at = at.map_or(-1, |at| isize::try_from(at).unwrap_or(-1));
		let last = isize::try_from(shared.picks.len()).unwrap_or(0) - 1;
		let to = if down { (at + 1).min(last) } else { (at - 1).max(0) };
		let key = usize::try_from(to).ok().and_then(|to| shared.picks.get(to)).map(|pick| pick.key.clone());
		if key.is_some() {
			shared.state.selected = key;
		}
	}

	/// Enter: goes to the picked thread's pane, or opens the picked link row.
	fn enter(&self) -> bool {
		let Some(pick) = self.inner.shared().picked().cloned() else { return false };
		let message = match pick.pane_id.filter(|id| !id.is_empty()) {
			Some(pane_id) => ClientMessage::PaneFocus { pane_id },
			None => ClientMessage::LinkOpen { url: pick.url.unwrap_or_default() },
		};
		if !self.feed.send(&message) {
			self.inner.say("Not connected; try again.".to_owned(), true);
		}
		true
	}

	fn stop(&self) {
		self.clock.abort();
		if let Some(timer) = self.inner.shared().say_timer.take() {
			timer.abort();
		}
		self.feed.close();
	}
}

impl Drop for TodayProgram {
	fn drop(&mut self) {
		self.stop();
	}
}

impl PaneProgram for TodayProgram {
	fn lines(&mut self, width: u16, height: u16) -> Vec<Line> {
		let (width, height) = (usize::from(width), usize::from(height));
		let mut shared = self.inner.shared();
		let mut drawn = render(&shared.state, now_ms(), width, height);
		if !drawn.picks.iter().any(|pick| Some(&pick.key) == shared.state.selected.as_ref()) {
			// The picked row went (or none was picked yet): the first thread, else the first row Enter acts on.
			let first = drawn.picks.iter().find(|pick| pick.pane_id.as_ref().is_some_and(|id| !id.is_empty()));
			let key = first.or(drawn.picks.first()).map(|pick| pick.key.clone());
			if key != shared.state.selected {
				shared.state.selected = key;
				drawn = render(&shared.state, now_ms(), width, height);
			}
		}
		shared.picks.clone_from(&drawn.picks);
		drawn.lines
	}

	fn on_key(&mut self, event: KeyEvent) {
		let key = Key::from(event);
		let action = match key.name.as_str() {
			"f" => Some(PluginAction::Finish),
			"s" => Some(PluginAction::Stop),
			"r" => Some(PluginAction::Review),
			"n" => Some(PluginAction::NewThread),
			_ => None,
		};
		match key.name.as_str() {
			"up" | "k" => self.step(false),
			"down" | "j" => self.step(true),
			"return" | "enter" => {
				if !self.enter() {
					return;
				}
			}
			_ => match action {
				Some(action) if !key.ctrl && !key.meta => self.inner.act(action),
				_ => return,
			},
		}
		self.inner.pane.draw();
	}

	fn cleanup(&mut self) {
		self.stop();
	}
}
