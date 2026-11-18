//! How the `jocasta` herdr plugin's programs reach the running Jocasta service (0553): its port and token from
//! `<data>/jocasta-service.json` (herdr starts them with the service's environment, so the same data dir) when its
//! process is alive, the state feed over the WebSocket, his thread keys and his questions over HTTP with the token as a
//! bearer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Message, http};

use crate::frontier::protocol::{
	ClientMessage, PLUGIN_ACTION_PATH, POPUP_HOLD, POPUP_PATH, POPUP_READER_HEADER, PluginAction, PopupAnswer,
	PopupView, ServerMessage, WS_PATH,
};
use crate::shared::pane_check::{PANE_CHECK_ASK_MS, PANE_CHECK_PATH, PaneCheckAnswer};
use crate::shared::push_hook::{PUSH_PATH, PushReply, PushRequest};
use crate::shared::service_file::{NOT_RUNNING, connection_refused, running_service};

/// The running service's address and token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceAt {
	pub base: String,
	pub token: String,
}

/// The running service's address and token, or `None` while none runs (no file, or its process is gone).
pub fn service_at() -> Option<ServiceAt> {
	running_service().map(|at| ServiceAt { base: format!("http://127.0.0.1:{}", at.port), token: at.token })
}

/// A client for the loopback service: no proxy, whatever the environment says.
fn http_client() -> reqwest::Client {
	reqwest::Client::builder().no_proxy().build().unwrap_or_default()
}

/// Why a call to the service failed: nothing listening is the one "not running" text (0581 UX-13).
fn failed(error: &reqwest::Error) -> String {
	if connection_refused(error) { NOT_RUNNING.to_owned() } else { format!("Jocasta didn't answer: {error}") }
}

/// `Jocasta answered <status>: <body>`.
async fn answered(response: reqwest::Response) -> String {
	let status = response.status().as_u16();
	format!("Jocasta answered {status}: {}", response.text().await.unwrap_or_default())
}

/// Sends one of his thread keys to the service (0554); resolves why not when it wasn't taken, else `None`.
pub async fn post_action(action: PluginAction, pane_id: Option<&str>) -> Option<String> {
	let Some(at) = service_at() else { return Some(NOT_RUNNING.to_owned()) };
	let mut body = json!({ "action": action });
	if let Some(pane_id) = pane_id.filter(|id| !id.is_empty()) {
		body["paneId"] = json!(pane_id);
	}
	let sent = http_client()
		.post(format!("{}{PLUGIN_ACTION_PATH}", at.base))
		.bearer_auth(&at.token)
		.timeout(Duration::from_secs(10))
		.json(&body)
		.send()
		.await;
	match sent {
		Ok(response) if response.status().is_success() => None,
		Ok(response) => Some(answered(response).await),
		Err(error) => Some(failed(&error)),
	}
}

/// The check for the project pane started with `pane_key` (0617): what the service gives its agent at a turn's end.
/// Empty when there is nothing to say, and when Jocasta isn't running, didn't answer in time or answered an error: the
/// extension then lets the turn end.
pub async fn ask_pane_check(pane_key: Option<&str>) -> String {
	let (Some(at), Some(pane_key)) = (service_at(), pane_key.filter(|key| !key.is_empty())) else {
		return String::new();
	};
	let sent = http_client()
		.post(format!("{}{PANE_CHECK_PATH}", at.base))
		.bearer_auth(&at.token)
		.timeout(Duration::from_millis(PANE_CHECK_ASK_MS.unsigned_abs()))
		.json(&json!({ "paneKey": pane_key }))
		.send()
		.await;
	let Ok(response) = sent else { return String::new() };
	if !response.status().is_success() {
		return String::new();
	}
	response.json::<PaneCheckAnswer>().await.map(|answer| answer.text).unwrap_or_default()
}

/// Asks the service whether a push may go ahead (0613), which may wait for his Allow up to 2 minutes; an `Err` is why no
/// answer came (Jocasta isn't running, or didn't answer).
pub async fn ask_push(request: &PushRequest) -> PushReply {
	let at = service_at().ok_or_else(|| NOT_RUNNING.to_owned())?;
	let sent = http_client()
		.post(format!("{}{PUSH_PATH}", at.base))
		.bearer_auth(&at.token)
		.timeout(Duration::from_secs(150))
		.json(request)
		.send()
		.await
		.map_err(|error| failed(&error))?;
	if !sent.status().is_success() {
		return Err(answered(sent).await);
	}
	sent.json().await.map_err(|error| failed(&error))
}

/// A question as its one reader got it (0578): its texts and the reader secret its hold and answer carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
	pub code: String,
	pub view: PopupView,
	pub reader: String,
}

/// One question (`GET /api/popup/<code>`, its first and only read), or why not.
pub async fn question_view(code: &str) -> Result<Question, String> {
	let at = service_at().ok_or_else(|| NOT_RUNNING.to_owned())?;
	let response = http_client()
		.get(format!("{}{POPUP_PATH}{code}", at.base))
		.bearer_auth(&at.token)
		.timeout(Duration::from_secs(10))
		.send()
		.await
		.map_err(|error| failed(&error))?;
	let status = response.status();
	if !status.is_success() {
		return Err(if status.as_u16() == 403 {
			"This question was already answered.".to_owned()
		} else {
			status.as_u16().to_string()
		});
	}
	let reader = response
		.headers()
		.get(POPUP_READER_HEADER)
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default()
		.to_owned();
	let view = response.json::<PopupView>().await.map_err(|error| failed(&error))?;
	Ok(Question { code: code.to_owned(), view, reader })
}

/// The service's reply to an answer (0578): taken; a wrong code with the tries left; the third wrong code, which took
/// the fallback; or why it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerReply {
	Taken,
	Wrong { left: Option<u64> },
	Exhausted,
	Failed { text: String },
}

/// His answer (`POST /api/popup/<code>` with the reader secret).
pub async fn answer_question(question: &Question, answer: &PopupAnswer) -> AnswerReply {
	let Some(at) = service_at() else { return AnswerReply::Failed { text: NOT_RUNNING.to_owned() } };
	let sent = http_client()
		.post(format!("{}{POPUP_PATH}{}", at.base, question.code))
		.bearer_auth(&at.token)
		.header(POPUP_READER_HEADER, &question.reader)
		.timeout(Duration::from_secs(10))
		.json(answer)
		.send()
		.await;
	let response = match sent {
		Ok(response) => response,
		Err(error) => return AnswerReply::Failed { text: failed(&error) },
	};
	match response.status().as_u16() {
		200..=299 => AnswerReply::Taken,
		410 => AnswerReply::Exhausted,
		409 => {
			let body: Option<Value> = response.json().await.ok();
			AnswerReply::Wrong { left: body.as_ref().and_then(|body| body.get("left")).and_then(Value::as_u64) }
		}
		_ => AnswerReply::Failed { text: answered(response).await },
	}
}

/// What ends a hold: `on_end` runs once, whichever comes first.
type OnEnd = Box<dyn FnOnce() + Send>;

/// Holds the question's `GET /api/popup/<code>/hold` open while the popup shows it (0558); `on_end` runs once when it
/// ends: the service ended the question (answered, withdrawn, Jocasta closing) or went away. The returned release ends
/// the hold, which takes the fallback when the question is still open (closing the popup any other way denies). A
/// release that is dropped without being called leaves the hold open.
pub fn hold_question(question: &Question, on_end: impl FnOnce() + Send + 'static) -> Box<dyn FnOnce() + Send> {
	let Some(at) = service_at() else {
		on_end();
		return Box::new(|| {});
	};
	let (release, released) = oneshot::channel::<()>();
	let url = format!("{}{POPUP_PATH}{}{POPUP_HOLD}", at.base, question.code);
	let reader = question.reader.clone();
	let on_end: OnEnd = Box::new(on_end);
	tokio::spawn(async move {
		let held = async {
			let sent = http_client().get(url).bearer_auth(&at.token).header(POPUP_READER_HEADER, reader).send().await;
			let Ok(mut response) = sent else { return };
			if !response.status().is_success() {
				return;
			}
			// Only its end matters.
			while let Ok(Some(_)) = response.chunk().await {}
		};
		let released = async {
			if released.await.is_err() {
				std::future::pending::<()>().await;
			}
		};
		tokio::select! { () = held => {}, () = released => {} }
		on_end();
	});
	Box::new(move || {
		let _ = release.send(());
	})
}

/// The state feed `state_feed` returns.
pub struct StateFeed {
	socket: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
	closed: Arc<AtomicBool>,
	stop: Arc<Notify>,
}

impl StateFeed {
	/// Sends a client message; false when the feed isn't connected (nothing was sent).
	pub fn send(&self, message: &ClientMessage) -> bool {
		let Ok(text) = serde_json::to_string(message) else { return false };
		let socket = self.socket.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
		socket.as_ref().is_some_and(|socket| socket.send(text).is_ok())
	}

	/// Stops the feed: no reconnect, and no more calls to `on_up` or `on_message`.
	pub fn close(&self) {
		self.closed.store(true, Ordering::SeqCst);
		self.socket.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
		self.stop.notify_one();
	}
}

/// The feed's callbacks and state, shared with its task.
struct Feed<M, U> {
	on_message: M,
	on_up: U,
	socket: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
	closed: Arc<AtomicBool>,
}

impl<M: Fn(ServerMessage) + Send + Sync, U: Fn(bool) + Send + Sync> Feed<M, U> {
	fn up(&self, up: bool) {
		if !self.closed.load(Ordering::SeqCst) {
			(self.on_up)(up);
		}
	}

	/// One connection: connected, relaying until it ends or the feed closes. Returns whether it closed.
	async fn connect(&self, at: &ServiceAt, stop: &Notify) -> bool {
		let url = format!("{}{WS_PATH}", at.base.replacen("http", "ws", 1));
		let Ok(mut request) = url.into_client_request() else { return false };
		let Ok(auth) = http::HeaderValue::from_str(&format!("Bearer {}", at.token)) else { return false };
		request.headers_mut().insert("authorization", auth);
		let Ok((mut socket, _)) = tokio_tungstenite::connect_async(request).await else { return false };
		let (sender, mut outgoing) = mpsc::unbounded_channel::<String>();
		*self.socket.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
		self.up(true);
		let closed = loop {
			tokio::select! {
				() = stop.notified() => break true,
				text = outgoing.recv() => match text {
					Some(text) => {
						if socket.send(Message::text(text)).await.is_err() {
							break false;
						}
					}
					None => break true,
				},
				incoming = socket.next() => match incoming {
					Some(Ok(Message::Text(text))) => self.message(text.as_str()),
					Some(Ok(Message::Close(_)) | Err(_)) | None => break false,
					Some(Ok(_)) => {}
				},
			}
		};
		self.socket.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
		closed
	}

	fn message(&self, text: &str) {
		if self.closed.load(Ordering::SeqCst) {
			return;
		}
		// Not one of the service's messages: skipped. Outside the parse, a failure drawing it is the pane's own (FE-5).
		if let Ok(message) = serde_json::from_str::<ServerMessage>(text) {
			(self.on_message)(message);
		}
	}

	async fn run(&self, stop: &Notify) {
		loop {
			if self.closed.load(Ordering::SeqCst) {
				return;
			}
			match service_at() {
				None => self.up(false),
				Some(at) => {
					if self.connect(&at, stop).await {
						return;
					}
					self.up(false);
				}
			}
			tokio::select! {
				() = stop.notified() => return,
				() = tokio::time::sleep(Duration::from_secs(2)) => {}
			}
		}
	}
}

/// The service's state feed (`/ws`): every message to `on_message`; `on_up` with whether it is connected. Reconnects
/// every 2 s while the service is down or restarting (a new port or token in `jocasta-service.json`).
pub fn state_feed(
	on_message: impl Fn(ServerMessage) + Send + Sync + 'static,
	on_up: impl Fn(bool) + Send + Sync + 'static,
) -> StateFeed {
	let socket = Arc::new(Mutex::new(None));
	let closed = Arc::new(AtomicBool::new(false));
	let stop = Arc::new(Notify::new());
	let feed = Feed { on_message, on_up, socket: Arc::clone(&socket), closed: Arc::clone(&closed) };
	let task_stop = Arc::clone(&stop);
	tokio::spawn(async move { feed.run(&task_stop).await });
	StateFeed { socket, closed, stop }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
