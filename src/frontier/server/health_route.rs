//! `GET /health` (the Rust port of `health-route.ts`).

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::{Map, Value, json};
use url::Url;

use crate::frontier::server::routes::{BoxFuture, PageRoute, has_bearer};
use crate::shared::layer::{hosts_on, layer_on};

/// The `/health` route: see [`health_route`].
pub struct HealthRoute {
	token: String,
	closing: Box<dyn Fn() -> bool + Send + Sync>,
}

/// `GET /health` with the bearer: `closing` while Jocasta is closing, so a `jocasta` started meanwhile waits instead of
/// attaching to it (LC-3); `layer` and `hosts`: the test-only switches as this service has them, which the benchmarks
/// check (0576).
pub fn health_route(token: &str, closing: impl Fn() -> bool + Send + Sync + 'static) -> HealthRoute {
	HealthRoute { token: token.to_owned(), closing: Box::new(closing) }
}

fn on_off(on: bool) -> &'static str {
	if on { "on" } else { "off" }
}

impl HealthRoute {
	fn answer(&self, request: &Request) -> Response {
		if !has_bearer(request, &self.token) {
			let mut response = Response::new(Body::from("Forbidden"));
			*response.status_mut() = StatusCode::FORBIDDEN;
			response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain;charset=utf-8"));
			return response;
		}
		let mut body = Map::new();
		body.insert("pid".to_owned(), json!(std::process::id()));
		body.insert("layer".to_owned(), json!(on_off(layer_on(None))));
		body.insert("hosts".to_owned(), json!(on_off(hosts_on(None))));
		if (self.closing)() {
			body.insert("closing".to_owned(), json!(true));
		}
		let mut response = Response::new(Body::from(Value::Object(body).to_string()));
		response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json;charset=utf-8"));
		response
	}
}

impl PageRoute for HealthRoute {
	fn matches(&self, path: &str) -> bool {
		path == "/health"
	}

	fn handle(&self, request: Request, _url: Url) -> BoxFuture<Response> {
		let response = self.answer(&request);
		Box::pin(async move { response })
	}
}

#[cfg(test)]
#[path = "health_route_tests.rs"]
mod tests;
