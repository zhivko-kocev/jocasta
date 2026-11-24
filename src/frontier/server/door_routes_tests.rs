//! Tests of `door_routes.rs`: bare `jocasta`'s shutdown, on a listening server as `routes_tests.rs` does.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::frontier::server::routes::{Served, listen, text_response};
use crate::shared::testing::temp_jocasta;

struct Running {
	closed: Arc<AtomicUsize>,
	server: ServerHandle,
	client: reqwest::Client,
}

impl Running {
	fn url(&self, path: &str) -> String {
		format!("http://127.0.0.1:{}{path}", self.server.port())
	}

	async fn post(&self, path: &str, token: Option<&str>, body: &str) -> reqwest::Response {
		let mut request = self.client.post(self.url(path)).body(body.to_owned());
		if let Some(token) = token {
			request = request.bearer_auth(token);
		}
		request.send().await.unwrap()
	}
}

fn running() -> Running {
	let closed = Arc::new(AtomicUsize::new(0));
	let count = closed.clone();
	let routes = door_routes(DoorParts {
		token: "t".to_owned(),
		client_closed: Arc::new(move || {
			count.fetch_add(1, Ordering::SeqCst);
		}),
	});
	let server = listen(
		0,
		Served {
			routes,
			page: Box::new(|_request, _url, _port| Box::pin(async { text_response(StatusCode::NOT_FOUND, "page") })),
		},
	)
	.unwrap();
	Running { closed, server, client: reqwest::Client::new() }
}

#[tokio::test]
async fn shutdown_is_answered_202_at_once_and_the_close_follows_after_the_answer() {
	let _j = temp_jocasta();
	let r = running();
	let none = r.post("/api/shutdown", None, "").await;
	assert_eq!(none.status(), 401);
	assert_eq!(none.headers()["content-type"], "text/plain;charset=utf-8");
	assert_eq!(none.text().await.unwrap(), "invalid token");
	let get = r.client.get(r.url("/api/shutdown")).bearer_auth("t").send().await.unwrap();
	assert_eq!((get.status().as_u16(), get.text().await.unwrap().as_str()), (405, "POST only"));
	assert_eq!(r.closed.load(Ordering::SeqCst), 0);
	let accepted = r.post("/api/shutdown", Some("t"), "").await;
	assert_eq!(accepted.status(), 202);
	assert_eq!(accepted.text().await.unwrap(), "");
	assert_eq!(r.closed.load(Ordering::SeqCst), 0);
	for _ in 0..200 {
		if r.closed.load(Ordering::SeqCst) > 0 {
			break;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	assert_eq!(r.closed.load(Ordering::SeqCst), 1);
	r.server.stop(true).await;
}

#[tokio::test]
async fn the_door_answers_its_own_path_only_and_the_old_doors_are_gone() {
	let r = running();
	for (path, expected) in [
		("/api/shutdown", true),
		("/api/shutdowns", false),
		("/api/mcp/x", false),
		("/api/cli", false),
		("/api/push", false),
		("/api/pane-check", false),
	] {
		let routes = door_routes(DoorParts { token: "t".to_owned(), client_closed: Arc::new(|| {}) });
		assert_eq!(routes.iter().filter(|route| route.matches(path)).count(), usize::from(expected), "{path}");
	}
	for gone in ["/api/mcp/x", "/api/cli", "/api/push", "/api/pane-check"] {
		assert_eq!(r.client.post(r.url(gone)).send().await.unwrap().text().await.unwrap(), "page", "{gone}");
	}
	r.server.stop(true).await;
}
