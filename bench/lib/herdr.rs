//! One request to a herdr session, through the product's client (`herdr_request_once`): its framing, `HerdrError`s and
//! 0517's single retry of a dropped read. Only for the calls the `Herdr` interface has no method for
//! (`pane.wait_for_output`, `tab.close`, and H6's `agent.prompt` into a competitor's TUI); everything else goes through the client (`open_sandbox().herdr()`, BEN-18).

use serde::de::DeserializeOwned;
use serde_json::Value;

use jocasta::frontier::server::herdr::{HerdrResult, herdr_request_once};

/// `timeout_ms` is 10 s when `None`.
pub async fn herdr_request<T: DeserializeOwned>(
	session: &str,
	method: &str,
	params: &Value,
	timeout_ms: Option<i64>,
) -> HerdrResult<T> {
	herdr_request_once(session, method, params, Some(timeout_ms.unwrap_or(10_000))).await
}
