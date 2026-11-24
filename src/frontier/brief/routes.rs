//! The brief routes (0678): the page, its data, its two assets and its actions, answered among the browser's pages
//! (after the cookie check of `page_routes.rs`). `GET /brief/<id>` is the page, `GET /brief/<id>.json` its brief,
//! `GET /brief/assets/brief.js|brief.css` the embedded page files and `POST /brief/<id>/act` an action whose answer is
//! the whole updated brief. Every request needs this server's own `Host`; a `POST` also its own `Origin` and
//! `content-type: application/json`.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use serde_json::{Value, json};

use crate::frontier::brief::model::Brief;
use crate::frontier::brief::store;
use crate::frontier::server::routes::{BoxFuture, host_allowed, json_response, text_response};
use crate::shared::errors::{Error, Result};

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;

/// The page's files, embedded: no framework, no external URL.
const PAGE_HTML: &str = include_str!("page/brief.html");
const PAGE_JS: &str = include_str!("page/brief.js");
const PAGE_CSS: &str = include_str!("page/brief.css");

/// Every brief response's policy (0678): the page loads its two files from this server and fetches from it, nothing else.
pub const CONTENT_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// The most an action's body may hold.
const MAX_ACTION_BODY: usize = 1 << 20;

/// What runs a brief's actions: the brief's id and the action as the page sent it; the updated brief, or the refusal.
pub trait BriefActions: Send + Sync {
	fn act(&self, id: String, action: Value) -> BoxFuture<Result<Brief>>;
}

/// The routes of `/brief/…`.
pub struct BriefRoutes {
	port: Arc<dyn Fn() -> u16 + Send + Sync>,
	actions: Arc<dyn BriefActions>,
}

fn not_found() -> Response {
	text_response(StatusCode::NOT_FOUND, "Not found")
}

fn forbidden() -> Response {
	text_response(StatusCode::FORBIDDEN, "Forbidden")
}

fn asset(text: &'static str, content_type: &'static str) -> Response {
	let mut response = Response::new(Body::from(text));
	let headers = response.headers_mut();
	headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
	headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
	response
}

/// The text a refused action answers: `{"error": "…"}`.
fn error_response(status: StatusCode, message: &str) -> Response {
	json_response(status, &json!({ "error": message }))
}

impl BriefRoutes {
	/// Routes on the service port (read at each request) that hand a brief's actions to `actions`.
	pub fn new(port: Arc<dyn Fn() -> u16 + Send + Sync>, actions: Arc<dyn BriefActions>) -> Self {
		Self { port, actions }
	}

	/// Whether `path` is a brief route's.
	pub fn matches(path: &str) -> bool {
		path.starts_with("/brief/")
	}

	/// One request's answer.
	pub async fn handle(&self, request: Request) -> Response {
		let port = (self.port)();
		let host = request.headers().get(header::HOST).and_then(|value| value.to_str().ok()).map(str::to_owned);
		if !host_allowed(host.as_deref(), port) {
			return forbidden();
		}
		let path = request.uri().path().to_owned();
		let rest = path.strip_prefix("/brief/").unwrap_or_default();
		match (request.method().clone(), rest) {
			(Method::GET, "assets/brief.js") => asset(PAGE_JS, "text/javascript; charset=utf-8"),
			(Method::GET, "assets/brief.css") => asset(PAGE_CSS, "text/css; charset=utf-8"),
			(Method::GET, _) => Self::read(rest),
			(Method::POST, _) => match rest.strip_suffix("/act") {
				Some(id) => self.act(request, id, host.as_deref().unwrap_or_default()).await,
				None => not_found(),
			},
			_ => text_response(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed"),
		}
	}

	/// `<id>` (the page) or `<id>.json` (its brief).
	fn read(rest: &str) -> Response {
		let (id, json) = match rest.strip_suffix(".json") {
			Some(id) => (id, true),
			None => (rest, false),
		};
		let Some(brief) = store::load(id) else { return not_found() };
		if json {
			json_response(StatusCode::OK, &serde_json::to_value(&brief).unwrap_or(Value::Null))
		} else {
			let mut response = Response::new(Body::from(PAGE_HTML));
			let headers = response.headers_mut();
			headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
			headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
			response
		}
	}

	async fn act(&self, request: Request, id: &str, host: &str) -> Response {
		let origin = request.headers().get(header::ORIGIN).and_then(|value| value.to_str().ok());
		if origin != Some(format!("http://{host}").as_str()) {
			return forbidden();
		}
		let content_type = request.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok());
		let is_json = content_type.is_some_and(|value| {
			value.split(';').next().is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json"))
		});
		if !is_json {
			return error_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, "an action is sent as application/json");
		}
		if store::load(id).is_none() {
			return not_found();
		}
		let Ok(bytes) = axum::body::to_bytes(request.into_body(), MAX_ACTION_BODY).await else {
			return error_response(StatusCode::PAYLOAD_TOO_LARGE, "the action is too large");
		};
		let Ok(action) = serde_json::from_slice::<Value>(&bytes) else {
			return error_response(StatusCode::BAD_REQUEST, "the action isn't JSON");
		};
		match self.actions.act(id.to_owned(), action).await {
			Ok(brief) => json_response(StatusCode::OK, &serde_json::to_value(&brief).unwrap_or(Value::Null)),
			Err(Error::User { message, .. }) => error_response(StatusCode::BAD_REQUEST, &message),
			Err(other) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &other.to_string()),
		}
	}
}
