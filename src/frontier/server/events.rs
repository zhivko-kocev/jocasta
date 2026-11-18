//! What the service says (0405, 0407, 0554): `[event]` lines into the boss pane, held while the boss works and in quiet
//! hours (0298), and messages to the clients, a toast also raised as a Windows notification through herdr.
//!
//! The queue, the unshown notifications and their flags sit behind one lock that is never held across an `.await` and
//! never while a part is called. The boss pane's state is read and changed through its [`BossRef`]; the service's
//! `stop` waits for [`Events::settled`].

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use regex::Regex;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::frontier::protocol::{ServerMessage, ToastLevel, ToastPriority, ToastReason};
use crate::frontier::server::context::{BossRef, Calls, Link};
use crate::frontier::server::day::AlertDetails;
use crate::frontier::server::herdr::{AgentState, Note, NotificationResult, Sound, pane_text};
use crate::frontier::server::thread::{ThreadRef, is_idle, settled, subject};
use crate::frontier::server::wording::{cut, quoted};
use crate::shared::clock::now_ms;
use crate::shared::link::web_link;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;

/// herdr's notification limits (0554): a title of 80 characters, a body of 240.
const NOTIFICATION_TITLE: usize = 80;
const NOTIFICATION_BODY: usize = 240;
/// How long the start's unshown notifications wait for his terminal to attach before they're dropped (0584).
const UNSHOWN_WAIT_MS: i64 = 10 * 60_000;

/// The boss pane as the service keeps it (0404).
#[derive(Debug, Clone, PartialEq)]
pub struct BossPane {
	pub pane_id: String,
	pub workspace_id: String,
	pub state: AgentState,
	pub session_path: Option<String>,
	/// When herdr last pushed its state.
	pub event_at: Option<i64>,
	/// When the service last put a line into it.
	pub prompted_at: Option<i64>,
}

/// Whether a queued `[event]` line still holds when the boss is idle again (P8).
pub type Holds = Arc<dyn Fn() -> bool + Send + Sync>;

/// A toast message's fields: the `ServerMessage::Toast` variant, which a notification is raised from.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
	pub reason: ToastReason,
	pub level: ToastLevel,
	pub text: String,
	pub pane_id: Option<String>,
	pub priority: Option<ToastPriority>,
	pub url: Option<String>,
}

impl Toast {
	/// A toast of `reason` and `level` with only its text.
	pub fn new(reason: ToastReason, level: ToastLevel, text: impl Into<String>) -> Self {
		Self { reason, level, text: text.into(), pane_id: None, priority: None, url: None }
	}

	fn of(message: &ServerMessage) -> Option<Self> {
		let ServerMessage::Toast { reason, level, text, pane_id, priority, url } = message else { return None };
		Some(Self {
			reason: *reason,
			level: *level,
			text: text.clone(),
			pane_id: pane_id.clone(),
			priority: *priority,
			url: url.clone(),
		})
	}
}

impl From<Toast> for ServerMessage {
	fn from(toast: Toast) -> Self {
		Self::Toast {
			reason: toast.reason,
			level: toast.level,
			text: toast.text,
			pane_id: toast.pane_id,
			priority: toast.priority,
			url: toast.url,
		}
	}
}

/// `value` with control characters (ESC, BEL, the other C0 but tab and newline, DEL, C1) removed from every string in
/// it (N-3): what the page, toasts and speech get can't carry terminal escapes from a title or a question. Keys are
/// left as they are. Where `value` doesn't survive a trip through JSON (it can't be written, or read back as the same
/// type) the result is an error: nothing is better than a message with escapes in it.
pub fn clean<T: Serialize + DeserializeOwned>(value: &T) -> Result<T, serde_json::Error> {
	serde_json::from_value(clean_json(serde_json::to_value(value)?))
}

fn clean_json(value: Value) -> Value {
	match value {
		Value::String(text) => Value::String(pane_text(&text)),
		Value::Array(items) => Value::Array(items.into_iter().map(clean_json).collect()),
		Value::Object(fields) => {
			Value::Object(fields.into_iter().map(|(key, inner)| (key, clean_json(inner))).collect())
		}
		other => other,
	}
}

/// An `[event]` line waiting for the boss to be idle (0405), with what must still hold when it goes (P8).
struct Queued {
	line: String,
	holds: Option<Holds>,
}

#[derive(Default)]
struct State {
	/// `[event]` lines waiting for the boss to be idle.
	boss_queue: Vec<Queued>,
	/// The start's notifications herdr couldn't show for want of a client, by text (in the order they came), to send
	/// again once (`resend_unshown`, 0584). Only ones raised while `starting` wait; later ones without a client are
	/// dropped, as before.
	unshown: Vec<(String, Toast)>,
	/// When the first of them was kept: they're dropped once no terminal attached within `UNSHOWN_WAIT_MS`.
	unshown_since: Option<i64>,
	/// Until herdr first answers a notification for a client, or `UNSHOWN_WAIT_MS` passed with none.
	starting: bool,
	resending: bool,
}

/// Whitespace as JavaScript's `\s` has it.
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\x{FEFF}]+").expect("the whitespace pattern"));

pub struct Events {
	link: Link,
	broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	state: Mutex<State>,
	/// Notifications and `[event]` lines sent without waiting, until herdr answered and the log line is written.
	in_flight: Calls,
}

impl Events {
	pub fn new(link: Link, broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>) -> Self {
		Self {
			link,
			broadcast,
			state: Mutex::new(State { starting: true, ..State::default() }),
			in_flight: Calls::default(),
		}
	}

	fn st(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// The boss pane's state as `today` would say it: herdr's `done` is idle.
	pub fn boss_state(&self) -> AgentState {
		self.link.ctx().boss().map_or(AgentState::Unknown, |boss| settled(boss.with(|boss| boss.state)))
	}

	/// The boss pane's state changed: an idle boss gets what waits for it.
	pub async fn boss_changed(&self, state: AgentState) {
		let Some(boss) = self.link.ctx().boss() else { return };
		let changed = boss.with(|boss| {
			let changed = boss.state != state;
			boss.state = state;
			changed
		});
		if changed && is_idle(state) {
			self.flush().await;
		}
	}

	/// A thread's change as an `[event]` line for the boss (0405), in the one format (UX-21): `[event] <project>/<title> —
	/// <what>`. In quiet hours only urgent ones go (0298); a normal one is held and goes when they end (0470's way). A
	/// line that waits for the boss and has `holds` goes only if it still holds then (beta day P8: a plan approved
	/// meanwhile isn't said as waiting).
	pub async fn thread(&self, thread: &ThreadRef, what: &str, priority: ToastPriority, holds: Option<Holds>) {
		let ctx = self.link.ctx();
		let said = format!("{} — {what}", thread.with(|thread| subject(thread)));
		// Thread lines that aren't queue rows go into today's activity (N-4); urgent ones (awaiting him) are rows.
		if priority == ToastPriority::Normal {
			ctx.day.record_event(&said);
			if ctx.day.quiet() {
				if let Err(error) = ctx.day.hold(&said) {
					ctx.warn(error);
				}
				return;
			}
		}
		self.tell(&format!("[event] {said}"), priority, holds).await;
	}

	/// One of his day's events (0299) as an `[event]` line (UX-21): its text with the parts from outside quoted (SEC-2),
	/// then ` · <url>` when the url is a plain http(s) link (SE-4): a newline or a script link from a calendar entry never
	/// reaches the boss pane.
	pub fn alert(&self, text: &str, details: &AlertDetails) {
		let mut said = SPACES.replace_all(text, " ").into_owned();
		for part in &details.outside {
			let flat = SPACES.replace_all(part, " ");
			if let Some(at) = said.find(flat.as_ref()) {
				said = format!("{}{}{}", &said[..at], quoted(&flat), &said[at + flat.len()..]);
			}
		}
		let url = web_link(details.url.as_deref()).map(|url| format!(" · {url}")).unwrap_or_default();
		let (line, priority, ctx) = (format!("[event] {said}{url}"), details.priority, self.link.ctx());
		self.in_flight.spawn(async move { ctx.events.tell(&line, priority, None).await });
	}

	/// Urgent lines go into the boss pane at once; normal ones wait until it is idle (0405). A line put in makes the boss
	/// busy at once, so the next one waits for the turn it starts (CR-08); herdr's next idle sends what waits as one
	/// prompt, without the lines that no longer hold (P8).
	pub async fn tell(&self, line: &str, priority: ToastPriority, holds: Option<Holds>) {
		// Not ready, or blocked on his answer: it waits with the rest.
		if priority == ToastPriority::Urgent
			&& let Some(boss) = self.link.ctx().boss()
			&& self.prompt_boss(&boss, line).await
		{
			return;
		}
		self.st().boss_queue.push(Queued { line: line.to_owned(), holds });
		self.flush().await;
	}

	async fn flush(&self) {
		let Some(boss) = self.link.ctx().boss() else { return };
		let taken = {
			let mut state = self.st();
			if !boss.with(|boss| is_idle(boss.state)) || state.boss_queue.is_empty() {
				return;
			}
			std::mem::take(&mut state.boss_queue)
		};
		// `holds` runs with nothing locked.
		let waiting: Vec<Queued> =
			taken.into_iter().filter(|queued| queued.holds.as_ref().is_none_or(|holds| holds())).collect();
		if waiting.is_empty() {
			return;
		}
		let text = waiting.iter().map(|queued| queued.line.as_str()).collect::<Vec<_>>().join("\n");
		if !self.prompt_boss(&boss, &text).await {
			self.st().boss_queue.splice(0..0, waiting);
		}
	}

	/// Puts `text` into the boss pane, marking it busy until herdr says otherwise (CO-7): herdr's own working event then
	/// changes nothing, and the next line waits for the turn this one starts. False when herdr refused it.
	async fn prompt_boss(&self, boss: &BossRef, text: &str) -> bool {
		let ctx = self.link.ctx();
		let (pane_id, was) = boss.with(|boss| {
			let was = boss.state;
			boss.state = AgentState::Working;
			boss.prompted_at = Some(now_ms());
			(boss.pane_id.clone(), was)
		});
		match ctx.herdr.prompt(&pane_id, text).await {
			Ok(()) => true,
			Err(error) => {
				boss.with(|boss| {
					if boss.state == AgentState::Working {
						boss.state = was;
					}
				});
				ctx.warn(error);
				false
			}
		}
	}

	/// A message to every client, cleaned (N-3); a toast is also a notification.
	pub fn send(&self, message: ServerMessage) {
		// Control characters from outside (titles, questions, reminders) stop here for every surface (N-3).
		let cleaned = match clean(&message) {
			Ok(cleaned) => cleaned,
			Err(error) => {
				self.link.ctx().warn(format!("a message to the clients couldn't be cleaned and was dropped: {error}"));
				return;
			}
		};
		(self.broadcast)(cleaned.clone());
		if let Some(toast) = Toast::of(&cleaned) {
			let ctx = self.link.ctx();
			self.in_flight.spawn(async move { ctx.events.notify(&toast, true).await });
		}
	}

	/// Resolves once every notification and line sent without waiting settled (the service's stop, before herdr closes).
	pub async fn settled(&self) {
		self.in_flight.settled().await;
	}

	/// A toast as a Windows notification through herdr (0407, 0554), and a refused key's (0552): the thread it names as
	/// the title (the text before " — "), else "Jocasta", and the rest as the body, cut to herdr's 80 and 240 characters;
	/// urgent and blocked ones with herdr's request sound. Each call and herdr's answer is one line of
	/// `<data>/notifications.jsonl`, for the benchmark's time to the notification (H2, 0543). One raised before any client
	/// was attached (`no_foreground_client`: the service started before his terminal attached) is kept and sent again
	/// once by `resend_unshown` (0584: the start's settings problems are said once). `show` false (a repeat, 0578,
	/// SEC-NET-8) is logged, not shown again.
	pub async fn notify(&self, toast: &Toast, show: bool) {
		const SEPARATOR: &str = " — ";
		let ctx = self.link.ctx();
		let split = toast.text.find(SEPARATOR).filter(|at| *at > 0);
		let title = cut(split.map_or("Jocasta", |at| &toast.text[..at]), NOTIFICATION_TITLE);
		let body = cut(split.map_or(toast.text.as_str(), |at| &toast.text[at + SEPARATOR.len()..]), NOTIFICATION_BODY);
		let urgent = toast.reason == ToastReason::Blocked || toast.priority == Some(ToastPriority::Urgent);
		let result = if show {
			let note = Note {
				title: title.clone(),
				body: Some(body.clone()),
				sound: if urgent { Sound::Request } else { Sound::None },
			};
			match ctx.herdr.notify(&note).await {
				Ok(result) => result,
				Err(error) => {
					ctx.warn(error);
					return self.after_notify(toast, "error", &title, &body);
				}
			}
		} else {
			return self.after_notify(toast, "repeat", &title, &body);
		};
		self.after_notify(toast, result_name(result), &title, &body);
	}

	fn after_notify(&self, toast: &Toast, result: &str, title: &str, body: &str) {
		{
			let mut state = self.st();
			if result == "no_foreground_client" {
				if state.starting {
					match state.unshown.iter_mut().find(|(text, _)| *text == toast.text) {
						Some(slot) => slot.1 = toast.clone(),
						None => state.unshown.push((toast.text.clone(), toast.clone())),
					}
					state.unshown_since.get_or_insert_with(now_ms);
				}
			} else if result != "repeat" {
				state.unshown.retain(|(text, _)| *text != toast.text);
				if result != "error" {
					state.starting = false;
				}
			}
		}
		let line = json!({
			"at": now_ms(),
			"paneId": toast.pane_id,
			"reason": serde_json::to_value(toast.reason).unwrap_or(Value::Null),
			"title": title,
			"body": body,
			"result": result,
		});
		let written = OpenOptions::new()
			.create(true)
			.append(true)
			.open(data_dir().join("notifications.jsonl"))
			.and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
		if let Err(error) = written {
			self.link.ctx().warn(error);
		}
	}

	/// Sends the start's notifications herdr couldn't show again, in order (each refresh tick): the first one probes;
	/// while no client was ever attached they all wait for the next tick, and once one is, each is sent once, whatever
	/// herdr answers. A service no terminal attached to within `UNSHOWN_WAIT_MS` drops them.
	pub fn resend_unshown(&self) {
		{
			let mut state = self.st();
			if state.unshown.is_empty() || state.resending {
				return;
			}
			if now_ms() - state.unshown_since.unwrap_or_else(now_ms) > UNSHOWN_WAIT_MS {
				state.starting = false;
				state.unshown.clear();
				return;
			}
			state.resending = true;
		}
		let ctx = self.link.ctx();
		self.in_flight.spawn(async move {
			ctx.events.resend_all().await;
			ctx.events.st().resending = false;
		});
	}

	async fn resend_all(&self) {
		let toasts: Vec<Toast> = self.st().unshown.iter().map(|(_, toast)| toast.clone()).collect();
		for toast in toasts {
			if self.link.ctx().stopping() {
				return;
			}
			self.notify(&toast, true).await;
			let mut state = self.st();
			if state.starting {
				return;
			}
			state.unshown.retain(|(text, _)| *text != toast.text);
		}
	}

	/// A warning toast and its log line: a problem said to him once (settings, `projects.yaml`, a missing tool).
	pub fn warning(&self, text: &str) {
		self.warning_from(text, "service");
	}

	/// [`Events::warning`] with the log line's source.
	pub fn warning_from(&self, text: &str, source: &str) {
		log(LogLevel::Warn, source, text, None);
		self.send(Toast::new(ToastReason::Event, ToastLevel::Warning, text).into());
	}
}

/// herdr's answer as the notifications file and the unshown bookkeeping name it.
fn result_name(result: NotificationResult) -> &'static str {
	match result {
		NotificationResult::Shown => "shown",
		NotificationResult::Disabled => "disabled",
		NotificationResult::RateLimited => "rate_limited",
		NotificationResult::NoForegroundClient => "no_foreground_client",
		NotificationResult::Busy => "busy",
	}
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
