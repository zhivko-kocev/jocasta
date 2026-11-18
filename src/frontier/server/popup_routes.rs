//! His questions' routes and the thread keys' route (the Rust port of `popup-routes.ts`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use futures_util::future::BoxFuture as LocalBoxFuture;
use serde_json::{Value, json};
use url::Url;

use crate::frontier::protocol::{
	PLUGIN_ACTION_PATH, PLUGIN_ACTIONS, POPUP_HOLD, POPUP_PATH, POPUP_READER_HEADER, PluginAction, PopupView,
	ServerMessage, ToastLevel, ToastReason,
};
use crate::frontier::server::close::CloseOutcome;
use crate::frontier::server::events::Toast;
use crate::frontier::server::herdr::pane_text;
use crate::frontier::server::plugin::{ActionDeps, KeyService, run_action};
use crate::frontier::server::popups::{AnswerTaken, Popup, PopupEnd, PopupRead, Popups};
use crate::frontier::server::routes::{
	BoxFuture, PageRoute, Route, ServerHandle, body_object, has_bearer, json_response, refused_post, text_response,
};
use crate::shared::log::{LogLevel, log};
use crate::shared::process::AbortSignal;

/// One refusal notification per text this often (0578); `notifications.jsonl` still logs each.
const REFUSAL_TOAST_MS: i64 = 30_000;

/// Asks him a question: `popups` opens it, `show` shows it (0519, 0552). The signal ends it unanswered once it aborts;
/// the flag says his key asked it (`fromKey`).
pub type Ask = Arc<dyn Fn(&PopupView, Option<AbortSignal>, bool) -> Popup + Send + Sync>;

/// Shows a question; an `Err` is logged as a question it couldn't show.
pub type Show = Arc<dyn Fn(&Popup) -> crate::shared::errors::Result<()> + Send + Sync>;

/// What the service raises a notification with: the TypeScript's `Pick<JocastaService, "notify">`. `show` false is a
/// repeat that is logged and not shown again.
pub trait Notifier: Send + Sync {
	fn notify(&self, toast: Toast, show: bool) -> LocalBoxFuture<'_, ()>;
}

/// What `AskPopups` offers these routes: the `ask` popup's held request and bringing a question to the front.
pub trait PopupHolder: Send + Sync {
	/// The hold of question `code`: open as long as it shows its question. Its body ends the question as closed when
	/// the request drops it (axum drops a body when its client leaves), so no signal goes in.
	fn hold(&self, code: &str) -> Response;
	/// Brings an open question to the front of the queue (asked again).
	fn front(&self, popup: &Popup);
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Opens each question in `popups` and hands it to `show`; it ends unanswered on a signal.
pub fn asker(popups: Arc<Popups>, show: Option<Show>) -> Ask {
	Arc::new(move |view, signal, from_key| {
		let popup = popups.open(view, from_key);
		if let Some(signal) = signal {
			if signal.borrow().is_some() {
				popup.close(None);
			} else {
				let watched = popup.clone();
				tokio::spawn(async move {
					let mut signal = Some(signal);
					tokio::select! {
						_ = crate::shared::process::aborted(&mut signal) => watched.close(None),
						_ = watched.answer() => {}
					}
				});
			}
		}
		if popup.open() {
			log(LogLevel::Info, "question", &format!("asking him: {}", popup.view().title), None);
			if let Some(Err(error)) = show.as_ref().map(|show| show(&popup)) {
				log(LogLevel::Warn, "question", &format!("couldn't show it: {error}"), None);
			}
		}
		popup
	})
}

/// What his questions' routes need.
pub struct PopupParts {
	pub token: String,
	pub popups: Arc<Popups>,
	pub ask_popups: Option<Arc<dyn PopupHolder>>,
	pub service: Arc<dyn Notifier>,
}

/// His questions' routes: [`hold`](PopupRoutes::hold) before the page fallback, [`question`](PopupRoutes::question)
/// among the page's bearer routes.
pub struct PopupRoutes {
	pub hold: HoldRoute,
	pub question: QuestionRoute,
}

/// What both routes share.
struct Shared {
	token: String,
	popups: Arc<Popups>,
	ask_popups: Option<Arc<dyn PopupHolder>>,
	service: Arc<dyn Notifier>,
}

/// The `ask` popup's held request (0558): see [`popup_routes`].
pub struct HoldRoute(Arc<Shared>);

/// `/api/popup/<code>`, a question's texts and its answer: see [`popup_routes`].
pub struct QuestionRoute(Arc<Shared>);

fn forbidden() -> Response {
	text_response(StatusCode::FORBIDDEN, "Forbidden")
}

fn reader_of(request: &Request) -> Option<&str> {
	request.headers().get(POPUP_READER_HEADER).and_then(|value| value.to_str().ok())
}

impl Shared {
	/// A second program read his question (0578): it ended as Deny or Cancel, and he is told.
	fn second_reader(&self, popup: &Popup) {
		let fallback = &popup.view().fallback;
		log(
			LogLevel::Warn,
			"question",
			&format!("a second program read the question, which counted as {fallback}"),
			None,
		);
		let text =
			format!("Jocasta — Another program read your question \"{}\"; it counted as Deny.", popup.view().message);
		let service = self.service.clone();
		tokio::spawn(
			async move { service.notify(Toast::new(ToastReason::Failed, ToastLevel::Warning, text), true).await },
		);
	}

	/// A question's texts and its answer, each with the service's token (checked before) and the question's one-time
	/// code: an unknown or used code is refused. The first read hands out the reader secret, which the answer must
	/// carry; a second read ends the question (410). Answers: 204 taken; 409 `{left}` a wrong code; 410 the third wrong
	/// code, which took the fallback.
	async fn popup_response(&self, request: Request, url: Url) -> Response {
		let code = url.path().get(POPUP_PATH.len()..).unwrap_or("").to_owned();
		if code.contains('/') {
			return text_response(StatusCode::NOT_FOUND, "Not found");
		}
		if request.method() == axum::http::Method::GET {
			return match self.popups.read(&code) {
				None => forbidden(),
				Some(PopupRead::Second(popup)) => {
					self.second_reader(&popup);
					text_response(StatusCode::GONE, "read already")
				}
				Some(PopupRead::First { view, reader }) => {
					let mut response =
						json_response(StatusCode::OK, &serde_json::to_value(&*view).unwrap_or(Value::Null));
					let headers = response.headers_mut();
					headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
					if let Ok(reader) = HeaderValue::from_str(&reader) {
						headers.insert(POPUP_READER_HEADER, reader);
					}
					response
				}
			};
		}
		if request.method() != axum::http::Method::POST {
			return text_response(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed");
		}
		if !self.popups.is_reader(&code, reader_of(&request)) {
			return forbidden();
		}
		// An answer without a body is no choice.
		let taken = self.popups.answer(&code, &body_object(request).await);
		match taken {
			AnswerTaken::Unknown => forbidden(),
			AnswerTaken::Invalid => text_response(StatusCode::BAD_REQUEST, "not one of its choices"),
			// A coded choice with letters that aren't its code (0558, 0568): it stays open, until the third.
			AnswerTaken::Wrong => json_response(StatusCode::CONFLICT, &json!({ "left": self.popups.left(&code) })),
			AnswerTaken::Exhausted => text_response(StatusCode::GONE, "wrong code three times"),
			AnswerTaken::Taken => {
				let mut response = Response::new(Body::empty());
				*response.status_mut() = StatusCode::NO_CONTENT;
				response
			}
		}
	}

	/// The hold: the bearer and the reader secret of its first read (0578). A hold for an open question without that
	/// secret ends it as a second read does.
	async fn hold_response(&self, request: Request, url: Url) -> Response {
		let path = url.path();
		let code = path.get(POPUP_PATH.len()..path.len().saturating_sub(POPUP_HOLD.len())).unwrap_or("").to_owned();
		let Some(ask_popups) = self.ask_popups.as_ref().filter(|_| has_bearer(&request, &self.token)) else {
			return forbidden();
		};
		if !self.popups.is_reader(&code, reader_of(&request)) {
			let Some(open) = self.popups.get(&code) else { return forbidden() };
			open.close(Some(PopupEnd::SecondReader));
			self.second_reader(&open);
			return text_response(StatusCode::GONE, "read already");
		}
		// Axum has no idle timeout, so the hold needs none switched off.
		ask_popups.hold(&code)
	}
}

impl Route for HoldRoute {
	fn matches(&self, path: &str) -> bool {
		path.starts_with(POPUP_PATH) && path.ends_with(POPUP_HOLD)
	}

	fn handle(&self, request: Request, url: Url, _server: ServerHandle) -> BoxFuture<Response> {
		let shared = self.0.clone();
		Box::pin(async move { shared.hold_response(request, url).await })
	}
}

impl PageRoute for QuestionRoute {
	fn matches(&self, path: &str) -> bool {
		path.starts_with(POPUP_PATH)
	}

	fn handle(&self, request: Request, url: Url) -> BoxFuture<Response> {
		let shared = self.0.clone();
		Box::pin(async move {
			if has_bearer(&request, &shared.token) { shared.popup_response(request, url).await } else { forbidden() }
		})
	}
}

/// His questions' routes: the `ask` popup's held request (0558), before the page fallback; and `/api/popup/<code>`, its
/// texts and its answer (0519, 0578), among the page's bearer routes.
pub fn popup_routes(parts: PopupParts) -> PopupRoutes {
	let shared = Arc::new(Shared {
		token: parts.token,
		popups: parts.popups,
		ask_popups: parts.ask_popups,
		service: parts.service,
	});
	PopupRoutes { hold: HoldRoute(shared.clone()), question: QuestionRoute(shared) }
}

/// What the service offers his thread keys' route: the keys' own methods, and the notification of a refusal.
pub trait ActionService: KeyService + Notifier {}

impl<T: KeyService + Notifier> ActionService for T {}

/// What his thread keys' route needs.
pub struct ActionParts {
	pub token: String,
	pub service: Arc<dyn ActionService>,
	pub ask: Ask,
	pub ask_popups: Option<Arc<dyn PopupHolder>>,
	pub broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	/// Jocasta's close from his quit key, asking first.
	pub quit: Arc<dyn Fn() -> LocalBoxFuture<'static, CloseOutcome> + Send + Sync>,
	/// How long Finish and New thread wait for his answer.
	pub waiting_ms: i64,
}

/// `POST /api/plugin/action`: see [`action_route`].
pub struct ActionRoute {
	state: Arc<Actions>,
}

struct Actions {
	token: String,
	reporter: Arc<Reporter>,
	/// What `run_action` needs; it holds his keys' open questions by (action, thread) (0578).
	deps: ActionDeps,
}

/// What says a key's outcome: the log, the pane's `action` message and a refusal's notification.
struct Reporter {
	service: Arc<dyn ActionService>,
	broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	/// When each refusal text last went out as a notification (0578), in the order they did.
	refusals_shown: Mutex<Vec<(String, i64)>>,
}

impl Reporter {
	fn report(&self, said: &str, is_error: bool) {
		let text = pane_text(said);
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

impl Actions {
	async fn run_key(self: Arc<Self>, action: PluginAction, pane_id: Option<String>) {
		match run_action(action, pane_id.as_deref(), &self.deps).await {
			Ok(Some(outcome)) => self.reporter.report(&outcome.text, outcome.is_error),
			Ok(None) => {}
			Err(error) => self.reporter.report(&error.to_string(), true),
		}
	}
}

fn action_names() -> Vec<String> {
	PLUGIN_ACTIONS.iter().filter_map(|action| serde_json::to_value(action).ok()?.as_str().map(str::to_owned)).collect()
}

impl Route for ActionRoute {
	fn matches(&self, path: &str) -> bool {
		path == PLUGIN_ACTION_PATH
	}

	fn handle(&self, request: Request, _url: Url, _server: ServerHandle) -> BoxFuture<Response> {
		let state = self.state.clone();
		Box::pin(async move {
			if let Some(refused) = refused_post(&request, &state.token, false) {
				return refused;
			}
			// No body: no action.
			let body = body_object(request).await;
			let said = body.get("action").and_then(Value::as_str);
			let Some(action) = PLUGIN_ACTIONS.iter().find(|action| {
				serde_json::to_value(action)
					.ok()
					.as_ref()
					.and_then(Value::as_str)
					.is_some_and(|name| Some(name) == said)
			}) else {
				return text_response(
					StatusCode::BAD_REQUEST,
					&format!("action is one of {}", action_names().join(", ")),
				);
			};
			let pane_id = body.get("paneId").and_then(Value::as_str).filter(|id| !id.is_empty()).map(str::to_owned);
			tokio::spawn(state.run_key(*action, pane_id));
			let mut response = Response::new(Body::empty());
			*response.status_mut() = StatusCode::ACCEPTED;
			response
		})
	}
}

/// `POST /api/plugin/action` (0544, 0554): one of his thread keys, `{action, paneId?}` with the service's token,
/// answered at once; what it did follows as an `action` message, and a refusal or half-done outcome as a notification
/// too: he may be on another workspace. A notification of the same text goes out once per 30 s;
/// `notifications.jsonl` logs each.
pub fn action_route(parts: ActionParts) -> ActionRoute {
	let reporter = Arc::new(Reporter {
		service: parts.service.clone(),
		broadcast: parts.broadcast,
		refusals_shown: Mutex::new(Vec::new()),
	});
	let (holder, quit, progress) = (parts.ask_popups, parts.quit, reporter.clone());
	let deps = ActionDeps {
		service: parts.service,
		ask: parts.ask,
		front: Box::new(move |popup| {
			if let Some(holder) = &holder {
				holder.front(popup);
			}
		}),
		quit: Box::new(move || {
			let closing = quit();
			Box::pin(async move { Ok(closing.await) })
		}),
		progress: Box::new(move |text| progress.report(text, false)),
		asking: Mutex::new(HashMap::new()),
		wait_ms: parts.waiting_ms,
	};
	ActionRoute { state: Arc::new(Actions { token: parts.token, reporter, deps }) }
}

#[cfg(test)]
#[path = "popup_routes_tests.rs"]
mod tests;
