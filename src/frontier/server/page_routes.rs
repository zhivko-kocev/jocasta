//! The browser's pages and their cookie (the Rust port of `page-routes.ts`): launch codes, the earlier-run page, and
//! the responder that serves the briefs (0678) behind them.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use url::{Url, form_urlencoded};

use crate::frontier::brief::routes::BriefRoutes;
use crate::frontier::protocol::{TOKEN_QUERY, token_cookie};
use crate::frontier::server::routes::{BoxFuture, PageRoute, has_bearer, text_response};
use crate::shared::fsx::same_token;

/// How long a page URL's one-time launch code may wait for the browser to open it (SEC-LOC-11).
const LAUNCH_CODE_MS: i64 = 60_000;

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
/// run has its own page token), and what brings a fresh one. No token is in it, and no style: the content policy
/// (0678) allows none inline.
const EARLIER_RUN_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Jocasta</title></head>
<body><p>This Jocasta page is from an earlier run. Ask the boss to show it again; it opens a fresh page.</p></body>
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

/// What the pages need.
pub struct PageParts {
	pub token: String,
	/// The browser's own token (SEC-NET-4): its cookie authorizes the pages only.
	pub page_token: String,
	pub codes: Arc<LaunchCodes>,
	/// The server's port, for the cookie's name.
	pub port: Arc<dyn Fn() -> u16 + Send + Sync>,
	/// The briefs' routes (0678), behind the cookie.
	pub briefs: BriefRoutes,
	/// The bearer's routes answered here, after the launch code: `/health` and his questions.
	pub bearer_routes: Vec<Box<dyn PageRoute>>,
}

/// The page responder: a request and its url in, the response out.
pub type PageResponder = Arc<dyn Fn(Request, Url) -> BoxFuture<Response> + Send + Sync>;

fn not_found() -> Response {
	text_response(StatusCode::NOT_FOUND, "Not found")
}

/// The launch code swapped for the page cookie (`/brief/<id>?t=<code>`): the redirect that sets it and goes on to the
/// page without the code, else the refusal.
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
	let location = format!("{}{}", url.path(), if query.is_empty() { String::new() } else { format!("?{query}") });
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
	// The page token as this port's cookie (the browser's pages only; SEC-NET-4).
	let page_cookie = same_token(cookie_value(&request, &token_cookie((parts.port)())).as_deref(), &parts.page_token);
	if !has_bearer(&request, &parts.token) && !page_cookie {
		return refused_page(&request);
	}
	if BriefRoutes::matches(url.path()) {
		return parts.briefs.handle(request).await;
	}
	not_found()
}

/// The bearer's routes (`/health`, his questions) and the browser's pages: `/brief/<id>?t=<code>` swaps a live launch
/// code for the page cookie; the pages and their files need the cookie (or the bearer).
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
