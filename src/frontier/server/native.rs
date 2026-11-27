//! Jocasta's half of its own shell (0553, 0683): the service talks to the `today` dock and the question box that the
//! shell (`frontier/shell.rs`) draws, over in-process messages (`protocol.rs`'s `SHELL_*` kinds). The shell gets the
//! boss's workspace and the state feed; his questions go to it, which becomes the question's only reader (0578): an
//! answer from another client is refused, and the reader leaving ends the question as its popup closing did (`Closed`).
//! A question nobody got yet goes to the next shell that attaches. A terminal a thread's program attached (`jocasta`
//! run in a pane) never reads a question. Each question but those his own keys opened also goes out as a
//! notification with the request sound, for when the terminal isn't in front (0552, 0581).
//! The sidebar's agents list pins the boss's row first by the `pin` token `status.rs` reports on the boss pane (0667).

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::runtime::Handle;

use super::events::Toast;
use super::feed::Reply;
use super::popups::{AnswerTaken, Popup, PopupEnd};
use super::surface::{NoteUntil, SurfaceSnapshot};
use crate::frontier::protocol::{
	AnswerReply, ClientMessage, PLUGIN_ACTIONS, PluginAction, SHELL_ACTION, SHELL_ANSWER, SHELL_ANSWERED, SHELL_BOSS,
	SHELL_FEED, SHELL_GONE, SHELL_QUESTION, ServerMessage, ShellAction, ShellAnswer, ShellAnswered, ShellQuestion,
	ToastLevel, ToastReason,
};
use crate::shared::log::{LogLevel, log};
use crate::shared::proctree::descends_from;
/// Where a message to the shell goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
	/// One shell, by the id `joined` was given.
	Client(u64),
	/// Every shell.
	Shells,
}

/// Prefix of every kind of a shell message (`protocol.rs`'s `SHELL_*`).
pub const KIND_PREFIX: &str = "jocasta.";

type SendFn = Arc<dyn Fn(Target, &str, String) + Send + Sync>;

/// What the service's half of the shell needs from the process's shell: a way to send it messages and a signal for
/// every change of the surface's panes the refresh reads (0683: one process, so both are in-process calls).
#[derive(Clone)]
pub struct ShellHost {
	send: SendFn,
	changed: Arc<tokio::sync::Notify>,
}

impl ShellHost {
	pub fn new(send: SendFn, changed: Arc<tokio::sync::Notify>) -> Self {
		Self { send, changed }
	}

	/// Sends `data` as a `kind` message to `target`, from any thread.
	pub fn send(&self, target: Target, kind: &str, data: String) {
		(self.send)(target, kind, data);
	}

	/// Resolves at the surface's next change; changes while nothing waits are one wake.
	pub async fn changed(&self) {
		self.changed.notified().await;
	}
}

const NOTE_BODY: usize = 240;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn json_of<T: Serialize>(value: &T) -> String {
	serde_json::to_string(value).unwrap_or_default()
}

/// The sidebar token that pins the boss's row first in the Agents view (0667): `status.rs` reports it on the boss pane.
pub const PIN_TOKEN: &str = "pin";
/// The token's value on the boss pane.
pub const PIN_VALUE: &str = "1";

/// `text` cut to herdr's notification body: 240 UTF-16 units, the last of a cut one an ellipsis.
pub fn note_body(text: &str) -> String {
	if text.encode_utf16().count() <= NOTE_BODY {
		return text.to_owned();
	}
	let mut kept = String::new();
	let mut units = 0;
	for ch in text.chars() {
		units += ch.len_utf16();
		if units > NOTE_BODY - 1 {
			break;
		}
		kept.push(ch);
	}
	kept.push('…');
	kept
}

/// What the shell's messages need of the running server; `JocastaServer` is the real one.
pub trait NativeServer: Send + Sync {
	/// Takes an answer to question `code` (`Popups::answer`), with the wrong codes it still takes.
	fn answer(&self, code: &str, answer: &Value) -> (AnswerTaken, u32);
	/// Runs one of his thread keys on the thread of `pane_id`.
	fn key(&self, action: PluginAction, pane_id: Option<String>) -> BoxFuture<'static, ()>;
	/// The feed's snapshot a shell gets first; `None` when it failed (logged).
	fn snapshot(&self) -> Option<ServerMessage>;
	/// One message of a shell's, `reply` answering that shell only.
	fn handle(&self, message: ClientMessage, reply: Reply) -> BoxFuture<'static, ()>;
	/// Raises a question's notification: a blocked toast, so it is logged in `notifications.jsonl` like one (0699).
	fn notify(&self, toast: Toast) -> BoxFuture<'static, ()>;
}

/// The question shown now, and the client that got it.
struct Shown {
	popup: Popup,
	reader: Option<u64>,
}

#[derive(Default)]
struct State {
	/// The attached shells in the order they attached, each with whether it runs inside one of the server's panes.
	clients: Vec<(u64, bool)>,
	boss: Option<String>,
	queue: Vec<Popup>,
	current: Option<Shown>,
}

/// The service's side of herdr's client shells: see the module's text. Cheap to share; its calls from herdr's server
/// thread return at once, and their work runs on the service's runtime.
pub struct Native {
	host: ShellHost,
	runtime: Handle,
	server: OnceLock<Arc<dyn NativeServer>>,
	state: Mutex<State>,
}

impl Native {
	/// Jocasta's half of the client shell on herdr's `host`; its herdr calls run on `runtime`.
	pub fn new(host: ShellHost, runtime: Handle) -> Arc<Self> {
		Arc::new(Self { host, runtime, server: OnceLock::new(), state: Mutex::new(State::default()) })
	}

	/// The server, once it started; before that, a shell's answers and keys wait for nothing and are refused.
	pub fn attach(&self, server: Arc<dyn NativeServer>) {
		let _ = self.server.set(server);
	}

	/// Resolves at herdr's next change the refresh reads, as herdr's server reports it.
	pub fn herdr_changed(&self) -> BoxFuture<'static, ()> {
		let host = self.host.clone();
		async move { host.changed().await }.boxed()
	}

	// What the service calls.

	/// A feed message for every shell.
	pub fn feed(&self, message: &ServerMessage) {
		self.host.send(Target::Shells, SHELL_FEED, json_of(message));
	}

	/// Heals against `snapshot` (the service's refresh): the boss's workspace for every shell, whose dock shows there.
	/// Nothing without a boss pane in the snapshot.
	pub async fn heal(&self, snapshot: &SurfaceSnapshot, boss_pane_id: Option<&str>) {
		let Some(boss) = boss_pane_id.and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id)) else {
			return;
		};
		let workspace = boss.workspace_id.clone();
		let changed = {
			let mut state = locked(&self.state);
			let changed = state.boss.as_ref() != Some(&workspace);
			state.boss = Some(workspace.clone());
			changed
		};
		if changed {
			self.host.send(Target::Shells, SHELL_BOSS, json_of(&workspace));
		}
	}

	/// Shows a question: now when none is shown, else once those before it ended (Allow questions and the close
	/// question first, then those his keys opened, each oldest first; 0578).
	pub fn show(self: &Arc<Self>, popup: &Popup) {
		locked(&self.state).queue.push(popup.clone());
		let (native, popup) = (Arc::clone(self), popup.clone());
		self.runtime.spawn(async move {
			popup.answer().await;
			native.ended(&popup);
		});
		self.pump();
	}

	/// A question asked again (0578): first among its kind in the queue; the one shown now stays as it is.
	pub fn front(self: &Arc<Self>, popup: &Popup) {
		{
			let mut state = locked(&self.state);
			if state.current.as_ref().is_some_and(|shown| &shown.popup == popup) {
				return;
			}
			let Some(index) = state.queue.iter().position(|p| p == popup).filter(|index| *index > 0) else {
				return;
			};
			let moved = state.queue.remove(index);
			state.queue.insert(0, moved);
		}
		self.pump();
	}

	fn pump(self: &Arc<Self>) {
		let next = {
			let mut state = locked(&self.state);
			if state.current.is_some() {
				return;
			}
			let next = state
				.queue
				.iter()
				.find(|p| p.open() && !p.from_key())
				.or_else(|| state.queue.iter().find(|p| p.open()))
				.cloned();
			let Some(next) = next else { return };
			state.current = Some(Shown { popup: next.clone(), reader: None });
			next
		};
		// A question his own key opened shows where he looks: no notification (0581, UX-6).
		if !next.from_key() {
			self.notify(&next);
		}
		self.present(&next);
	}

	/// Shows `popup` to its reader: the first shell to attach that doesn't run inside a pane; with none, it waits for one.
	fn present(&self, popup: &Popup) {
		let reader = {
			let mut state = locked(&self.state);
			let reader = state.clients.iter().find(|(_, inside)| !inside).map(|(client, _)| *client);
			let Some(shown) = state.current.as_mut().filter(|shown| &shown.popup == popup && shown.reader.is_none())
			else {
				return;
			};
			shown.reader = reader;
			reader
		};
		let Some(reader) = reader else { return };
		let question = ShellQuestion { code: popup.code().to_owned(), view: popup.view().clone() };
		self.host.send(Target::Client(reader), SHELL_QUESTION, json_of(&question));
	}

	fn notify(&self, popup: &Popup) {
		let Some(server) = self.server.get() else { return };
		let text = format!("Jocasta asks — {}", note_body(&popup.view().message));
		let asked = popup.clone();
		let until = NoteUntil(Arc::new(move || {
			let asked = asked.clone();
			async move {
				asked.how().await;
			}
			.boxed()
		}));
		let toast = Toast::new(ToastReason::Blocked, ToastLevel::Warning, text).until(until);
		self.runtime.spawn(server.notify(toast));
	}

	fn ended(self: &Arc<Self>, popup: &Popup) {
		let shown = {
			let mut state = locked(&self.state);
			state.queue.retain(|p| p != popup);
			if state.current.as_ref().is_none_or(|shown| &shown.popup != popup) {
				return;
			}
			state.current.take()
		};
		if let Some(shown) = shown {
			self.host.send(Target::Shells, SHELL_GONE, json_of(&shown.popup.code()));
		}
		self.pump();
	}

	// What herdr's server says (its `ServerExtension` calls).

	/// A shell attached: it gets the boss's workspace and the feed's snapshot, and a question nobody got yet unless it
	/// runs inside one of the server's panes (`pid`, its process as herdr read it from the connection).
	pub fn joined(self: &Arc<Self>, client: u64, pid: Option<u32>) {
		let inside = pid.is_some_and(|pid| descends_from(pid, std::process::id()));
		if inside {
			log(
				LogLevel::Warn,
				"question",
				"a terminal attached from inside a Jocasta pane; it gets no question",
				None,
			);
		}
		let (boss, unread) = {
			let mut state = locked(&self.state);
			state.clients.push((client, inside));
			let unread = state.current.as_ref().filter(|shown| shown.reader.is_none()).map(|shown| shown.popup.clone());
			(state.boss.clone(), unread)
		};
		if let Some(boss) = boss {
			self.host.send(Target::Client(client), SHELL_BOSS, json_of(&boss));
		}
		if let Some(snapshot) = self.server.get().and_then(|server| server.snapshot()) {
			self.host.send(Target::Client(client), SHELL_FEED, json_of(&snapshot));
		}
		if let Some(popup) = unread {
			self.present(&popup);
		}
	}

	/// A shell left: a question it was showing ends as its fallback (`Closed`).
	pub fn left(&self, client: u64) {
		let closing = {
			let mut state = locked(&self.state);
			state.clients.retain(|(known, _)| *known != client);
			state.current.as_ref().filter(|shown| shown.reader == Some(client)).map(|shown| shown.popup.clone())
		};
		if let Some(popup) = closing {
			log(LogLevel::Info, "question", &format!("its terminal left unanswered: {}", popup.view().title), None);
			popup.close(Some(PopupEnd::Closed));
		}
	}

	/// A shell's message: an answer, a thread key, or a feed message.
	pub fn message(&self, client: u64, kind: &str, data: &str) {
		match kind {
			SHELL_ANSWER => self.answer(client, data),
			SHELL_ACTION => {
				if let Ok(ShellAction { action, pane_id }) = serde_json::from_str(data) {
					self.key(action, pane_id);
				}
			}
			SHELL_FEED => {
				let Some(server) = self.server.get() else { return };
				let Ok(message) = serde_json::from_str::<ClientMessage>(data) else { return };
				let host = self.host.clone();
				let reply: Reply = Arc::new(move |message| {
					host.send(Target::Client(client), SHELL_FEED, json_of(&message));
				});
				self.runtime.spawn(server.handle(message, reply));
			}
			_ => {}
		}
	}

	/// His keybind for `jocasta.<action>` with the focused pane: true when it is one of his thread keys.
	pub fn action(&self, action: &str, pane_id: Option<&str>) -> bool {
		let Some(action) =
			PLUGIN_ACTIONS.into_iter().find(|known| serde_json::to_value(known).ok() == Some(json!(action)))
		else {
			return false;
		};
		self.key(action, pane_id.filter(|id| !id.is_empty()).map(str::to_owned));
		true
	}

	fn key(&self, action: PluginAction, pane_id: Option<String>) {
		if let Some(server) = self.server.get() {
			self.runtime.spawn(server.key(action, pane_id));
		}
	}

	fn answer(&self, client: u64, data: &str) {
		let Ok(ShellAnswer { code, answer }) = serde_json::from_str(data) else { return };
		let reader = locked(&self.state).current.as_ref().filter(|shown| shown.popup.code() == code).map(|s| s.reader);
		let reply = match (reader, self.server.get()) {
			(None, _) => AnswerReply::Failed { text: "That question is over.".to_owned() },
			(Some(reader), _) if reader != Some(client) => {
				log(LogLevel::Warn, "question", "an answer came from a terminal the question wasn't shown in", None);
				AnswerReply::Failed { text: "This question is shown in another terminal.".to_owned() }
			}
			(Some(_), None) => AnswerReply::Failed { text: "Jocasta is still starting.".to_owned() },
			(Some(_), Some(server)) => match server.answer(&code, &serde_json::to_value(&answer).unwrap_or_default()) {
				(AnswerTaken::Taken, _) => AnswerReply::Taken,
				(AnswerTaken::Wrong, left) => AnswerReply::Wrong { left: Some(u64::from(left)) },
				(AnswerTaken::Exhausted, _) => AnswerReply::Exhausted,
				(AnswerTaken::Invalid, _) => AnswerReply::Failed { text: "Not one of its choices.".to_owned() },
				(AnswerTaken::Unknown, _) => AnswerReply::Failed { text: "That question is over.".to_owned() },
			},
		};
		self.host.send(Target::Client(client), SHELL_ANSWERED, json_of(&ShellAnswered { code, reply }));
	}
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
