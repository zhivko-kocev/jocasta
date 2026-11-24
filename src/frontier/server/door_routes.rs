//! The HTTP door that is left (0675): bare `jocasta`'s shutdown. The panes' tools, the `jocasta thread|worktree` words,
//! git's pre-push hook and the turn-end check all speak to the service over the bridge (`bridge.rs`); `/health` is
//! `health_route.rs`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use url::Url;

use crate::frontier::server::launch::SHUTDOWN_PATH;
use crate::frontier::server::routes::{BoxFuture, Route, ServerHandle, refused_post};

/// How long after the answer `POST /api/shutdown` waits to start the close.
const SHUTDOWN_DELAY: Duration = Duration::from_millis(100);

/// What the door needs.
pub struct DoorParts {
	pub token: String,
	/// His client closed (`POST /api/shutdown`): the close, asking first.
	pub client_closed: Arc<dyn Fn() + Send + Sync>,
}

struct ShutdownRoute {
	parts: Arc<DoorParts>,
}

impl ShutdownRoute {
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

impl Route for ShutdownRoute {
	/// `POST /api/shutdown` from bare `jocasta` (0434, 0549): his client closed. Answered at once, since it may come
	/// from a closing terminal with seconds left.
	fn matches(&self, path: &str) -> bool {
		path == SHUTDOWN_PATH
	}

	fn handle(&self, request: Request, _url: Url, _server: ServerHandle) -> BoxFuture<Response> {
		let response = self.shutdown(&request);
		Box::pin(async move { response })
	}
}

/// The door: bare `jocasta`'s shutdown.
pub fn door_routes(parts: DoorParts) -> Vec<Box<dyn Route>> {
	vec![Box::new(ShutdownRoute { parts: Arc::new(parts) })]
}

#[cfg(test)]
#[path = "door_routes_tests.rs"]
mod tests;
