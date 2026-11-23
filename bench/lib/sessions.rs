//! omp session files (`~/.omp/agent/sessions/<cwd>/<time>_<id>.jsonl`, omp 19.1.1): one entry per line with an ISO
//! `timestamp`; `message` entries carry `{role, content, timestamp?, completedAt?}`. Times follow the product's
//! `sessions` `entryTime`: the message's own epoch-ms `timestamp` when non-zero, else the entry's ISO time.
//!
//! A time is epoch ms as an `f64`, NaN when the entry has none, as in the TypeScript.

use serde_json::{Map, Value};

use jocasta::shared::fsx::as_object;
use jocasta::shared::jsonl::json_lines;
use jocasta::shared::omp_session::date_parse;

type Entry = Map<String, Value>;

/// One `message` entry: its time, the entry and its message object.
struct Message {
	at: f64,
	entry: Entry,
	message: Map<String, Value>,
}

fn field<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	map.get(key).and_then(Value::as_str)
}

/// The entry's ISO `timestamp` as epoch ms, NaN when it has none.
fn iso_time(entry: &Entry) -> f64 {
	field(entry, "timestamp").and_then(date_parse).map_or(f64::NAN, |ms| ms as f64)
}

/// `message[key]` when it is a positive number.
fn positive(message: &Map<String, Value>, key: &str) -> Option<f64> {
	message.get(key).and_then(Value::as_f64).filter(|value| *value > 0.0)
}

fn entry_time(entry: &Entry, message: Option<&Map<String, Value>>) -> f64 {
	message.and_then(|message| positive(message, "timestamp")).unwrap_or_else(|| iso_time(entry))
}

/// When an assistant message was complete (0482's "needs you" moment for an ask): omp stamps the message's own
/// `timestamp` when it builds the request, before the model streams (`anthropic.ts`), and `completedAt` at
/// `message_end` (`agent-session.ts`), when the entry's ISO time is written too. So `completedAt`, else the entry's
/// time, never `message.timestamp`, which would put the model's generation inside H2's t1−t0 and t3−t0.
fn completed_time(entry: &Entry, message: &Map<String, Value>) -> f64 {
	positive(message, "completedAt").unwrap_or_else(|| iso_time(entry))
}

fn messages(text: &str) -> Vec<Message> {
	json_lines::<Entry>(text)
		.into_iter()
		.filter(|entry| field(entry, "type") == Some("message"))
		.filter_map(|entry| {
			let message = entry.get("message").and_then(as_object)?.clone();
			Some(Message { at: entry_time(&entry, Some(&message)), entry, message })
		})
		.collect()
}

/// The session's start: its header's time.
pub fn session_start(text: &str) -> f64 {
	let entries: Vec<Entry> = json_lines(text);
	entries.iter().find(|entry| field(entry, "type") == Some("session")).map_or(f64::NAN, iso_time)
}

/// When the agent asked him something: assistant messages with an `ask` tool call (the product's `sessions` reads the
/// same), at the message's completion ([`completed_time`]).
pub fn ask_times(text: &str) -> Vec<f64> {
	messages(text)
		.iter()
		.filter(|m| {
			field(&m.message, "role") == Some("assistant")
				&& m.message.get("content").and_then(Value::as_array).is_some_and(|parts| {
					parts
						.iter()
						.filter_map(as_object)
						.any(|part| field(part, "type") == Some("toolCall") && field(part, "name") == Some("ask"))
				})
		})
		.map(|m| completed_time(&m.entry, &m.message))
		.collect()
}

/// When a plan became ready for his review: a successful `propose` (omp 19.1.1: a direct `propose` result, or a bash run
/// whose `details.toolCommands` holds a `propose` execution that exited 0; the product's `sessions` `proposedPlan`).
pub fn plan_ready_times(text: &str) -> Vec<f64> {
	messages(text)
		.iter()
		.filter(|m| {
			if field(&m.message, "role") != Some("toolResult") {
				return false;
			}
			match field(&m.message, "toolName") {
				Some("propose") => m.message.get("isError") != Some(&Value::Bool(true)),
				Some("bash") => m
					.message
					.get("details")
					.and_then(as_object)
					.and_then(|details| details.get("toolCommands"))
					.and_then(Value::as_array)
					.is_some_and(|records| {
						records.iter().filter_map(as_object).any(|record| {
							field(record, "tool") == Some("propose")
								&& field(record, "mode") == Some("execute")
								&& record.get("exitCode").and_then(Value::as_f64) == Some(0.0)
						})
					}),
				_ => false,
			}
		})
		.map(|m| m.at)
		.collect()
}

/// When he left plan mode after a plan was ready (his approval): a `mode_change` away from `plan`, counted only when it
/// leaves `plan` (the mode before it was `plan`, or no mode was recorded before it). omp 19.1.1 writes a second
/// `mode_change` to `none` with `data.approvedPlanFilePath` once an approval is through, so
/// "Approve and keep context" leaves two in one file (0589).
pub fn plan_approval_times(text: &str) -> Vec<f64> {
	let ready = plan_ready_times(text);
	let entries: Vec<Entry> = json_lines(text);
	let mut mode: Option<&Value> = None;
	let mut left: Vec<&Entry> = Vec::new();
	for entry in entries.iter().filter(|entry| field(entry, "type") == Some("mode_change")) {
		let now = entry.get("mode");
		let to_plan = now == Some(&Value::String("plan".to_owned()));
		let from_plan = mode.is_none_or(|before| before == "plan");
		if !to_plan && from_plan {
			left.push(entry);
		}
		mode = now;
	}
	left.into_iter()
		.map(|entry| entry_time(entry, None))
		.filter(|at| ready.iter().any(|ready_at| ready_at <= at))
		.collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserMessage {
	pub at: f64,
	pub text: String,
}

/// The text of a message's content: a string, or its text parts joined by newlines.
fn content_text(content: Option<&Value>) -> String {
	match content {
		Some(Value::String(text)) => text.clone(),
		Some(Value::Array(parts)) => parts
			.iter()
			.filter_map(as_object)
			.filter(|part| field(part, "type") == Some("text"))
			.filter_map(|part| field(part, "text"))
			.collect::<Vec<_>>()
			.join("\n"),
		_ => String::new(),
	}
}

/// The user-role messages that aren't synthetic, with their text.
pub fn user_messages(text: &str) -> Vec<UserMessage> {
	messages(text)
		.iter()
		.filter(|m| field(&m.message, "role") == Some("user") && m.message.get("synthetic") != Some(&Value::Bool(true)))
		.map(|m| UserMessage { at: m.at, text: content_text(m.message.get("content")) })
		.collect()
}
