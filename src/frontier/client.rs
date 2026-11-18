//! How Jocasta's own programs reach the running Jocasta service: the push hook (0613) and a project pane's check (0617).
//! Its port and token come from `<data>/jocasta-service.json` when its process is alive; each call goes over HTTP with
//! the token as a bearer.

use std::time::Duration;

use serde_json::json;

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

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
