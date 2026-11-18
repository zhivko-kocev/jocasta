//! The service's server: the routes it answers, the host check and the listening itself, on 127.0.0.1 (the Rust port of
//! `routes.ts`; axum, 0631).
//!
//! Where the TypeScript hands its routes Bun's `Server`, the Rust routes get a [`ServerHandle`]: the port, the state
//! feed's publish and subscribe, and the close signal. A route that became a WebSocket answers with axum's upgrade
//! response, so [`Route::handle`] always returns a [`Response`] (the TypeScript's `undefined`).

use std::any::Any;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use futures_util::FutureExt;
use serde_json::{Map, Value, json};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use url::Url;

use crate::shared::errors::{Error, Result};
use crate::shared::fsx::same_token;
use crate::shared::log::{LogLevel, log};

/// A boxed future, what a route's answer is.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// The most a request body may hold: Bun's default `maxRequestBodySize` (128 MiB).
const MAX_BODY: usize = 128 * 1024 * 1024;

/// How long a stopping server waits for its connections to end before it drops them.
const STOP_WAIT: Duration = Duration::from_secs(5);

/// How many state-feed messages a slow client may fall behind by before it misses some.
const FEED_BACKLOG: usize = 1024;

/// One route of the service's server: the paths it answers, and its answer.
pub trait Route: Send + Sync {
	fn matches(&self, path: &str) -> bool;
	/// Its answer; a request that became a WebSocket answers with the upgrade response.
	fn handle(&self, request: Request, url: Url, server: ServerHandle) -> BoxFuture<Response>;
}

/// A route answered among the pages (after the launch code, before the cookie check): always a response.
pub trait PageRoute: Send + Sync {
	fn matches(&self, path: &str) -> bool;
	fn handle(&self, request: Request, url: Url) -> BoxFuture<Response>;
}

/// The hosts this server answers as (DNS rebinding, SE-3).
pub fn host_allowed(host: Option<&str>, port: u16) -> bool {
	host.is_some_and(|host| host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}"))
}

/// The bearer of `request`'s Authorization header; empty when none.
pub fn bearer_token<B>(request: &axum::http::Request<B>) -> String {
	bearer_of(request.headers())
}

fn bearer_of(headers: &HeaderMap) -> String {
	let auth = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()).unwrap_or("");
	auth.strip_prefix("Bearer ").unwrap_or("").to_owned()
}

/// Whether `request` carries the API token as its bearer (every API call; SEC-NET-4).
pub fn has_bearer<B>(request: &axum::http::Request<B>, token: &str) -> bool {
	same_token(Some(&bearer_token(request)), token)
}

/// `text` as a plain-text response, as `new Response(text, { status })`.
pub fn text_response(status: StatusCode, text: &str) -> Response {
	let mut response = Response::new(Body::from(text.to_owned()));
	*response.status_mut() = status;
	response.headers_mut().insert(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain;charset=utf-8"));
	response
}

/// `value` as a JSON response, as `Response.json(value, { status })`.
pub fn json_response(status: StatusCode, value: &Value) -> Response {
	let mut response = Response::new(Body::from(value.to_string()));
	*response.status_mut() = status;
	response
		.headers_mut()
		.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json;charset=utf-8"));
	response
}

/// The prelude of the API's POST routes: 401 without the API token as the bearer, 405 for another method, each as the
/// tools' `{text, isError}` with `json`, else as plain text; `None` when the request goes on.
pub fn refused_post<B>(request: &axum::http::Request<B>, token: &str, json: bool) -> Option<Response> {
	let refuse = |text: &str, status: StatusCode| {
		if json {
			json_response(status, &json!({ "text": text, "isError": true }))
		} else {
			text_response(status, text)
		}
	};
	if !has_bearer(request, token) {
		return Some(refuse("invalid token", StatusCode::UNAUTHORIZED));
	}
	if request.method() != axum::http::Method::POST {
		return Some(refuse("POST only", StatusCode::METHOD_NOT_ALLOWED));
	}
	None
}

/// `value` when it is a plain object (a tool's or command's `args`), else none.
pub fn object_args(value: &Value) -> Map<String, Value> {
	value.as_object().cloned().unwrap_or_default()
}

/// The request's JSON body when it is an object (or an array, as `typeof` says), else `{}` (no body, not JSON, or not
/// an object).
pub async fn body_object(request: Request) -> Value {
	let Ok(bytes) = axum::body::to_bytes(request.into_body(), MAX_BODY).await else { return json!({}) };
	match serde_json::from_slice::<Value>(&bytes) {
		Ok(parsed @ (Value::Object(_) | Value::Array(_))) => parsed,
		_ => json!({}),
	}
}

/// What the server answers: the API's routes in order, then the pages.
pub struct Served {
	pub routes: Vec<Box<dyn Route>>,
	/// Every other request, with the server's port.
	pub page: Box<dyn Fn(Request, Url, u16) -> BoxFuture<Response> + Send + Sync>,
}

struct Inner {
	port: u16,
	clients: broadcast::Sender<String>,
	closing: watch::Sender<bool>,
	forced: watch::Sender<bool>,
	task: Mutex<Option<JoinHandle<()>>>,
}

/// The running server: what a route may ask of it, and how it stops. Cheap to clone.
#[derive(Clone)]
pub struct ServerHandle {
	inner: Arc<Inner>,
}

impl ServerHandle {
	/// The port it listens on.
	pub fn port(&self) -> u16 {
		self.inner.port
	}

	/// Sends `message` to every subscriber of the state feed (the TypeScript's `server.publish`).
	pub fn publish(&self, message: &str) {
		let _ = self.inner.clients.send(message.to_owned());
	}

	/// A receiver of what [`Self::publish`] sends from now on: a state-feed client's subscription.
	pub fn subscribe(&self) -> broadcast::Receiver<String> {
		self.inner.clients.subscribe()
	}

	/// Resolves once the server is stopping, for a long-lived connection (the feed's socket) to end.
	pub async fn closed(&self) {
		let mut closing = self.inner.closing.subscribe();
		let _ = closing.wait_for(|stopping| *stopping).await;
	}

	/// Stops listening and waits for the connections to end; with `close_active`, requests still being answered and
	/// connections still open are dropped (Bun's `stop(true)`).
	pub async fn stop(&self, close_active: bool) {
		if close_active {
			let _ = self.inner.forced.send(true);
		}
		let _ = self.inner.closing.send(true);
		let task = self.task();
		if let Some(mut task) = task {
			if close_active && tokio::time::timeout(STOP_WAIT, &mut task).await.is_err() {
				task.abort();
			} else if !close_active {
				let _ = task.await;
			}
		}
	}

	fn task(&self) -> Option<JoinHandle<()>> {
		let mut slot: MutexGuard<Option<JoinHandle<()>>> =
			self.inner.task.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		slot.take()
	}
}

/// What every request sees.
struct State {
	handle: ServerHandle,
	routes: Vec<Box<dyn Route>>,
	page: Box<dyn Fn(Request, Url, u16) -> BoxFuture<Response> + Send + Sync>,
}

/// The server on 127.0.0.1:`wanted`, or on a free port for this run when that one is busy (another program, or another
/// Jocasta's data dir; 0358). Each request: the host check, then the first route that answers its path, else the page.
/// Runs inside a tokio runtime.
pub fn listen(wanted: u16, served: Served) -> Result<ServerHandle> {
	let (listener, port) = match bind(wanted) {
		Ok(bound) => bound,
		Err(_) => {
			let bound = bind(0).map_err(|error| Error::other(format!("no port to listen on: {error}")))?;
			log(
				LogLevel::Warn,
				"server",
				&format!("port {wanted} (service.port) was busy; Jocasta is on port {} for this run", bound.1),
				None,
			);
			bound
		}
	};
	let (clients, _) = broadcast::channel(FEED_BACKLOG);
	let inner = Arc::new(Inner {
		port,
		clients,
		closing: watch::channel(false).0,
		forced: watch::channel(false).0,
		task: Mutex::new(None),
	});
	let handle = ServerHandle { inner: inner.clone() };
	let state = Arc::new(State { handle: handle.clone(), routes: served.routes, page: served.page });
	let app = Router::new().fallback(move |request: Request| dispatch(state.clone(), request));
	let mut closing = inner.closing.subscribe();
	let task = tokio::spawn(async move {
		let shutdown = async move {
			let _ = closing.wait_for(|stopping| *stopping).await;
		};
		let _ = axum::serve(listener, app).with_graceful_shutdown(shutdown).await;
	});
	*handle.inner.task.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(task);
	Ok(handle)
}

fn bind(port: u16) -> io::Result<(tokio::net::TcpListener, u16)> {
	let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
	listener.set_nonblocking(true)?;
	let port = listener.local_addr()?.port();
	Ok((tokio::net::TcpListener::from_std(listener)?, port))
}

/// One request: the host check, then the first route that answers its path, else the page; a handler that panics
/// answers 500 and is logged, as the TypeScript's `error` hook does; a stop that closes active connections drops it.
async fn dispatch(state: Arc<State>, request: Request) -> Response {
	let port = state.handle.port();
	let host = request.headers().get(header::HOST).and_then(|value| value.to_str().ok()).map(str::to_owned);
	if !host_allowed(host.as_deref(), port) {
		return text_response(StatusCode::FORBIDDEN, "Forbidden");
	}
	let path_and_query = request.uri().path_and_query().map_or("/", |pq| pq.as_str());
	let Ok(url) = Url::parse(&format!("http://{}{path_and_query}", host.as_deref().unwrap_or_default())) else {
		return text_response(StatusCode::BAD_REQUEST, "Bad Request");
	};
	let answer = match state.routes.iter().find(|route| route.matches(url.path())) {
		Some(route) => route.handle(request, url, state.handle.clone()),
		None => (state.page)(request, url, port),
	};
	let mut forced = state.handle.inner.forced.subscribe();
	tokio::select! {
		answered = std::panic::AssertUnwindSafe(answer).catch_unwind() => match answered {
			Ok(response) => response,
			Err(panic) => {
				log(LogLevel::Error, "server", &panic_message(panic.as_ref()), None);
				text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
			}
		},
		_ = forced.wait_for(|forced| *forced) => dropped(),
	}
}

/// A response whose body fails at once, so the connection is dropped, not answered.
fn dropped() -> Response {
	let failing = futures_util::stream::once(async { Err::<Bytes, io::Error>(io::Error::other("the server stopped")) });
	Response::new(Body::from_stream(failing))
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
	panic
		.downcast_ref::<&str>()
		.map(|message| (*message).to_owned())
		.or_else(|| panic.downcast_ref::<String>().cloned())
		.unwrap_or_else(|| "a route panicked".to_owned())
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;
