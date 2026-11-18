//! The benchmark's side of the cost ledger (0538, 0546; the rows, waste and peak are the product's
//! `shared::ledger`): each trial's saved records, read once at its end: omp's session files (`.jsonl`, subagents' files
//! beside the main one), Claude Code's transcripts (`.jsonl`, subagents under `<session>/subagents/`) and opencode's
//! `opencode export` JSON, priced at the pinned table.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::lib::prices::{PriceTable, RequestTokens, price_request};
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::as_object;
use jocasta::shared::jsonl::json_lines;
use jocasta::shared::ledger::{
	LedgerHarness, LedgerPricing, RequestFields, SessionRecord, ToolCall, TrialLedger, new_record, push_tool,
	read_omp_session, request_row, result_text, string_or_unknown, time_of, token_count, trial_ledger, utf8_bytes,
};

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;

/// The ledger's pricing from the pinned table: a request at [`price_request`], the window its `context_window`.
pub struct BenchPricing<'a> {
	prices: &'a PriceTable,
}

pub fn bench_pricing(prices: &PriceTable) -> BenchPricing<'_> {
	BenchPricing { prices }
}

impl LedgerPricing for BenchPricing<'_> {
	fn cost(&self, request: &RequestFields, _usage: &Map<String, Value>) -> Option<f64> {
		let price = self.prices.lookup(request.provider.as_deref(), &request.model)?;
		Some(price_request(&price, &RequestTokens::from(request)))
	}

	fn window(&self, provider: Option<&str>, model: &str) -> Option<u64> {
		self.prices.lookup(provider, model)?.context_window
	}
}

/// A message's id and first time, and its last line (the response's final usage).
struct Response {
	id: String,
	at: Option<i64>,
	message: Map<String, Value>,
}

struct Call {
	at: Option<i64>,
	id: String,
	tool: String,
	args: Value,
}

fn field<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	map.get(key).and_then(Value::as_str)
}

/// One Claude Code transcript: an API response is written as several `assistant` lines sharing `message.id`, each with
/// the response's usage, so a request is one id (its last line's usage); `<synthetic>` messages aren't requests.
/// `tool_result` blocks in user lines carry results; a `compact_boundary` system line is a compaction.
pub fn read_claude_transcript(text: &str, pricing: &dyn LedgerPricing, agent: Option<&str>) -> SessionRecord {
	let agent = agent.unwrap_or("main");
	let entries: Vec<Map<String, Value>> = json_lines(text);
	let session_id = entries.iter().find_map(|entry| field(entry, "sessionId"));
	let mut record = new_record(LedgerHarness::Claude, session_id.unwrap_or(agent), agent);
	let mut results: HashMap<String, (usize, bool)> = HashMap::new();
	let mut responses: Vec<Response> = Vec::new();
	let mut calls: Vec<Call> = Vec::new();
	for entry in &entries {
		let at = entry.get("timestamp").and_then(time_of);
		let message = entry.get("message").and_then(as_object);
		let kind = field(entry, "type");
		if kind == Some("system") && field(entry, "subtype") == Some("compact_boundary") {
			record.compactions.push(at);
		}
		if kind == Some("user")
			&& let Some(blocks) = message.and_then(|m| m.get("content")).and_then(Value::as_array)
		{
			for block in blocks.iter().filter_map(as_object) {
				if field(block, "type") == Some("tool_result")
					&& let Some(id) = field(block, "tool_use_id")
				{
					let bytes = utf8_bytes(&block.get("content").map(result_text).unwrap_or_default());
					results.insert(id.to_owned(), (bytes, block.get("is_error") == Some(&Value::Bool(true))));
				}
			}
		}
		let Some(message) =
			message.filter(|message| kind == Some("assistant") && field(message, "model") != Some("<synthetic>"))
		else {
			continue;
		};
		let id = field(message, "id")
			.map_or_else(|| at.map_or_else(|| "NaN".to_owned(), |at| at.to_string()), str::to_owned);
		match responses.iter_mut().find(|response| response.id == id) {
			Some(first) => first.message = message.clone(),
			None => responses.push(Response { id, at, message: message.clone() }),
		}
		let blocks = message.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
		for block in blocks.iter().filter_map(as_object) {
			if field(block, "type") == Some("tool_use")
				&& let Some(id) = field(block, "id")
			{
				let tool = string_or_unknown(block.get("name"));
				calls.push(Call {
					at,
					id: id.to_owned(),
					tool,
					args: block.get("input").cloned().unwrap_or(Value::Null),
				});
			}
		}
	}
	let none = Map::new();
	for Response { at, message, .. } in &responses {
		let usage = message.get("usage").and_then(as_object).unwrap_or(&none);
		let creation = usage.get("cache_creation").and_then(as_object).unwrap_or(&none);
		let count = |map: &Map<String, Value>, key: &str| map.get(key).map_or(0, token_count);
		let fields = RequestFields {
			at: *at,
			provider: Some("anthropic".to_owned()),
			model: string_or_unknown(message.get("model")),
			input: count(usage, "input_tokens"),
			cache_read: count(usage, "cache_read_input_tokens"),
			cache_write: count(usage, "cache_creation_input_tokens"),
			cache_write_1h: count(creation, "ephemeral_1h_input_tokens"),
			output: count(usage, "output_tokens"),
			purpose: None,
		};
		let row = request_row(&record, pricing, fields, usage);
		record.requests.push(row);
	}
	for call in &calls {
		let result = results.get(&call.id);
		let tool = ToolCall {
			at: call.at,
			tool: &call.tool,
			args: &call.args,
			result_bytes: result.map(|(bytes, _)| *bytes),
			error: result.is_some_and(|(_, error)| *error),
		};
		push_tool(&mut record, &tool);
	}
	record.requests.sort_by_key(|request| request.at.unwrap_or(i64::MAX));
	record
}

/// `value` as an object, or `none` (the TypeScript's `(x ?? {}) as Record`).
fn object_or<'a>(value: Option<&'a Value>, none: &'a Map<String, Value>) -> &'a Map<String, Value> {
	value.and_then(as_object).unwrap_or(none)
}

/// One `opencode export <session>` JSON: each `step-finish` part of an assistant message is a model request (output
/// plus reasoning tokens count as output), timed at the latest part end before it or the message's start; `tool` parts
/// are calls, `compaction` parts compactions.
pub fn read_opencode_export(text: &str, pricing: &dyn LedgerPricing, agent: Option<&str>) -> Result<SessionRecord> {
	let agent = agent.unwrap_or("main");
	let exported: Value = serde_json::from_str(text)?;
	let id = exported.get("info").and_then(as_object).and_then(|info| field(info, "id"));
	let mut record = new_record(LedgerHarness::Opencode, id.unwrap_or(agent), agent);
	let none = Map::new();
	let messages = exported.get("messages").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
	for message in messages.iter().filter_map(as_object) {
		let info = object_or(message.get("info"), &none);
		let mut clock = object_or(info.get("time"), &none).get("created").and_then(time_of);
		let parts = message.get("parts").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
		for part in parts.iter().filter_map(as_object) {
			let state = object_or(part.get("state"), &none);
			let state_time = object_or(state.get("time"), &none);
			match field(part, "type") {
				Some("compaction") => record.compactions.push(clock),
				Some("tool") => {
					let output = field(state, "output").or_else(|| field(state, "error"));
					let tool = string_or_unknown(part.get("tool"));
					let call = ToolCall {
						at: state_time.get("start").and_then(time_of).or(clock),
						tool: &tool,
						args: state.get("input").unwrap_or(&Value::Null),
						result_bytes: output.map(utf8_bytes),
						error: field(state, "status") == Some("error"),
					};
					push_tool(&mut record, &call);
				}
				Some("step-finish") if field(info, "role") == Some("assistant") => {
					let tokens = object_or(part.get("tokens"), &none);
					let cache = object_or(tokens.get("cache"), &none);
					let count = |map: &Map<String, Value>, key: &str| map.get(key).map_or(0, token_count);
					let fields = RequestFields {
						at: clock,
						provider: field(info, "providerID").map(str::to_owned),
						model: string_or_unknown(info.get("modelID")),
						input: count(tokens, "input"),
						cache_read: count(cache, "read"),
						cache_write: count(cache, "write"),
						cache_write_1h: 0,
						output: count(tokens, "output") + count(tokens, "reasoning"),
						purpose: None,
					};
					let row = request_row(&record, pricing, fields, tokens);
					record.requests.push(row);
				}
				_ => {}
			}
			let part_time = object_or(part.get("time"), &none);
			for end in [part_time.get("end").and_then(time_of), state_time.get("end").and_then(time_of)] {
				// A clock that isn't a time (NaN) stays one: no end is later than it.
				if let (Some(end), Some(now)) = (end, clock)
					&& end > now
				{
					clock = Some(end);
				}
			}
		}
	}
	Ok(record)
}

/// The files under `dir` (recursively) with `extension` (`.jsonl`), sorted by path; none when `dir` is missing.
fn files_under(dir: &Path, extension: &str) -> Vec<PathBuf> {
	fn walk(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
		let Ok(entries) = fs::read_dir(dir) else { return };
		for entry in entries.flatten() {
			let path = entry.path();
			if path.is_dir() {
				walk(&path, extension, found);
			} else if path.is_file()
				&& path.extension().is_some_and(|ext| ext.to_string_lossy() == extension.trim_start_matches('.'))
			{
				found.push(path);
			}
		}
	}
	let mut found = Vec::new();
	walk(dir, extension, &mut found);
	found.sort();
	found
}

static JSON_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.jsonl?$").expect("a fixed pattern"));

/// The agent a session file is: `main` beside the others, else its path under `dir` without the extension.
fn agent_of(dir: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
	if rel.contains('/') { JSON_SUFFIX.replace(&rel, "").into_owned() } else { "main".to_owned() }
}

/// The sessions saved for one trial in `dir`: omp's and Claude Code's `.jsonl` files (a file in a subfolder is a
/// subagent, named by its path), opencode's exported `.json` files. Files that aren't a session are skipped.
pub fn read_trial_sessions(
	dir: &Path,
	harness: LedgerHarness,
	pricing: &dyn LedgerPricing,
) -> Result<Vec<SessionRecord>> {
	let mut records = Vec::new();
	if harness == LedgerHarness::Opencode {
		for path in files_under(dir, ".json") {
			let text = String::from_utf8_lossy(&fs::read(&path)?).into_owned();
			records.push(read_opencode_export(&text, pricing, Some(&agent_of(dir, &path)))?);
		}
		return Ok(records);
	}
	for path in files_under(dir, ".jsonl") {
		let text = String::from_utf8_lossy(&fs::read(&path)?).into_owned();
		let agent = agent_of(dir, &path);
		let record = if harness == LedgerHarness::Omp {
			read_omp_session(&text, pricing, Some(&agent))
		} else {
			read_claude_transcript(&text, pricing, Some(&agent))
		};
		if !record.requests.is_empty() || !record.tools.is_empty() {
			records.push(record);
		}
	}
	Ok(records)
}

/// A trial's ledger from its saved sessions folder.
pub fn ledger_of_dir(dir: &Path, harness: LedgerHarness, pricing: &dyn LedgerPricing) -> Result<TrialLedger> {
	Ok(trial_ledger(&read_trial_sessions(dir, harness, pricing)?, pricing))
}
