//! How Jocasta's own programs reach the running Jocasta service: the push hook (0613), a sleeping pane's wake (0676)
//! and a brief window's buttons (0721).
//! Its port and token come from
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

/// One `cli` command to the service over the bridge pipe, waiting up to `secs`: its answer text, or why none came
/// (Jocasta isn't running, didn't answer, or refused it with its own words).
async fn cli_call(command: &str, args: serde_json::Value, secs: u64) -> Result<String, String> {
	let at = running_service().ok_or_else(|| NOT_RUNNING.to_owned())?;
	let mut peer = CliPeer::open(at.port, &at.token).await.map_err(|error| failed(&error))?;
	let message = json!({ "t": "cli", "command": command, "args": args });
	let answer = tokio::time::timeout(Duration::from_secs(secs), peer.request(message))
		.await
		.map_err(|_| "Jocasta didn't answer in time".to_owned())?
		.map_err(|error| failed(&error))?;
	if answer.get("ok") == Some(&json!(false)) {
		return Err(answer.get("error").and_then(serde_json::Value::as_str).unwrap_or("Jocasta refused it").to_owned());
	}
	Ok(answer.get("text").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned())
}

/// Asks the service to wake the sleeping pane started with `pane_key` (0676): resolves once the service has started omp
/// in it again (which ends the asker's own process, so an answer is only seen for a refusal). `Err` is why not.
pub async fn ask_wake(pane_key: &str) -> Result<(), String> {
	cli_call("wake", json!({ "key": pane_key }), 60).await.map(drop)
}

/// Runs a brief's action through the service (0721), which may wait for his Allow: the brief as it is after (with a
/// `polish` or `english` suggestion), or why not.
pub async fn brief_act(id: &str, action: serde_json::Value) -> Result<serde_json::Value, String> {
	let text = cli_call("brief act", json!({ "id": id, "action": action }), 150).await?;
	serde_json::from_str(&text).map_err(|error| format!("Jocasta's answer wasn't a brief: {error}"))
}

/// Whether the brief `id` still has a live thread behind its buttons (0721); `Err` when the service didn't answer.
pub async fn brief_live(id: &str) -> Result<bool, String> {
	Ok(cli_call("brief live", json!({ "id": id }), 10).await? == "true")
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
