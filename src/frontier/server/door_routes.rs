//! The doors into the service (the Rust port of `door-routes.ts`): `jocasta mcp`'s tools, `jocasta thread|worktree`,
//! git's pre-push hook, a project pane's check and bare `jocasta`'s shutdown.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use futures_util::future::BoxFuture as LocalBoxFuture;
use serde_json::{Map, Value, json};
use tokio::sync::watch;
use url::Url;

use crate::frontier::server::launch::SHUTDOWN_PATH;
use crate::frontier::server::push_gate::parse_push_request;
use crate::frontier::server::routes::{
	BoxFuture, Route, ServerHandle, json_response, object_args, refused_post, text_response,
};
use crate::frontier::server::tools::{Caller, ToolReply};
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};
use crate::shared::pane_check::{PANE_CHECK_PATH, PaneCheckAnswer};
use crate::shared::process::AbortSignal;
use crate::shared::push_hook::{PUSH_PATH, PushAnswer, PushRequest};
use crate::shared::thread_cli::CLI_PATH;

/// Where `jocasta mcp` calls its tools (`src/mcp/server.ts`).
const MCP_PATH: &str = "/api/mcp/";

/// The biggest request body read, as the other routes' (`routes.rs`).
const MAX_BODY: usize = 128 * 1024 * 1024;

/// How long after the answer `POST /api/shutdown` waits to start the close.
const SHUTDOWN_DELAY: Duration = Duration::from_millis(100);

/// What the doors need of the service: the TypeScript's `Pick<JocastaService, "callTool" | "cliCall" | "pushVerdict" |
/// "paneCheck">`. An `Err` is a rejection: the route answers `500 Internal error` and logs it.
pub trait DoorService: Send + Sync {
	/// One tool of `jocasta mcp`.
	fn call_tool<'a>(
		&'a self,
		name: &'a str,
		args: Map<String, Value>,
		caller: Caller,
	) -> LocalBoxFuture<'a, Result<ToolReply>>;
	/// One `jocasta thread|worktree` command.
	fn cli_call<'a>(&'a self, command: &'a str, args: Map<String, Value>) -> LocalBoxFuture<'a, Result<ToolReply>>;
	/// git's pre-push hook.
	fn push_verdict(&self, request: PushRequest) -> LocalBoxFuture<'_, Result<PushAnswer>>;
	/// A project pane's check; `signal` aborts when its request ends.
	fn pane_check<'a>(&'a self, pane_key: &'a str, signal: AbortSignal) -> LocalBoxFuture<'a, Result<String>>;
}

/// What the doors need.
pub struct DoorParts {
	pub token: String,
	pub service: Arc<dyn DoorService>,
	/// His client closed (`POST /api/shutdown`): the close, asking first.
	pub client_closed: Arc<dyn Fn() + Send + Sync>,
}

/// Aborts its signal when dropped: the stand-in for `request.signal`, since a handler's future is dropped when its
/// client disconnects.
pub(crate) struct Disconnect(watch::Sender<Option<String>>);

impl Disconnect {
	/// A guard and the signal it aborts.
	pub(crate) fn new() -> (Self, AbortSignal) {
		let (sender, signal) = watch::channel(None);
		(Self(sender), signal)
	}
}

impl Drop for Disconnect {
	fn drop(&mut self) {
		self.0.send_replace(Some("the request ended".to_owned()));
	}
}

/// A route that threw: logged, answered `500 Internal error` (the server's `error` hook).
fn internal(message: &str) -> Response {
	log(LogLevel::Error, "server", message, None);
	text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
}

fn refusal(status: StatusCode, text: &str) -> Response {
	json_response(status, &json!({ "text": text, "isError": true }))
}

fn reply_json<T: serde::Serialize>(reply: Result<T>) -> Response {
	match reply.map(|reply| serde_json::to_value(reply)) {
		Ok(Ok(value)) => json_response(StatusCode::OK, &value),
		Ok(Err(error)) => internal(&error.to_string()),
		Err(error) => internal(&error.to_string()),
	}
}

/// `decodeURIComponent`: `None` where it throws (a malformed escape, or bytes that aren't UTF-8).
pub(crate) fn decode_uri_component(text: &str) -> Option<String> {
	let bytes = text.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut at = 0;
	while at < bytes.len() {
		if bytes[at] == b'%' {
			let pair = text.get(at + 1..at + 3)?;
			out.push(u8::from_str_radix(pair, 16).ok().filter(|_| pair.bytes().all(|b| b.is_ascii_hexdigit()))?);
			at += 3;
		} else {
			out.push(bytes[at]);
			at += 1;
		}
	}
	String::from_utf8(out).ok()
}

/// What a door's call has read of its request: the JSON object it sent.
struct Call {
	body: Map<String, Value>,
	url: Url,
	disconnect: Disconnect,
	signal: AbortSignal,
}

/// A door's call (`jocasta mcp`, `jocasta thread|worktree`): the API token as the bearer, POST, a JSON body; answered
/// `{text, isError?}`. A call may wait for his Allow up to 2 minutes (0484), a merge for its hooks: axum has no idle
/// timeout, so it needs none set.
async fn read_call(request: Request, url: Url, token: &str) -> std::result::Result<Call, Box<Response>> {
	if let Some(refused) = refused_post(&request, token, true) {
		return Err(Box::new(refused));
	}
	let not_json = || Box::new(refusal(StatusCode::BAD_REQUEST, "body is not JSON"));
	let Ok(bytes) = axum::body::to_bytes(request.into_body(), MAX_BODY).await else { return Err(not_json()) };
	let Ok(parsed) = serde_json::from_slice::<Value>(&bytes) else { return Err(not_json()) };
	let body = match parsed {
		Value::Object(map) => map,
		// Reading a property of `null` throws; of any other value it is `undefined`.
		Value::Null => return Err(Box::new(internal("TypeError: null is not an object (evaluating 'body.command')"))),
		_ => Map::new(),
	};
	let (disconnect, signal) = Disconnect::new();
	Ok(Call { body, url, disconnect, signal })
}

fn string_of(body: &Map<String, Value>, key: &str) -> Option<String> {
	body.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Which door a route is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Door {
	Mcp,
	Cli,
	Push,
	PaneCheck,
	Shutdown,
}

struct DoorRoute {
	door: Door,
	parts: Arc<DoorParts>,
}

impl DoorRoute {
	async fn answer(door: Door, parts: &DoorParts, call: Call) -> Response {
		let Call { body, url, disconnect, signal } = call;
		let service = &parts.service;
		let response = match door {
			// `POST /api/mcp/<tool>`: body `{args, paneKey?, bossKey?}` (the pane's `JOCASTA_PANE_KEY`, the boss's key; 0578).
			Door::Mcp => {
				let Some(tool) = decode_uri_component(url.path().get(MCP_PATH.len()..).unwrap_or("")) else {
					return internal("URIError: URI malformed");
				};
				let caller = Caller { pane_key: string_of(&body, "paneKey"), boss_key: string_of(&body, "bossKey") };
				let args = object_args(body.get("args").unwrap_or(&Value::Null));
				reply_json(service.call_tool(&tool, args, caller).await)
			}
			// `POST /api/cli` (0544): body `{command, args}`.
			Door::Cli => match body.get("command").and_then(Value::as_str) {
				None => refusal(StatusCode::BAD_REQUEST, "command is required"),
				Some(command) => {
					reply_json(service.cli_call(command, object_args(body.get("args").unwrap_or(&Value::Null))).await)
				}
			},
			// `POST /api/push` from git's pre-push hook (0613): body `{dir, remote, refs, paneKey?}`; answered `{allow, reason?}`.
			Door::Push => match parse_push_request(&body) {
				None => refusal(StatusCode::BAD_REQUEST, "not a push request"),
				Some(request) => reply_json(service.push_verdict(request).await),
			},
			// `POST /api/pane-check` from a project pane's `jocasta pane-check` (0617): body `{paneKey}`; answered `{text}`.
			Door::PaneCheck => match body.get("paneKey").and_then(Value::as_str) {
				None => refusal(StatusCode::BAD_REQUEST, "paneKey is required"),
				Some(pane_key) => {
					reply_json(service.pane_check(pane_key, signal).await.map(|text| PaneCheckAnswer { text }))
				}
			},
			Door::Shutdown => unreachable!("the shutdown door reads no call"),
		};
		drop(disconnect);
		response
	}

	fn shutdown(&self, request: &Request) -> Response {
		if let Some(refused) = refused_post(request, &self.parts.token, false) {
			return refused;
		}
		// After the answer has gone out, as it may come from a closing terminal.
		let closed = self.parts.client_closed.clone();
		tokio::spawn(async move {
			tokio::time::sleep(SHUTDOWN_DELAY).await;
			closed();
		});
		let mut response = Response::new(axum::body::Body::empty());
		*response.status_mut() = StatusCode::ACCEPTED;
		response
	}
}

impl Route for DoorRoute {
	fn matches(&self, path: &str) -> bool {
		match self.door {
			Door::Mcp => path.starts_with(MCP_PATH),
			Door::Cli => path == CLI_PATH,
			Door::Push => path == PUSH_PATH,
			Door::PaneCheck => path == PANE_CHECK_PATH,
			// `POST /api/shutdown` from bare `jocasta` (0434, 0549): his client closed. Answered at once, since it may come
			// from a closing terminal with seconds left.
			Door::Shutdown => path == SHUTDOWN_PATH,
		}
	}

	fn handle(&self, request: Request, url: Url, _server: ServerHandle) -> BoxFuture<Response> {
		if self.door == Door::Shutdown {
			let response = self.shutdown(&request);
			return Box::pin(async move { response });
		}
		let (door, parts) = (self.door, self.parts.clone());
		Box::pin(async move {
			match read_call(request, url, &parts.token).await {
				Ok(call) => Self::answer(door, &parts, call).await,
				Err(refused) => *refused,
			}
		})
	}
}

/// The doors into the service: `jocasta mcp`'s tools, `jocasta thread|worktree` and bare `jocasta`'s shutdown.
pub fn door_routes(parts: DoorParts) -> Vec<Box<dyn Route>> {
	let parts = Arc::new(parts);
	[Door::Mcp, Door::Cli, Door::Push, Door::PaneCheck, Door::Shutdown]
		.into_iter()
		.map(|door| Box::new(DoorRoute { door, parts: parts.clone() }) as Box<dyn Route>)
		.collect()
}

#[cfg(test)]
#[path = "door_routes_tests.rs"]
mod tests;
