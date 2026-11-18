//! The browser's pages and their cookie (the Rust port of `page-routes.ts`): launch codes, the earlier-run page,
//! whiteboard's connection and the responder that serves all of it.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::{Value, json};
use url::{Url, form_urlencoded};

use crate::frontier::protocol::{ServiceState, TOKEN_QUERY, token_cookie};
use crate::frontier::server::assets::ui_asset;
use crate::frontier::server::routes::{BoxFuture, PageRoute, has_bearer, json_response, text_response};
use crate::frontier::server::service::{FindReview, OpenReview};
use crate::frontier::server::whiteboard::{
	CANVAS_PATH, CONNECTION_PATH, WHITEBOARD_VERSION, WORKER_PATH, Whiteboard, canvas_asset, canvas_dir, canvas_files,
	monaco_worker,
};
use crate::shared::errors::Error;
use crate::shared::fsx::same_token;

/// How long a page URL's one-time launch code may wait for the browser to open it (SEC-LOC-11).
const LAUNCH_CODE_MS: i64 = 60_000;

/// How long whiteboard's review lookup may take.
const REVIEW_LOOKUP: Duration = Duration::from_secs(10);

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The clock launch codes expire by: ms since the epoch.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// One-time launch codes of the page URLs handed out, with when each expires (SEC-LOC-11), oldest first.
pub struct LaunchCodes {
	codes: Mutex<Vec<(String, i64)>>,
	now: Clock,
}

impl Default for LaunchCodes {
	fn default() -> Self {
		Self::new(None)
	}
}

impl LaunchCodes {
	/// Launch codes by `now`, `Date.now` by default.
	pub fn new(now: Option<Clock>) -> Self {
		let now = now.unwrap_or_else(|| Arc::new(|| chrono::Utc::now().timestamp_millis()));
		Self { codes: Mutex::new(Vec::new()), now }
	}

	/// A fresh code, live for a minute.
	pub fn mint(&self) -> String {
		let now = (self.now)();
		let mut codes = locked(&self.codes);
		codes.retain(|(_, until)| *until > now);
		let mut bytes = [0_u8; 32];
		getrandom::fill(&mut bytes).expect("the OS has random bytes");
		let code: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
		codes.push((code.clone(), now + LAUNCH_CODE_MS));
		code
	}

	/// Takes `given` as a live launch code once.
	pub fn take(&self, given: &str) -> bool {
		let now = (self.now)();
		let mut codes = locked(&self.codes);
		let Some(at) = codes.iter().position(|(code, _)| same_token(Some(given), code)) else { return false };
		let (_, until) = codes.remove(at);
		until > now
	}
}

/// The value of `cookie` in the request's Cookie header.
pub fn cookie_value<B>(request: &axum::http::Request<B>, cookie: &str) -> Option<String> {
	let header = request.headers().get(header::COOKIE).map(|value| String::from_utf8_lossy(value.as_bytes()));
	for part in header.as_deref().unwrap_or("").split(';') {
		let mut pieces = part.trim().split('=');
		if pieces.next() == Some(cookie) {
			return Some(pieces.collect::<Vec<_>>().join("="));
		}
	}
	None
}

/// What a browser that opens a page without a valid cookie sees (0581, UX-9): the page is from an earlier run (every
/// run has its own page token), and what brings a fresh one. No token is in it.
const EARLIER_RUN_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Jocasta</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#1a1b26;color:#c0caf5;font:16px/1.5 system-ui,sans-serif}p{max-width:36em;padding:0 1.5em}</style>
</head>
<body><p>This Jocasta page is from an earlier run. Ask the boss to show the review again; it opens a fresh page.</p></body>
</html>
"#;

/// A refused request: the earlier-run page for a browser's navigation, else a plain 403.
pub fn refused_page<B>(request: &axum::http::Request<B>) -> Response {
	let accept = request.headers().get(header::ACCEPT).and_then(|value| value.to_str().ok()).unwrap_or("");
	if accept.contains("text/html") {
		let mut response = Response::new(Body::from(EARLIER_RUN_PAGE));
		*response.status_mut() = StatusCode::FORBIDDEN;
		response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
		response
	} else {
		text_response(StatusCode::FORBIDDEN, "Forbidden")
	}
}

fn unavailable(error: &str) -> Response {
	json_response(StatusCode::SERVICE_UNAVAILABLE, &json!({ "error": error }))
}

/// What the canvas page mounts whiteboard's canvas with (0393): the server's url and token, once it answers, and the
/// canvas build's files; 503 with the reason when either is missing, or when the server stopped answering since (its
/// process died, B-3): the page then shows its own failed state, not whiteboard's.
async fn whiteboard_connection(whiteboard: Option<&Whiteboard>) -> Response {
	let dir = canvas_dir();
	let (Some(whiteboard), Some(dir)) = (whiteboard, dir) else {
		return unavailable(if whiteboard.is_some() {
			"whiteboard's canvas isn't built: run `mise run build`"
		} else {
			"whiteboard is off"
		});
	};
	let connection = match whiteboard.ready().await {
		Ok(connection) => connection,
		Err(error) => return unavailable(&error.to_string()),
	};
	if whiteboard.check().await.state != ServiceState::Up {
		return unavailable(&format!("whiteboard's server at {} stopped answering", connection.url));
	}
	match canvas_files(&dir) {
		Ok(files) => json_response(
			StatusCode::OK,
			&json!({
				"url": connection.url,
				"token": connection.token,
				"version": WHITEBOARD_VERSION,
				"canvas": { "script": files.script, "styles": files.styles, "wasm": files.wasm },
				"worker": WORKER_PATH,
			}),
		),
		Err(error) => unavailable(&error.to_string()),
	}
}

/// `encodeURIComponent`.
fn encode_uri_component(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for byte in text.bytes() {
		if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
			out.push(char::from(byte));
		} else {
			out.push_str(&format!("%{byte:02X}"));
		}
	}
	out
}

/// What the boss's `show` needs of whiteboard (0552): the two service options it fills.
pub struct WhiteboardReviews {
	pub find_review: FindReview,
	pub open_review: OpenReview,
}

/// The boss's `show` with whiteboard (0552): it checks the review exists first (whiteboard's `GET /reviews-api/:id`,
/// `None` when none), then opens it in his browser on this server's canvas page with a fresh one-time launch code
/// (`open` is given the query that carries the review's id).
pub fn whiteboard_reviews(whiteboard: Arc<Whiteboard>, open: Arc<dyn Fn(&str) + Send + Sync>) -> WhiteboardReviews {
	let find_review: FindReview = Arc::new(move |id: String| {
		let whiteboard = whiteboard.clone();
		Box::pin(async move {
			let connection = whiteboard.ready().await?;
			let client = reqwest::Client::builder().timeout(REVIEW_LOOKUP).build()?;
			let response = client
				.get(format!("{}/reviews-api/{}?full=true", connection.url, encode_uri_component(&id)))
				.header("x-review-token", &connection.token)
				.send()
				.await?;
			if response.status() == reqwest::StatusCode::NOT_FOUND {
				return Ok(None);
			}
			if !response.status().is_success() {
				return Err(Error::other(format!("whiteboard answered {}", response.status().as_u16())));
			}
			let body: Value = response.json().await?;
			let title = body.get("title").and_then(Value::as_str).filter(|title| !title.is_empty());
			Ok(Some(title.map_or(id, str::to_owned)))
		})
	});
	let open_review: OpenReview = Arc::new(move |id: &str| open(&format!("&review={}", encode_uri_component(id))));
	WhiteboardReviews { find_review, open_review }
}

/// What the pages need.
pub struct PageParts {
	pub token: String,
	/// The browser's own token (SEC-NET-4): its cookie authorizes the pages and whiteboard's connection only.
	pub page_token: String,
	pub codes: Arc<LaunchCodes>,
	/// The server's port, for the cookie's name.
	pub port: Arc<dyn Fn() -> u16 + Send + Sync>,
	pub whiteboard: Option<Arc<Whiteboard>>,
	/// The bearer's routes answered here, after the launch code: `/health` and his questions.
	pub bearer_routes: Vec<Box<dyn PageRoute>>,
}

/// The page responder: a request and its url in, the response out.
pub type PageResponder = Arc<dyn Fn(Request, Url) -> BoxFuture<Response> + Send + Sync>;

fn not_found() -> Response {
	text_response(StatusCode::NOT_FOUND, "Not found")
}

/// A canvas build file as a response: its type and cache headers, its bytes.
async fn canvas_response(dir: &std::path::Path, pathname: &str) -> Option<Response> {
	let asset = canvas_asset(dir, pathname)?;
	let bytes = tokio::fs::read(&asset.path).await.ok()?;
	let mut response = Response::new(Body::from(bytes));
	let headers = response.headers_mut();
	headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(asset.content_type));
	headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(asset.cache_control));
	Some(response)
}

/// The launch code swapped for the page cookie (`/?t=<code>`): the redirect that sets it, else the refusal.
fn swap_launch_code(parts: &PageParts, request: &Request, url: &Url, given: &str) -> Response {
	if !parts.codes.take(given) {
		return refused_page(request);
	}
	let rest: Vec<(String, String)> = url
		.query_pairs()
		.filter(|(name, _)| name != TOKEN_QUERY)
		.map(|(n, v)| (n.into_owned(), v.into_owned()))
		.collect();
	let query = form_urlencoded::Serializer::new(String::new()).extend_pairs(rest).finish();
	let location = format!("/{}", if query.is_empty() { String::new() } else { format!("?{query}") });
	let cookie = format!("{}={}; Path=/; HttpOnly; SameSite=Strict", token_cookie((parts.port)()), parts.page_token);
	let mut response = Response::new(Body::empty());
	*response.status_mut() = StatusCode::FOUND;
	let headers = response.headers_mut();
	if let (Ok(location), Ok(cookie)) = (HeaderValue::from_str(&location), HeaderValue::from_str(&cookie)) {
		headers.insert(header::LOCATION, location);
		headers.insert(header::SET_COOKIE, cookie);
	}
	response
}

async fn respond(parts: &PageParts, request: Request, url: Url) -> Response {
	let from_query = url.query_pairs().find(|(name, _)| name == TOKEN_QUERY).map(|(_, value)| value.into_owned());
	if let Some(given) = from_query {
		return swap_launch_code(parts, &request, &url, &given);
	}
	if let Some(route) = parts.bearer_routes.iter().find(|route| route.matches(url.path())) {
		return route.handle(request, url).await;
	}
	// The page token as this port's cookie (the browser's pages and whiteboard's connection only; SEC-NET-4).
	let page_cookie = same_token(cookie_value(&request, &token_cookie((parts.port)())).as_deref(), &parts.page_token);
	if !has_bearer(&request, &parts.token) && !page_cookie {
		return refused_page(&request);
	}
	if request.method() != axum::http::Method::GET {
		return not_found();
	}
	let pathname = url.path();
	if pathname == CONNECTION_PATH {
		return whiteboard_connection(parts.whiteboard.as_deref()).await;
	}
	if pathname == WORKER_PATH {
		return monaco_worker();
	}
	let canvas = if pathname.starts_with(CANVAS_PATH) { canvas_dir() } else { None };
	let served = match canvas {
		Some(dir) => canvas_response(&dir, pathname).await,
		None => ui_asset(pathname).await,
	};
	served.unwrap_or_else(not_found)
}

/// The bearer's routes (`/health`, his questions) and the browser's pages: `/?t=<code>` swaps a live launch code for
/// the page cookie; the pages and their files need the cookie (or the bearer).
pub fn page_responder(parts: PageParts) -> PageResponder {
	let parts = Arc::new(parts);
	Arc::new(move |request, url| {
		let parts = parts.clone();
		Box::pin(async move { respond(&parts, request, url).await })
	})
}

#[cfg(test)]
#[path = "page_routes_tests.rs"]
mod tests;
