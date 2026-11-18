//! His questions (0434, 0484, 0519, 0552, 0558, 0568, 0578): the close question, his Allow box, Finish's merge-or-PR
//! choice and the New thread form. Each has a one-time code, random and never listed or broadcast: only the client shell
//! it is shown in gets it (fork plan phase 5), its answer names it, and the code dies with the answer, so an answer can't
//! be forged by something that only has the service's token (the Allow box's, 0484). Allow, Merge, PR and Stop
//! everything also take three letters only their box shows (0558, 0568): herdr's socket can press keys in a client, but
//! blind keys can't type letters they can't see. One reader per question (0578): the client it reached; `native.rs`
//! keeps that rule.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use serde_json::Value;
use tokio::sync::watch;

use super::herdr::clean_text;
use crate::frontier::protocol::{PopupForm, PopupView};

/// The close question's buttons (0434, 0549); Cancel is the default, Escape's and the answer of no answer.
pub const STOP: &str = "Stop everything";
pub const CANCEL: &str = "Cancel";
/// His Allow box's buttons (0484); Deny is the default, Escape's and the answer of no answer.
pub const ALLOW: &str = "Allow";
pub const DENY: &str = "Deny";
/// Finish's choices (0426, 0544): merge into trunk or open a PR; Cancel is the default.
pub const MERGE: &str = "Merge";
pub const PR: &str = "PR";
/// New thread's button (0554).
pub const START: &str = "Start";

/// Letters a typed code is drawn from: no I or O, which read as 1 and 0.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
/// Wrong codes before a coded question counts as its fallback: no guessing through herdr's socket (0558).
pub const WRONG_CODES: u32 = 3;

fn random_bytes<const N: usize>() -> [u8; N] {
	let mut bytes = [0_u8; N];
	getrandom::fill(&mut bytes).expect("the OS has random bytes");
	bytes
}

/// 32 random bytes as hex: a question's code.
fn random_hex() -> String {
	random_bytes::<32>().iter().fold(String::with_capacity(64), |mut hex, byte| {
		let _ = write!(hex, "{byte:02x}");
		hex
	})
}

/// Three random letters he types to take a coded question's choices other than its fallback (0558, 0568).
fn typed_code() -> String {
	let limit = 256 - 256 % CODE_LETTERS.len();
	let mut code = String::new();
	while code.len() < 3 {
		for byte in random_bytes::<8>() {
			if usize::from(byte) < limit && code.len() < 3 {
				code.push(char::from(CODE_LETTERS[usize::from(byte) % CODE_LETTERS.len()]));
			}
		}
	}
	code
}

/// How many threads are working and how many await him, as the close question counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
	pub working: usize,
	pub awaiting: usize,
}

fn noun(n: usize) -> &'static str {
	if n == 1 { "thread" } else { "threads" }
}

/// The close question's text (0438, 0549): working and awaiting threads counted apart; "thread" for 1.
pub fn close_question(counts: Counts) -> String {
	let Counts { working, awaiting } = counts;
	let question = if awaiting == 0 {
		format!("Stop {working} working {}", noun(working))
	} else if working == 0 {
		format!("Stop {awaiting} {} awaiting you", noun(awaiting))
	} else {
		format!("Stop {working} working {} and {awaiting} awaiting you", noun(working))
	};
	format!("{question}? They pick up where they left off next time.")
}

fn choices(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

/// The close question (0434, 0438, 0549, 0568): his threads counted; Stop everything takes its code, Cancel one key.
/// `code` is the three letters, random when `None`.
pub fn stop_question(counts: Counts, code: Option<&str>) -> PopupView {
	PopupView {
		title: "Jocasta: Stop everything?".to_owned(),
		message: close_question(counts),
		detail: None,
		choices: choices(&[STOP, CANCEL]),
		fallback: CANCEL.to_owned(),
		code: Some(code.map_or_else(typed_code, str::to_owned)),
		form: None,
	}
}

/// His Allow box (0484, 0558): the service's question and its detail; typing its code allows, anything else denies.
pub fn allow_question(message: &str, detail: &str, code: Option<&str>) -> PopupView {
	PopupView {
		title: "Jocasta: Allow or Deny".to_owned(),
		message: message.to_owned(),
		detail: Some(detail.to_owned()),
		choices: choices(&[ALLOW, DENY]),
		fallback: DENY.to_owned(),
		code: Some(code.map_or_else(typed_code, str::to_owned)),
		form: None,
	}
}

/// Finish (0426, 0544, 0568): merge or PR for the thread his key named, each taking its code; Cancel one key.
pub fn finish_question(name: &str, detail: &str, code: Option<&str>) -> PopupView {
	PopupView {
		title: format!("Jocasta: Finish {name}"),
		message: format!("Merge {name} into its trunk, or open a PR?"),
		detail: Some(detail.to_owned()),
		choices: choices(&[MERGE, PR, CANCEL]),
		fallback: CANCEL.to_owned(),
		code: Some(code.map_or_else(typed_code, str::to_owned)),
		form: None,
	}
}

/// New thread (0554): the project and the brief; it opens in plan mode and waits for his approval.
pub fn new_thread_question(projects: &[String], project: Option<&str>) -> PopupView {
	PopupView {
		title: "Jocasta: New thread".to_owned(),
		message: "Pick the project and write the brief; the thread opens in plan mode.".to_owned(),
		detail: None,
		choices: choices(&[START, CANCEL]),
		fallback: CANCEL.to_owned(),
		code: None,
		form: Some(PopupForm { projects: projects.to_vec(), project: project.map(str::to_owned) }),
	}
}

/// How a question ended (0578): his answer (`Answered`, his Deny or Cancel included), three wrong codes
/// (`WrongCodes`), the terminal it showed in left without an answer (`Closed`), or the asker stopped waiting
/// (`Withdrawn`: the wait ran out, Jocasta is closing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupEnd {
	Answered,
	WrongCodes,
	Closed,
	Withdrawn,
}

/// The form's fields with his answer (New thread): the project and the brief.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PopupFields {
	pub project: Option<String>,
	pub brief: Option<String>,
}

#[derive(Default)]
struct EntryState {
	open: bool,
	fields: PopupFields,
	wrong: u32,
}

struct Inner {
	code: String,
	view: PopupView,
	from_key: bool,
	state: Mutex<EntryState>,
	ended: watch::Sender<Option<(String, PopupEnd)>>,
	registry: Weak<Registry>,
}

type Registry = Mutex<Vec<Popup>>;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One open question. Clones are the same question.
#[derive(Clone)]
pub struct Popup {
	inner: Arc<Inner>,
}

impl PartialEq for Popup {
	fn eq(&self, other: &Self) -> bool {
		Arc::ptr_eq(&self.inner, &other.inner)
	}
}

impl std::fmt::Debug for Popup {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Popup").field("view", &self.inner.view).field("open", &self.open()).finish_non_exhaustive()
	}
}

impl Popup {
	/// 256 random bits as hex: only what shows the question gets it.
	pub fn code(&self) -> &str {
		&self.inner.code
	}

	/// Its texts, control characters removed (N-3, SEC-NET-3).
	pub fn view(&self) -> &PopupView {
		&self.inner.view
	}

	/// Opened by one of his keys (Finish, New thread, Quit): no notification, and Allow questions go first (0578, 0581).
	pub fn from_key(&self) -> bool {
		self.inner.from_key
	}

	/// The form's fields with his answer (New thread): the project and the brief.
	pub fn fields(&self) -> PopupFields {
		locked(&self.inner.state).fields.clone()
	}

	/// Whether it still waits for its answer.
	pub fn open(&self) -> bool {
		locked(&self.inner.state).open
	}

	async fn ended(&self) -> (String, PopupEnd) {
		let mut rx = self.inner.ended.subscribe();
		match rx.wait_for(Option::is_some).await {
			Ok(ended) => ended.clone().expect("waited for an end"),
			Err(_) => unreachable!("the sender lives in the popup this future borrows"),
		}
	}

	/// His choice; the fallback (Cancel, Deny) when it goes unanswered. Resolves once the question ended.
	pub async fn answer(&self) -> String {
		self.ended().await.0
	}

	/// How it ended; resolves with `answer`.
	pub async fn how(&self) -> PopupEnd {
		self.ended().await.1
	}

	/// Ends it unanswered (`Withdrawn` unless said): the answer is the fallback and the code dies. Nothing once ended.
	pub fn close(&self, how: Option<PopupEnd>) {
		self.settle(&self.inner.view.fallback, how.unwrap_or(PopupEnd::Withdrawn));
	}

	fn settle(&self, choice: &str, end: PopupEnd) {
		{
			let mut state = locked(&self.inner.state);
			if !state.open {
				return;
			}
			state.open = false;
		}
		if let Some(registry) = self.inner.registry.upgrade() {
			locked(&registry).retain(|popup| popup != self);
		}
		self.inner.ended.send_replace(Some((choice.to_owned(), end)));
	}
}

/// What `Popups::answer` made of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerTaken {
	Taken,
	Unknown,
	Invalid,
	Wrong,
	Exhausted,
}

/// The open questions by code. A code is only ever looked up here; a used or unknown one finds nothing.
pub struct Popups {
	entries: Arc<Registry>,
}

impl Default for Popups {
	fn default() -> Self {
		Self::new()
	}
}

impl Popups {
	pub fn new() -> Self {
		Self { entries: Arc::new(Mutex::new(Vec::new())) }
	}

	fn find(&self, code: &str) -> Option<Popup> {
		locked(&self.entries).iter().find(|popup| popup.inner.code == code).cloned()
	}

	/// A new question's code and its answer; `from_key` when one of his keys opened it.
	pub fn open(&self, given: &PopupView, from_key: bool) -> Popup {
		let value = serde_json::to_value(given).map_or_else(|_| Value::Null, |value| clean_text(&value));
		let view = serde_json::from_value(value).unwrap_or_else(|_| given.clone());
		let popup = Popup {
			inner: Arc::new(Inner {
				code: random_hex(),
				view,
				from_key,
				state: Mutex::new(EntryState { open: true, ..EntryState::default() }),
				ended: watch::channel(None).0,
				registry: Arc::downgrade(&self.entries),
			}),
		};
		locked(&self.entries).push(popup.clone());
		popup
	}

	/// The texts for `code`'s question; `None` for a code that is unknown or already answered.
	pub fn view(&self, code: &str) -> Option<PopupView> {
		self.find(code).map(|popup| popup.inner.view.clone())
	}

	/// `code`'s open question, or `None`.
	pub fn get(&self, code: &str) -> Option<Popup> {
		self.find(code)
	}

	/// The wrong codes `code`'s question still takes before it counts as its fallback.
	pub fn left(&self, code: &str) -> u32 {
		self.find(code).map_or(0, |popup| WRONG_CODES.saturating_sub(locked(&popup.inner.state).wrong))
	}

	/// Takes `code`'s answer once: `Taken`, `Unknown` for a code that is wrong or already answered, `Invalid` for a
	/// choice its question doesn't offer or a form missing a field (the code stays alive for the real answer), `Wrong`
	/// for a coded question's choice other than its fallback with letters that aren't its code. The third wrong code ends
	/// it as its fallback (0558, 0568) and is `Exhausted`. A bare string is the choice alone.
	pub fn answer(&self, code: &str, given: &Value) -> AnswerTaken {
		let Some(popup) = self.find(code) else { return AnswerTaken::Unknown };
		let body = given.as_object().cloned().unwrap_or_default();
		let choice = match given {
			Value::String(choice) => Some(choice.as_str()),
			_ => body.get("choice").and_then(Value::as_str),
		};
		let view = &popup.inner.view;
		let Some(choice) = choice.filter(|choice| view.choices.iter().any(|offered| offered == choice)) else {
			return AnswerTaken::Invalid;
		};
		let text = |key: &str| body.get(key).and_then(Value::as_str);
		if let Some(want) = view.code.as_deref().filter(|_| choice != view.fallback) {
			let typed = text("typed").map(|typed| typed.trim().to_uppercase()).unwrap_or_default();
			if typed != want {
				let wrong = {
					let mut state = locked(&popup.inner.state);
					state.wrong += 1;
					state.wrong
				};
				if wrong < WRONG_CODES {
					return AnswerTaken::Wrong;
				}
				popup.settle(&view.fallback, PopupEnd::WrongCodes);
				return AnswerTaken::Exhausted;
			}
		}
		if let (Some(form), true) = (&view.form, view.choices.first().is_some_and(|first| first == choice)) {
			let project = text("project").unwrap_or("");
			let brief = text("brief").map(str::trim).unwrap_or("");
			if !form.projects.iter().any(|listed| listed == project) || brief.is_empty() {
				return AnswerTaken::Invalid;
			}
			let mut state = locked(&popup.inner.state);
			state.fields.project = Some(project.to_owned());
			state.fields.brief = Some(brief.to_owned());
		}
		popup.settle(choice, PopupEnd::Answered);
		AnswerTaken::Taken
	}

	/// The questions still waiting for an answer, oldest first.
	pub fn list(&self) -> Vec<Popup> {
		locked(&self.entries).clone()
	}

	/// Ends every open question unanswered (Jocasta is closing).
	pub fn close_all(&self) {
		for popup in self.list() {
			popup.close(None);
		}
	}
}

#[cfg(test)]
#[path = "popups_tests.rs"]
mod tests;
