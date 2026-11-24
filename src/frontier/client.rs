//! How Jocasta's own programs reach the running Jocasta service: the push hook (0613). Its port and token come from
//! `<data>/jocasta-service.json` when its process is alive; the call goes over the bridge pipe as a CLI peer (0675).

use std::time::Duration;

use serde_json::json;

use crate::shared::bridge_wire::{CliPeer, unreachable};
use crate::shared::push_hook::{PushReply, PushRequest};
use crate::shared::service_file::{NOT_RUNNING, running_service};

/// Why a call to the service failed: nothing listening is the one "not running" text (0581 UX-13).
fn failed(error: &std::io::Error) -> String {
	if unreachable(error) { NOT_RUNNING.to_owned() } else { format!("Jocasta didn't answer: {error}") }
}

/// Asks the service whether a push may go ahead (0613), which may wait for his Allow up to 2 minutes; an `Err` is why no
/// answer came (Jocasta isn't running, or didn't answer). The question goes over the bridge pipe (0675).
pub async fn ask_push(request: &PushRequest) -> PushReply {
	let at = running_service().ok_or_else(|| NOT_RUNNING.to_owned())?;
	let mut peer = CliPeer::open(at.port, &at.token).await.map_err(|error| failed(&error))?;
	let mut message =
		serde_json::to_value(request).map_err(|error| format!("Jocasta didn't take the push: {error}"))?;
	if let Some(object) = message.as_object_mut() {
		object.insert("t".to_owned(), json!("push"));
	}
	let answer = tokio::time::timeout(Duration::from_secs(150), peer.request(message))
		.await
		.map_err(|_| "Jocasta didn't answer in time".to_owned())?
		.map_err(|error| failed(&error))?;
	if answer.get("ok") != Some(&json!(true)) {
		return Err(format!("Jocasta answered: {answer}"));
	}
	serde_json::from_value(answer).map_err(|error| format!("Jocasta's answer wasn't a push answer: {error}"))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
