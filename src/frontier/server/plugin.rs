//! The `jocasta` herdr plugin's service side (0553, 0554, 0558): the `today` pane kept open beside the boss, boss 65 /
//! `today` 35, opened again when it is gone (0518, 0549); the Agents view (blocked and done first, then working, then
//! idle; the boss's workspace left out); his questions shown one at a time in the plugin's `ask` popup, with a
//! notification and herdr's request sound unless his own key opened them (0581); and his thread keys (Finish, Stop,
//! Review, New thread, Quit).

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Display;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use futures_util::Stream;
use futures_util::future::BoxFuture;
use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio::time::sleep;

use super::boss::{BOSS_WORKSPACE, boss_home, is_today_pane};
use super::close::CloseOutcome;
use super::herdr::{AgentView, Direction, Herdr, HerdrSnapshot, Note, PaneInfo, Placement, PluginPaneOptions, Sound};
use super::keys::{FinishHow, KeyFinishable, KeyOutcome, Keys, NewThreadForm};
use super::popup_routes::{Ask as AskQuestion, PopupHolder};
use super::popups::{MERGE, PR, Popup, PopupEnd, START, finish_question, new_thread_question};
use crate::frontier::protocol::{POPUP_ENV, PluginAction, PopupView};
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};

/// The plugin's id in `config/herdr-plugin/herdr-plugin.toml`.
const PLUGIN_ID: &str = "jocasta";
/// The boss's share of its tab beside `today` (0553): boss 65 / `today` 35.
const BOSS_SHARE: f64 = 0.65;
/// The first wait before `today` opens again after it went soon after opening; doubled each time, up to 5 minutes.
const TODAY_AGAIN_MS: i64 = 5_000;
const TODAY_AGAIN_MAX_MS: i64 = 5 * 60_000;
/// A `today` that lived this long went for a reason of its own: it opens again at once.
const TODAY_LIVED_MS: i64 = 30_000;
/// One warning per subject this often, so a herdr without the plugin doesn't fill the log.
const WARN_EVERY_MS: i64 = 10 * 60_000;
/// Text cut to herdr's notification body (240 characters).
const NOTE_BODY: usize = 240;
/// How long the popup has to hold its question's request before it counts as closed.
const HOLD_WAIT_MS: i64 = 10_000;
/// How long to wait before asking herdr again for a popup it couldn't open.
const RETRY_MS: i64 = 3_000;

/// The clock, in ms since the epoch (`Date.now`).
pub type Now = Arc<dyn Fn() -> i64 + Send + Sync>;

fn system_now() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The Agents view (0553, 0554): blocked and done threads first, then working, then idle, as herdr's status sort puts
/// them; the boss's workspace (the boss and `today`) left out.
pub fn agent_view(boss_workspace_id: &str) -> AgentView {
	AgentView {
		source: format!("plugin:{PLUGIN_ID}"),
		label: Some("threads".to_owned()),
		filter: Some(json!({
			"op": "not",
			"filter": {"op": "eq", "field": "workspace_id", "value": boss_workspace_id},
		})),
		sort: Some(vec![
			json!({"field": "attention", "order": "desc"}),
			json!({"field": "status", "order": "desc"}),
			json!({"field": "state_change_seq", "order": "desc"}),
		]),
	}
}

/// Applies the Agents view for the boss's workspace as herdr has it now (the plugin's startup hook, 0553); false, and
/// nothing set, before the boss's workspace exists: the service sets it once it made it.
pub async fn apply_agent_view(herdr: &dyn Herdr) -> Result<bool> {
	let Some(boss) = herdr.workspaces().await?.into_iter().find(|w| w.label == BOSS_WORKSPACE) else {
		return Ok(false);
	};
	herdr.set_agent_view(&agent_view(&boss.id)).await?;
	Ok(true)
}

/// Logs each subject's warning at most once per `WARN_EVERY_MS`.
struct Warner {
	said: Mutex<HashMap<String, i64>>,
	now: Now,
}

impl Warner {
	fn new(now: Now) -> Arc<Self> {
		Arc::new(Self { said: Mutex::new(HashMap::new()), now })
	}

	fn warn(&self, subject: &str, error: &dyn Display) {
		let now = (self.now)();
		{
			let mut said = locked(&self.said);
			if said.get(subject).is_some_and(|last| now - last < WARN_EVERY_MS) {
				return;
			}
			said.insert(subject.to_owned(), now);
		}
		log(LogLevel::Warn, "plugin", &format!("{subject}: {error}"), None);
	}
}

#[derive(Default)]
struct PanesState {
	today_id: Option<String>,
	opened_at: i64,
	next_try: i64,
	again: i64,
	view_for: Option<String>,
}

/// `today` beside the boss and the Agents view, healed on every refresh (0518, 0549, 0553).
pub struct PluginPanes {
	herdr: Arc<dyn Herdr>,
	now: Now,
	warner: Arc<Warner>,
	state: Mutex<PanesState>,
}

impl PluginPanes {
	/// `now` is the clock; the system's when `None`.
	pub fn new(herdr: Arc<dyn Herdr>, now: Option<Now>) -> Self {
		let now = now.unwrap_or_else(|| Arc::new(system_now));
		Self {
			herdr,
			warner: Warner::new(Arc::clone(&now)),
			now,
			state: Mutex::new(PanesState { again: TODAY_AGAIN_MS, ..PanesState::default() }),
		}
	}

	/// The `today` pane's id while it is open, else `None`.
	pub fn today(&self) -> Option<String> {
		locked(&self.state).today_id.clone()
	}

	/// Sets the Agents view once per boss workspace; a failure is a warning, the next refresh tries again.
	async fn set_view(&self, boss: &PaneInfo) {
		if locked(&self.state).view_for.as_deref() == Some(boss.workspace_id.as_str()) {
			return;
		}
		match self.herdr.set_agent_view(&agent_view(&boss.workspace_id)).await {
			Ok(()) => locked(&self.state).view_for = Some(boss.workspace_id.clone()),
			Err(error) => self.warner.warn("the Agents view wasn't set", &error),
		}
	}

	/// A `today` that went: at once after a long life; soon after opening, after a wait that doubles each time.
	fn note_gone(&self, now: i64) {
		let mut state = locked(&self.state);
		if state.today_id.is_none() {
			return;
		}
		let short = now - state.opened_at < TODAY_LIVED_MS;
		state.again = if short { (state.again * 2).min(TODAY_AGAIN_MAX_MS) } else { TODAY_AGAIN_MS };
		state.next_try = if short { now + state.again } else { now };
		state.today_id = None;
		drop(state);
		log(LogLevel::Info, "plugin", "today is gone; it opens again beside the boss", None);
	}

	/// Opens `today` beside the boss at 65/35; false when herdr refused (the next try waits).
	async fn open_today(&self, boss: &PaneInfo, now: i64) -> bool {
		let opened = self
			.herdr
			.open_plugin_pane(&PluginPaneOptions {
				plugin_id: PLUGIN_ID.to_owned(),
				entrypoint: "today".to_owned(),
				placement: Placement::Split,
				target_pane_id: Some(boss.pane_id.clone()),
				direction: Some(Direction::Right),
				cwd: Some(boss_home()),
				env: None,
				focus: Some(false),
			})
			.await;
		match opened {
			Ok(id) => {
				let mut state = locked(&self.state);
				state.today_id = id;
				state.opened_at = now;
				true
			}
			Err(error) => {
				{
					let mut state = locked(&self.state);
					state.next_try = now + state.again;
					state.again = (state.again * 2).min(TODAY_AGAIN_MAX_MS);
				}
				self.warner.warn("today didn't open", &error);
				false
			}
		}
	}

	/// Heals the panes against `snapshot`: the Agents view, and `today` beside the boss pane `boss_pane_id`. Nothing
	/// without a boss pane in the snapshot.
	pub async fn heal(&self, snapshot: &HerdrSnapshot, boss_pane_id: Option<&str>) {
		let Some(boss) = boss_pane_id.and_then(|id| snapshot.panes.iter().find(|p| p.pane_id == id)) else {
			return;
		};
		self.set_view(boss).await;
		let known = locked(&self.state).today_id.clone();
		let found = snapshot
			.panes
			.iter()
			.find(|p| Some(&p.pane_id) == known.as_ref())
			.or_else(|| snapshot.panes.iter().find(|p| p.workspace_id == boss.workspace_id && is_today_pane(p)));
		let now = (self.now)();
		if let Some(found) = found {
			locked(&self.state).today_id = Some(found.pane_id.clone());
			return;
		}
		self.note_gone(now);
		if now < locked(&self.state).next_try || !self.open_today(boss, now).await {
			return;
		}
		if let Err(error) = self.herdr.set_split_ratio(&boss.tab_id, BOSS_SHARE).await {
			self.warner.warn("today's width wasn't set", &error);
		}
	}
}

/// What `AskPopups` waits and how it reads the clock; each `None` is the default named.
#[derive(Default)]
pub struct AskOptions {
	/// How long the popup has to hold its question's request, or to show at all, before it is asked for again or
	/// counts as closed: 10 s.
	pub hold_wait_ms: Option<i64>,
	/// How long to wait before asking herdr again for a popup it couldn't open: 3 s.
	pub retry_ms: Option<i64>,
	/// The clock the warnings use: the system's.
	pub now: Option<Now>,
}

/// The question shown now.
struct Shown {
	popup: Popup,
	held: AtomicBool,
	held_now: watch::Sender<bool>,
	/// Ends the popup's held request: the popup closes.
	release: Mutex<Option<mpsc::UnboundedSender<String>>>,
}

impl Shown {
	fn release(&self) {
		if let Some(sender) = locked(&self.release).take() {
			// The popup may have closed its end already.
			let _ = sender.send("event: end\ndata: over\n\n".to_owned());
		}
	}
}

#[derive(Default)]
struct AskState {
	queue: Vec<Popup>,
	current: Option<Arc<Shown>>,
}

/// The popup's held request: ends the question as `closed` when the popup drops it before the answer.
struct HoldBody {
	events: mpsc::UnboundedReceiver<String>,
	popup: Popup,
}

impl Stream for HoldBody {
	type Item = std::result::Result<String, Infallible>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.events.poll_recv(cx).map(|event| event.map(Ok))
	}
}

impl Drop for HoldBody {
	fn drop(&mut self) {
		if !self.popup.open() {
			return;
		}
		log(LogLevel::Info, "question", &format!("the popup closed unanswered: {}", self.popup.view().title), None);
		self.popup.close(Some(PopupEnd::Closed));
	}
}

struct Ask {
	herdr: Arc<dyn Herdr>,
	hold_wait_ms: i64,
	retry_ms: i64,
	warner: Arc<Warner>,
	state: Mutex<AskState>,
}

fn millis(ms: i64) -> Duration {
	Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// His questions in the plugin's `ask` popup (0553, 0558), one at a time: Allow questions and the close question first,
/// then those his keys opened (0578), each oldest first. The popup gets the question's one-time code through `--env` and
/// holds `GET /api/popup/<code>/hold` while it shows it. Until it reads the question, herdr is asked to open it again
/// (Settings or another popup in front, no client attached); once read, never again, since a second read ends it
/// (0578), and a read never followed by its hold within the wait counts as closed. A held request that ends before the
/// answer is the popup closed some other way: the question ends as its fallback. Each question but those his own keys
/// opened also goes out as a notification with herdr's request sound, for when the terminal isn't in front (0552, 0581).
///
/// Needs a running tokio runtime: each shown question is presented by a task of its own.
#[derive(Clone)]
pub struct AskPopups {
	ask: Arc<Ask>,
}

impl AskPopups {
	pub fn new(herdr: Arc<dyn Herdr>, options: AskOptions) -> Self {
		let now = options.now.unwrap_or_else(|| Arc::new(system_now));
		Self {
			ask: Arc::new(Ask {
				herdr,
				hold_wait_ms: options.hold_wait_ms.unwrap_or(HOLD_WAIT_MS),
				retry_ms: options.retry_ms.unwrap_or(RETRY_MS),
				warner: Warner::new(now),
				state: Mutex::new(AskState::default()),
			}),
		}
	}

	pub fn show(&self, popup: &Popup) {
		locked(&self.ask.state).queue.push(popup.clone());
		let ask = Arc::clone(&self.ask);
		let popup = popup.clone();
		tokio::spawn(async move {
			popup.answer().await;
			ask.ended(&popup);
		});
		self.ask.pump();
	}

	/// A question asked again (0578): first among its kind in the queue; the one shown now stays as it is.
	pub fn front(&self, popup: &Popup) {
		{
			let mut state = locked(&self.ask.state);
			let Some(index) = state.queue.iter().position(|p| p == popup).filter(|index| *index > 0) else {
				return;
			};
			if state.current.as_ref().is_some_and(|shown| &shown.popup == popup) {
				return;
			}
			let moved = state.queue.remove(index);
			state.queue.insert(0, moved);
		}
		self.ask.pump();
	}

	/// The popup's held request for `code` (`GET /api/popup/<code>/hold`): 403 unless it is the question shown now.
	/// The request ends when the returned body is dropped (the popup closed some other way): the question denies.
	pub fn hold(&self, code: &str) -> Response {
		let shown = locked(&self.ask.state).current.clone();
		let Some(shown) = shown.filter(|shown| shown.popup.code() == code && shown.popup.open()) else {
			return forbidden();
		};
		if shown.held.swap(true, Ordering::SeqCst) {
			return forbidden();
		}
		let (sender, events) = mpsc::unbounded_channel();
		// The receiver is in hand: this can't fail.
		let _ = sender.send(": shown\n\n".to_owned());
		*locked(&shown.release) = Some(sender);
		shown.held_now.send_replace(true);
		let body = Body::from_stream(HoldBody { events, popup: shown.popup.clone() });
		let mut response = Response::new(body);
		response.headers_mut().insert(header::CONTENT_TYPE, "text/event-stream".parse().expect("a header value"));
		response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().expect("a header value"));
		response
	}
}

impl PopupHolder for AskPopups {
	fn hold(&self, code: &str) -> Response {
		AskPopups::hold(self, code)
	}

	fn front(&self, popup: &Popup) {
		AskPopups::front(self, popup);
	}
}

fn forbidden() -> Response {
	let mut response = Response::new(Body::from("Forbidden"));
	*response.status_mut() = StatusCode::FORBIDDEN;
	response
}

/// `text` cut to herdr's notification body: 240 UTF-16 units, the last of a cut one an ellipsis.
fn note_body(text: &str) -> String {
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

impl Ask {
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
			shown.release();
		}
		self.pump();
	}

	fn pump(self: &Arc<Self>) {
		let shown = {
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
			let shown = Arc::new(Shown {
				popup: next,
				held: AtomicBool::new(false),
				held_now: watch::channel(false).0,
				release: Mutex::new(None),
			});
			state.current = Some(Arc::clone(&shown));
			shown
		};
		let ask = Arc::clone(self);
		tokio::spawn(async move { ask.present(&shown).await });
	}

	fn is_showing(&self, shown: &Arc<Shown>) -> bool {
		let showing = locked(&self.state).current.as_ref().is_some_and(|current| Arc::ptr_eq(current, shown));
		showing && shown.popup.open() && !shown.held.load(Ordering::SeqCst)
	}

	/// Settles when the popup holds its request, the question is answered or `wait_ms` passed.
	async fn held_or_answered(&self, shown: &Shown, wait_ms: i64) {
		let mut held = shown.held_now.subscribe();
		tokio::select! {
			_ = held.wait_for(|held| *held) => {}
			_ = shown.popup.answer() => {}
			() = sleep(millis(wait_ms)) => {}
		}
	}

	fn notify(&self, popup: &Popup) {
		let note = Note {
			title: "Jocasta asks".to_owned(),
			body: Some(note_body(&popup.view().message)),
			sound: Sound::Request,
		};
		let (herdr, warner) = (Arc::clone(&self.herdr), Arc::clone(&self.warner));
		tokio::spawn(async move {
			if let Err(error) = herdr.notify(&note).await {
				warner.warn("the question's notification failed", &error);
			}
		});
	}

	async fn present(self: &Arc<Self>, shown: &Arc<Shown>) {
		let popup = &shown.popup;
		let options = PluginPaneOptions {
			plugin_id: PLUGIN_ID.to_owned(),
			entrypoint: "ask".to_owned(),
			placement: Placement::Popup,
			target_pane_id: None,
			direction: None,
			cwd: None,
			env: Some(vec![(POPUP_ENV.to_owned(), popup.code().to_owned())]),
			focus: Some(true),
		};
		let mut first = true;
		while self.is_showing(shown) {
			if popup.read() {
				// Read and not held within the wait: that popup is gone, and opening another would read it again.
				self.held_or_answered(shown, self.hold_wait_ms).await;
				if self.is_showing(shown) {
					let title = &popup.view().title;
					log(LogLevel::Info, "question", &format!("the popup read the question and closed: {title}"), None);
					popup.close(Some(PopupEnd::Closed));
				}
				return;
			}
			let mut wait = self.hold_wait_ms;
			if let Err(error) = self.herdr.open_plugin_pane(&options).await {
				wait = self.retry_ms;
				self.warner.warn("the question's popup didn't open", &error);
			}
			// A question his own key opened shows where he looks: no notification (0581, UX-6).
			if first && !popup.from_key() {
				first = false;
				self.notify(popup);
			}
			self.held_or_answered(shown, wait).await;
		}
	}
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
}

impl KeyService for Keys {
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
	pub ask: AskQuestion,
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
		PluginAction::Quit => {
			let text =
				if (deps.quit)().await? == CloseOutcome::Kept { "Jocasta keeps running." } else { "Jocasta stopped." };
			Ok(Some(said(text)))
		}
	}
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
