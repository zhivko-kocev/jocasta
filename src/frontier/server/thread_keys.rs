//! His thread keys (0544, 0554, 0581): Finish, Stop, Review and New thread on a thread, and Quit, from the dock's keys
//! or his herdr keybinds (fork plan phase 5). What a key did is said in the dock (an `action` message), and a refusal
//! or half-done outcome also as a notification: he may be on another workspace. A notification of the same text goes
//! out once per 30 s; `notifications.jsonl` logs each (0578).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
use tokio::sync::watch;
use tokio::time::sleep;

use super::asker::{Ask, Notifier};
use super::close::CloseOutcome;
use super::events::Toast;
use super::herdr::pane_text;
use super::keys::{FinishHow, KeyFinishable, KeyOutcome, Keys, NewThreadForm};
use super::popups::{MERGE, PR, Popup, START, YES, finish_question, new_thread_question, stop_all_question};
use crate::frontier::protocol::{PluginAction, PopupView, ServerMessage, ToastLevel, ToastReason};
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};

/// One refusal notification per text this often (0578); `notifications.jsonl` still logs each.
const REFUSAL_TOAST_MS: i64 = 30_000;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn millis(ms: i64) -> Duration {
	Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// What one of his keys did (0581, UX-5): a half-done or refused outcome is an error (red, and a notification).
pub type ActionOutcome = KeyOutcome;

/// The service's thread keys (`JocastaService`'s `keyStop`, `keyReview`, `keyFinishable`, `keyFinish`, `newThreadForm`
/// and `keyStart`): what his thread keys need of it. `Keys` is the real one.
pub trait KeyService: Send + Sync {
	fn key_stop<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>>;
	fn key_review<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>>;
	fn key_finishable<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<KeyFinishable>>;
	fn key_finish<'a>(&'a self, pane_id: &'a str, how: FinishHow) -> BoxFuture<'a, Result<KeyOutcome>>;
	fn new_thread_form<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<NewThreadForm>>;
	fn key_start<'a>(&'a self, project: &'a str, brief: &'a str) -> BoxFuture<'a, Result<KeyOutcome>>;
	fn key_stop_all<'a>(&'a self) -> BoxFuture<'a, Result<String>>;
}

impl KeyService for Keys {
	fn key_stop_all<'a>(&'a self) -> BoxFuture<'a, Result<String>> {
		Box::pin(self.stop_all())
	}

	fn key_stop<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		Box::pin(self.stop(pane_id))
	}

	fn key_review<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		Box::pin(self.review(pane_id))
	}

	fn key_finishable<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<KeyFinishable>> {
		Box::pin(self.finishable(pane_id))
	}

	fn key_finish<'a>(&'a self, pane_id: &'a str, how: FinishHow) -> BoxFuture<'a, Result<KeyOutcome>> {
		Box::pin(self.finish(pane_id, how))
	}

	fn new_thread_form<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<NewThreadForm>> {
		Box::pin(Keys::new_thread_form(self, pane_id))
	}

	fn key_start<'a>(&'a self, project: &'a str, brief: &'a str) -> BoxFuture<'a, Result<KeyOutcome>> {
		Box::pin(self.start(project, brief))
	}
}

/// Closes Jocasta (or not), once he confirmed.
pub type QuitFn = Box<dyn Fn() -> BoxFuture<'static, Result<CloseOutcome>> + Send + Sync>;

/// What his thread keys need beside the service: a question, Jocasta's close and where a start line goes (0563).
pub struct ActionDeps {
	pub service: Arc<dyn KeyService>,
	/// Asks him `view`; ask it with `from_key` (no notification, after Allow questions) and the signal that ends it unanswered.
	pub ask: Ask,
	/// Brings an open question to the front of the queue (asked again).
	pub front: Box<dyn Fn(&Popup) + Send + Sync>,
	pub quit: QuitFn,
	/// A start line for a slow action, said at once (0581, UX-5).
	pub progress: Box<dyn Fn(&str) + Send + Sync>,
	/// The open questions of his keys by (action, thread): one at a time each (0578, SEC-NET-8).
	pub asking: Mutex<HashMap<String, Popup>>,
	/// How long Finish and New thread wait for his answer, as Allow does: `CONFIRM_WAIT_MS`.
	pub wait_ms: i64,
}

/// Asks `view` as (action, thread) `key`, so a repeat finds it; its answer and how it ended.
async fn ask_once(deps: &ActionDeps, key: &str, view: &PopupView) -> Popup {
	let (aborter, signal) = watch::channel(None);
	let wait = millis(deps.wait_ms);
	// `AbortSignal.timeout`.
	let timer = tokio::spawn(async move {
		sleep(wait).await;
		aborter.send_replace(Some("the wait ran out".to_owned()));
	});
	let popup = (deps.ask)(view, Some(signal), true);
	locked(&deps.asking).insert(key.to_owned(), popup.clone());
	popup.answer().await;
	timer.abort();
	let mut asking = locked(&deps.asking);
	if asking.get(key) == Some(&popup) {
		asking.remove(key);
	}
	drop(asking);
	popup
}

/// The question of `key` that is still open, brought to the front.
fn reask(deps: &ActionDeps, key: &str) -> bool {
	let open = locked(&deps.asking).get(key).filter(|popup| popup.open()).cloned();
	let Some(open) = open else { return false };
	(deps.front)(&open);
	true
}

fn said(text: impl Into<String>) -> ActionOutcome {
	ActionOutcome { text: text.into(), is_error: false }
}

async fn finish_key(pane_id: Option<&str>, deps: &ActionDeps) -> Result<Option<ActionOutcome>> {
	let finish = deps.service.key_finishable(pane_id).await?;
	let key = format!("finish:{}", finish.pane_id);
	if reask(deps, &key) {
		return Ok(Some(said(format!("Finish: already asking about {}.", finish.name))));
	}
	let popup = ask_once(deps, &key, &finish_question(&finish.name, &finish.detail, None)).await;
	let choice = popup.answer().await;
	if choice != MERGE && choice != PR {
		return Ok(Some(said(format!("Finish: {} stays as it was.", finish.name))));
	}
	if choice == MERGE {
		(deps.progress)(&format!("Merging {} into {}…", finish.name, finish.trunk));
	}
	let how = if choice == MERGE { FinishHow::Merge } else { FinishHow::Pr };
	Ok(Some(deps.service.key_finish(&finish.pane_id, how).await?))
}

async fn stop_all_key(deps: &ActionDeps) -> Result<Option<ActionOutcome>> {
	if reask(deps, "stop-all") {
		return Ok(None);
	}
	let popup = ask_once(deps, "stop-all", &stop_all_question()).await;
	if popup.answer().await != YES {
		return Ok(Some(said("Stop all: every turn runs on.")));
	}
	Ok(Some(said(deps.service.key_stop_all().await?)))
}

async fn new_thread_key(pane_id: Option<&str>, deps: &ActionDeps) -> Result<Option<ActionOutcome>> {
	if reask(deps, "new-thread") {
		return Ok(None);
	}
	let form = deps.service.new_thread_form(pane_id).await?;
	let view = new_thread_question(&form.projects, Some(&form.project));
	let popup = ask_once(deps, "new-thread", &view).await;
	let fields = popup.fields();
	let (Some(project), Some(brief)) =
		(fields.project.filter(|project| !project.is_empty()), fields.brief.filter(|brief| !brief.is_empty()))
	else {
		return Ok(Some(said("New thread: none started.")));
	};
	if popup.answer().await != START {
		return Ok(Some(said("New thread: none started.")));
	}
	(deps.progress)(&format!("Starting a thread in {project}: worktree, omp, brief…"));
	Ok(Some(deps.service.key_start(&project, &brief).await?))
}

/// One of his thread keys (0544, 0554) on the thread of `pane_id` (the focused pane, or the thread picked in `today`):
/// what it did, in one line; a refusal is `Error::User` with why. A Finish or New thread whose question is open
/// already brings that one to the front instead (0578); a New thread then says nothing more (`None`).
pub async fn run_action(
	action: PluginAction,
	pane_id: Option<&str>,
	deps: &ActionDeps,
) -> Result<Option<ActionOutcome>> {
	match action {
		PluginAction::Stop => Ok(Some(said(deps.service.key_stop(pane_id).await?))),
		PluginAction::Review => Ok(Some(said(deps.service.key_review(pane_id).await?))),
		PluginAction::Finish => finish_key(pane_id, deps).await,
		PluginAction::NewThread => new_thread_key(pane_id, deps).await,
		PluginAction::StopAll => stop_all_key(deps).await,
		PluginAction::Quit => {
			let text =
				if (deps.quit)().await? == CloseOutcome::Kept { "Jocasta keeps running." } else { "Jocasta stopped." };
			Ok(Some(said(text)))
		}
	}
}

/// What the service offers his thread keys: the keys' own methods, and the notification of a refusal.
pub trait ActionService: KeyService + Notifier {}

impl<T: KeyService + Notifier> ActionService for T {}

/// What his thread keys need.
pub struct KeyParts {
	pub service: Arc<dyn ActionService>,
	pub ask: Ask,
	/// Brings an open question to the front of the queue (asked again).
	pub front: Box<dyn Fn(&Popup) + Send + Sync>,
	pub broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	/// Jocasta's close from his quit key, asking first.
	pub quit: Arc<dyn Fn() -> BoxFuture<'static, CloseOutcome> + Send + Sync>,
	/// How long Finish and New thread wait for his answer.
	pub waiting_ms: i64,
}

/// What says a key's outcome: the log, the dock's `action` message and a refusal's notification.
struct Reporter {
	service: Arc<dyn ActionService>,
	broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	/// When each refusal text last went out as a notification (0578), in the order they did.
	refusals_shown: Mutex<Vec<(String, i64)>>,
}

impl Reporter {
	fn report(&self, outcome: ActionOutcome) {
		let ActionOutcome { text, is_error } = outcome;
		let text = pane_text(&text);
		log(if is_error { LogLevel::Warn } else { LogLevel::Info }, "keys", &text, None);
		(self.broadcast)(ServerMessage::Action { text: text.clone(), is_error });
		if !is_error {
			return;
		}
		let now = chrono::Utc::now().timestamp_millis();
		let show = {
			let mut shown = locked(&self.refusals_shown);
			shown.retain(|(_, at)| now - at < REFUSAL_TOAST_MS);
			let show = !shown.iter().any(|(shown, _)| *shown == text);
			if show {
				shown.push((text.clone(), now));
			}
			show
		};
		// Logged in notifications.jsonl as the service's toasts are (0552).
		let service = self.service.clone();
		let toast = Toast::new(ToastReason::Failed, ToastLevel::Warning, text);
		tokio::spawn(async move { service.notify(toast, show).await });
	}
}

/// His thread keys: see the module's text.
pub struct ThreadKeys {
	reporter: Arc<Reporter>,
	/// What `run_action` needs; it holds his keys' open questions by (action, thread) (0578).
	deps: ActionDeps,
}

impl ThreadKeys {
	/// His keys over `parts`: the service, his questions, the dock's messages and Quit.
	pub fn new(parts: KeyParts) -> Arc<Self> {
		let reporter = Arc::new(Reporter {
			service: Arc::clone(&parts.service),
			broadcast: parts.broadcast,
			refusals_shown: Mutex::new(Vec::new()),
		});
		let (quit, progress) = (parts.quit, Arc::clone(&reporter));
		let deps = ActionDeps {
			service: parts.service,
			ask: parts.ask,
			front: parts.front,
			quit: Box::new(move || {
				let closing = quit();
				Box::pin(async move { Ok(closing.await) })
			}),
			progress: Box::new(move |text| progress.report(said(text))),
			asking: Mutex::new(HashMap::new()),
			wait_ms: parts.waiting_ms,
		};
		Arc::new(Self { reporter, deps })
	}

	/// Runs `action` on the thread of `pane_id` and says what it did.
	pub async fn run(self: Arc<Self>, action: PluginAction, pane_id: Option<String>) {
		match run_action(action, pane_id.as_deref(), &self.deps).await {
			Ok(Some(outcome)) => self.reporter.report(outcome),
			Ok(None) => {}
			Err(error) => self.reporter.report(ActionOutcome { text: error.to_string(), is_error: true }),
		}
	}
}

#[cfg(test)]
#[path = "thread_keys_tests.rs"]
mod tests;
