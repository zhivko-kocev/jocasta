//! The cost ledger (0538, 0546), one implementation for the product and the benchmark: rows computed from saved
//! session records. Per model request: time, model, uncached input, cache read, cache write, output and its cost; per
//! tool call: the tool, an argument hash, the file path, the result's size and whether it failed. Per session: the
//! waste counts and the peak context. What a request costs and the model's window come from a [`LedgerPricing`]:
//! the product's is omp's own (`usage.cost.total`, `models.db`), the benchmark's its pinned price table.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::shared::fsx::as_object;
use crate::shared::jsonl::json_lines;
use crate::shared::omp_session::{FileStat, artifacts_dir, date_parse, transcripts_of};
use crate::shared::paths::omp_agent_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerHarness {
	Omp,
	Claude,
	Opencode,
}

/// A request's fields as a reader has them, before it is priced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestFields {
	/// Epoch ms; `None` when the entry has no usable time (the TypeScript's `NaN`, JSON `null`).
	pub at: Option<i64>,
	pub provider: Option<String>,
	pub model: String,
	pub input: u64,
	pub cache_read: u64,
	pub cache_write: u64,
	/// The part of `cache_write` written with a 1-hour lifetime.
	pub cache_write_1h: u64,
	pub output: u64,
	/// omp's side requests (`model_usage`): `cache-warm`, compaction and the like; `None` for the agent's own turns.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub purpose: Option<String>,
}

/// One model request of a session (`kind: "request"` in a [`LedgerRow`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRow {
	pub session: String,
	/// `main`, or the subagent file's name.
	pub agent: String,
	pub at: Option<i64>,
	pub provider: Option<String>,
	pub model: String,
	pub input: u64,
	pub cache_read: u64,
	pub cache_write: u64,
	/// The part of `cache_write` written with a 1-hour lifetime.
	pub cache_write_1h: u64,
	pub output: u64,
	/// omp's side requests (`model_usage`): `cache-warm`, compaction and the like; `None` for the agent's own turns.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub purpose: Option<String>,
	/// Dollars from the pricing; `None` when it can't price the request.
	pub cost: Option<f64>,
}

/// One tool call of a session (`kind: "tool"` in a [`LedgerRow`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRow {
	pub session: String,
	pub agent: String,
	pub at: Option<i64>,
	pub tool: String,
	/// The first 16 hex of SHA-256 over the arguments as sorted-key JSON (omp's `i`/`intent` label left out).
	pub arg_hash: String,
	/// The file the call names, the first one when it names several; `None` when none.
	pub path: Option<String>,
	/// UTF-8 bytes of the result's text; `None` when the call has no result.
	pub result_bytes: Option<usize>,
	pub error: bool,
}

/// A row of the ledger: a request or a tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LedgerRow {
	Request(RequestRow),
	Tool(ToolRow),
}

impl LedgerRow {
	pub fn at(&self) -> Option<i64> {
		match self {
			LedgerRow::Request(row) => row.at,
			LedgerRow::Tool(row) => row.at,
		}
	}
}

/// A session as read: its rows and the times of its compactions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
	pub harness: LedgerHarness,
	pub session: String,
	pub agent: String,
	/// The session its header names as the one it moved from, forked off or ran under (omp's `parentSession`: an id
	/// for a move, a `/tan` clone or a seance fork, a file path for a subagent); its file may hold copies of that one's
	/// entries. `None` when the header names none.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub parent_session: Option<String>,
	/// Epoch ms of the session's header, when it has one; orders a thread's files parent-first.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub started_at: Option<i64>,
	pub requests: Vec<RequestRow>,
	pub tools: Vec<ToolRow>,
	/// Every file each call touched, by index into `tools`: reads, edits and writes.
	pub touched: Vec<Vec<String>>,
	/// Times of its compactions, Jocasta's own `/handoff` (`method: "handoff"`) left out (0582).
	pub compactions: Vec<Option<i64>>,
}

/// The waste counts of 0538.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waste {
	/// Reads of a file read before with no edit or write of it in between (a shell command's changes aren't seen).
	pub rereads: usize,
	/// A non-read call with the same tool and argument hash as an earlier one, with no edit or write in between.
	pub repeated_calls: usize,
	/// Results of [`OVERSIZED_RESULT_BYTES`] or more.
	pub oversized_results: usize,
	/// Failures of a tool after its first failure in a row (a success of that tool ends the row).
	pub repeated_failures: usize,
	/// Requests writing [`CACHE_MISS_WRITE_TOKENS`] or more to the cache after more than 5 minutes without a request
	/// on the same provider and model (each model has its own cache).
	pub cache_misses: usize,
	/// The idle minutes before each cache miss, rounded.
	pub cache_miss_idle_minutes: Vec<u64>,
	/// omp's compactions (`soft`, `shake`, `remote`, legacy), not Jocasta's own `/handoff` (`method: "handoff"`), which
	/// no context pressure caused (0582).
	pub compactions: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeakContext {
	/// The largest uncached input + cache read + cache write of the agent's own requests.
	pub tokens: u64,
	/// The model's window from the pricing; `None` when unknown.
	pub window: Option<u64>,
	pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
	pub requests: usize,
	pub tool_calls: usize,
	pub input: u64,
	pub cache_read: u64,
	pub cache_write: u64,
	pub output: u64,
	/// Dollars of the priced requests.
	pub cost: f64,
	/// Cache read over all input (uncached + read + write); `None` without input.
	pub cache_read_share: Option<f64>,
	/// Models the pricing can't price; their requests count in the tokens, not in `cost`.
	pub unpriced_models: Vec<String>,
}

/// A session's totals with its peak context and waste; its totals' fields read through it (`summary.cost`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
	pub harness: LedgerHarness,
	pub session: String,
	pub agent: String,
	#[serde(flatten)]
	pub totals: Totals,
	pub peak_context: PeakContext,
	pub waste: Waste,
}

impl Deref for SessionSummary {
	type Target = Totals;

	fn deref(&self) -> &Totals {
		&self.totals
	}
}

/// The sums of a [`TrialLedger`]; the [`Totals`]' fields read through it (`ledger.totals.cost`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialTotals {
	#[serde(flatten)]
	pub totals: Totals,
	pub peak_context: PeakContext,
	pub waste: Waste,
}

impl Deref for TrialTotals {
	type Target = Totals;

	fn deref(&self) -> &Totals {
		&self.totals
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialLedger {
	pub sessions: Vec<SessionSummary>,
	pub totals: TrialTotals,
	pub rows: Vec<LedgerRow>,
}

/// What a request costs and how large the model's window is; the seam between the ledger and a price source.
pub trait LedgerPricing {
	/// Dollars of one request; `usage` is the request's usage object as the session file has it. `None` when unknown.
	fn cost(&self, request: &RequestFields, usage: &Map<String, Value>) -> Option<f64>;
	/// The model's context window in tokens; `None` when unknown.
	fn window(&self, provider: Option<&str>, model: &str) -> Option<u64>;
}

pub const OVERSIZED_RESULT_BYTES: usize = 50_000;
const CACHE_MISS_IDLE_MS: i64 = 5 * 60_000;
pub const CACHE_MISS_WRITE_TOKENS: u64 = 10_000;

fn is_read_tool(tool: &str) -> bool {
	matches!(tool, "read" | "Read")
}

fn is_write_tool(tool: &str) -> bool {
	matches!(
		tool,
		"edit"
			| "write"
			| "ast_edit"
			| "Edit"
			| "Write"
			| "MultiEdit"
			| "NotebookEdit"
			| "apply_patch"
			| "patch"
			| "multiedit"
	)
}

fn pattern(source: &str) -> Regex {
	Regex::new(source).expect("a fixed pattern")
}

/// omp's read selector at a path's end: `:50`, `:50-80`, `:5-16,960-973`, `:-60`, `:raw`, `:2-4:raw`, `:conflicts`, `:img`.
static SELECTOR: LazyLock<Regex> =
	LazyLock::new(|| pattern(r":(?:-?[0-9][0-9,+-]*(?::raw)?|raw(?::-?[0-9][0-9,+-]*)?|conflicts|img)$"));
static EDIT_HEADER: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\[([^\]\n#]+)#[0-9A-Fa-f]{4}\]"));
static PATCH_HEADER: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\*\*\* (?:Update|Add|Delete) File: (.+)$"));

/// A finite number, else 0 (token counts as the session files have them).
pub fn token_count(value: &Value) -> u64 {
	match value.as_f64() {
		Some(count) if count.is_finite() && count > 0.0 => count as u64,
		_ => 0,
	}
}

/// A finite number, else 0 (prices as omp's model cache has them).
fn finite_or_zero(value: Option<&Value>) -> f64 {
	value.and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0)
}

fn canonical_number(n: &serde_json::Number) -> String {
	match n.as_f64() {
		Some(f) if n.is_f64() && f.is_finite() && f.fract() == 0.0 && f.abs() < 1e21 => format!("{:.0}", f + 0.0),
		_ => n.to_string(),
	}
}

fn canonical_object(map: &Map<String, Value>, skip: &[&str]) -> String {
	let mut keys: Vec<&String> = map.keys().filter(|key| !skip.contains(&key.as_str())).collect();
	keys.sort();
	let members: Vec<String> = keys
		.into_iter()
		.map(|key| format!("{}:{}", Value::String(key.clone()), canonical(&map[key.as_str()])))
		.collect();
	format!("{{{}}}", members.join(","))
}

/// Sorted-key JSON, so the same arguments in another order hash alike.
fn canonical(value: &Value) -> String {
	match value {
		Value::Array(items) => format!("[{}]", items.iter().map(canonical).collect::<Vec<_>>().join(",")),
		Value::Object(map) => canonical_object(map, &[]),
		Value::Number(n) => canonical_number(n),
		other => other.to_string(),
	}
}

/// The first 16 hex of SHA-256 over `args` as sorted-key JSON; omp's `i`/`intent` label is left out of an object.
pub fn arg_hash(args: &Value, harness: LedgerHarness) -> String {
	let text = match args {
		Value::Object(map) if harness == LedgerHarness::Omp => canonical_object(map, &["i", "intent"]),
		other => canonical(other),
	};
	let digest = Sha256::digest(text.as_bytes());
	digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

/// A path as compared: forward slashes, no leading `./`, omp's read selector (`:50-80`, `:raw`) cut, lowercased.
pub fn normalize_path(path: &str) -> String {
	let slashed = path.trim().replace('\\', "/");
	let mut cut = slashed.strip_prefix("./").unwrap_or(&slashed);
	if let Some(selector) = SELECTOR.find(cut)
		&& cut[..selector.start()].encode_utf16().count() > 1
	{
		cut = &cut[..selector.start()];
	}
	cut.to_lowercase()
}

/// The files a call names: `path`/`file_path`/`filePath`/`notebook_path`, omp edit's `[path#TAG]` headers, patch headers.
pub fn paths_of(args: &Value) -> Vec<String> {
	let Some(record) = args.as_object() else { return Vec::new() };
	let mut found: Vec<String> = Vec::new();
	for key in ["path", "file_path", "filePath", "notebook_path"] {
		if let Some(value) = record.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty()) {
			found.push(value.to_owned());
		}
	}
	for key in ["input", "patchText", "patch"] {
		let Some(text) = record.get(key).and_then(Value::as_str) else { continue };
		found.extend(EDIT_HEADER.captures_iter(text).map(|m| m[1].to_owned()));
		found.extend(PATCH_HEADER.captures_iter(text).map(|m| m[1].trim().to_owned()));
	}
	let mut seen = HashSet::new();
	found.iter().map(|path| normalize_path(path)).filter(|path| seen.insert(path.clone())).collect()
}

/// UTF-8 bytes of `text`.
pub fn utf8_bytes(text: &str) -> usize {
	text.len()
}

/// The text of a result's content: a string, or text blocks.
pub fn result_text(content: &Value) -> String {
	match content {
		Value::String(text) => text.clone(),
		Value::Array(blocks) => blocks
			.iter()
			.map(|block| {
				let Some(block) = block.as_object() else { return String::new() };
				if let Some(text) = block.get("text").and_then(Value::as_str) {
					return text.to_owned();
				}
				match (block.get("type").and_then(Value::as_str), block.get("content")) {
					(Some("tool_result"), Some(content)) => result_text(content),
					_ => String::new(),
				}
			})
			.collect(),
		_ => String::new(),
	}
}

/// Epoch ms of a number or an ISO string; `None` otherwise.
pub fn time_of(value: &Value) -> Option<i64> {
	match value {
		Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(|f| f as i64),
		Value::String(text) => date_parse(text),
		_ => None,
	}
}

pub fn new_record(harness: LedgerHarness, session: &str, agent: &str) -> SessionRecord {
	SessionRecord {
		harness,
		session: session.to_owned(),
		agent: agent.to_owned(),
		parent_session: None,
		started_at: None,
		requests: Vec::new(),
		tools: Vec::new(),
		touched: Vec::new(),
		compactions: Vec::new(),
	}
}

/// One tool call as [`push_tool`] takes it.
pub struct ToolCall<'a> {
	pub at: Option<i64>,
	pub tool: &'a str,
	pub args: &'a Value,
	pub result_bytes: Option<usize>,
	pub error: bool,
}

/// Adds one tool call to `record`, with the files it touched.
pub fn push_tool(record: &mut SessionRecord, call: &ToolCall) {
	let touched = paths_of(call.args);
	record.tools.push(ToolRow {
		session: record.session.clone(),
		agent: record.agent.clone(),
		at: call.at,
		tool: call.tool.to_owned(),
		arg_hash: arg_hash(call.args, record.harness),
		path: touched.first().cloned(),
		result_bytes: call.result_bytes,
		error: call.error,
	});
	record.touched.push(touched);
}

/// One request of `record`, priced.
pub fn request_row(
	record: &SessionRecord,
	pricing: &dyn LedgerPricing,
	fields: RequestFields,
	usage: &Map<String, Value>,
) -> RequestRow {
	let cost = pricing.cost(&fields, usage);
	RequestRow {
		session: record.session.clone(),
		agent: record.agent.clone(),
		at: fields.at,
		provider: fields.provider,
		model: fields.model,
		input: fields.input,
		cache_read: fields.cache_read,
		cache_write: fields.cache_write,
		cache_write_1h: fields.cache_write_1h,
		output: fields.output,
		purpose: fields.purpose,
		cost,
	}
}

/// Each tool result's size and failure by `toolCallId`, so a call can be joined with its result.
fn tool_results_of(entries: &[Map<String, Value>]) -> HashMap<String, (usize, bool)> {
	let mut results = HashMap::new();
	for entry in entries {
		let Some(message) = entry.get("message").and_then(as_object) else { continue };
		if entry.get("type").and_then(Value::as_str) != Some("message")
			|| message.get("role").and_then(Value::as_str) != Some("toolResult")
		{
			continue;
		}
		if let Some(id) = message.get("toolCallId").and_then(Value::as_str) {
			let text = message.get("content").map(result_text).unwrap_or_default();
			results.insert(id.to_owned(), (utf8_bytes(&text), message.get("isError") == Some(&Value::Bool(true))));
		}
	}
	results
}

/// A string field, or `unknown` when absent or null (the TypeScript's `String(value ?? "unknown")`).
pub fn string_or_unknown(value: Option<&Value>) -> String {
	match value {
		None | Some(Value::Null) => "unknown".to_owned(),
		Some(Value::String(text)) => text.clone(),
		Some(other) => other.to_string(),
	}
}

/// Each `toolCall` block of an assistant message's content as a call, joined with its result by id.
fn push_tool_calls(
	record: &mut SessionRecord,
	content: Option<&Value>,
	at: Option<i64>,
	results: &HashMap<String, (usize, bool)>,
) {
	let Some(blocks) = content.and_then(Value::as_array) else { return };
	for block in blocks {
		let Some(block) = block.as_object() else { continue };
		if block.get("type").and_then(Value::as_str) != Some("toolCall") {
			continue;
		}
		let result = block.get("id").and_then(Value::as_str).and_then(|id| results.get(id));
		let tool = string_or_unknown(block.get("name"));
		let call = ToolCall {
			at,
			tool: &tool,
			args: block.get("arguments").unwrap_or(&Value::Null),
			result_bytes: result.map(|(bytes, _)| *bytes),
			error: result.is_some_and(|(_, error)| *error),
		};
		push_tool(record, &call);
	}
}

/// A request's fields from `source` (an entry or a message holding `provider` and `model`) and its usage.
fn fields_of(
	at: Option<i64>,
	source: &Map<String, Value>,
	usage: &Map<String, Value>,
	purpose: Option<String>,
) -> RequestFields {
	let count = |key: &str| usage.get(key).map_or(0, token_count);
	RequestFields {
		at,
		provider: source.get("provider").and_then(Value::as_str).map(str::to_owned),
		model: string_or_unknown(source.get("model")),
		input: count("input"),
		cache_read: count("cacheRead"),
		cache_write: count("cacheWrite"),
		cache_write_1h: usage
			.get("cttl")
			.and_then(as_object)
			.and_then(|cttl| cttl.get("ephemeral1h"))
			.map_or(0, token_count),
		output: count("output"),
		purpose,
	}
}

/// One omp session file: assistant messages and `model_usage` entries are requests; `toolCall` blocks are calls;
/// compactions other than `method: "handoff"` count (0582). The header's `parentSession` and time are kept for
/// [`without_copies`]. `agent` is `main` unless given.
pub fn read_omp_session(text: &str, pricing: &dyn LedgerPricing, agent: Option<&str>) -> SessionRecord {
	let agent = agent.unwrap_or("main");
	let entries: Vec<Map<String, Value>> = json_lines(text);
	let header = entries.iter().find(|entry| entry.get("type").and_then(Value::as_str) == Some("session"));
	let text_of = |key: &str| header.and_then(|header| header.get(key)).and_then(Value::as_str);
	let mut record = new_record(LedgerHarness::Omp, text_of("id").unwrap_or(agent), agent);
	record.parent_session = text_of("parentSession").map(str::to_owned);
	record.started_at = header.and_then(|header| header.get("timestamp")).and_then(time_of);
	let results = tool_results_of(&entries);
	let no_usage = Map::new();
	for entry in &entries {
		let at = entry.get("timestamp").and_then(time_of);
		match entry.get("type").and_then(Value::as_str) {
			Some("compaction") => {
				if entry.get("method").and_then(Value::as_str) != Some("handoff") {
					record.compactions.push(at);
				}
			}
			Some("model_usage") => {
				let usage = entry.get("usage").and_then(as_object).unwrap_or(&no_usage);
				let purpose = entry.get("purpose").and_then(Value::as_str).unwrap_or("side");
				let fields = fields_of(at, entry, usage, Some(purpose.to_owned()));
				let row = request_row(&record, pricing, fields, usage);
				record.requests.push(row);
			}
			Some("message") => {
				let Some(message) = entry.get("message").and_then(as_object) else { continue };
				if message.get("role").and_then(Value::as_str) != Some("assistant") {
					continue;
				}
				let when = message.get("timestamp").and_then(time_of).or(at);
				if let Some(usage) = message.get("usage").and_then(as_object) {
					let row = request_row(&record, pricing, fields_of(when, message, usage, None), usage);
					record.requests.push(row);
				}
				push_tool_calls(&mut record, message.get("content"), when, &results);
			}
			_ => {}
		}
	}
	record
}

/// The waste counts of one session.
pub fn waste_of(record: &SessionRecord) -> Waste {
	let mut last_read: HashMap<&str, usize> = HashMap::new();
	let mut last_write: HashMap<&str, usize> = HashMap::new();
	let mut last_any_write: Option<usize> = None;
	let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
	let mut failing: HashMap<&str, usize> = HashMap::new();
	let mut waste = Waste::default();
	for (i, call) in record.tools.iter().enumerate() {
		let touched = record.touched.get(i).map_or(&[][..], Vec::as_slice);
		if is_read_tool(&call.tool) {
			for path in touched {
				let before = last_read.get(path.as_str()).copied();
				if let Some(before) = before
					&& last_write.get(path.as_str()).is_none_or(|&write| write < before)
				{
					waste.rereads += 1;
				}
				last_read.insert(path, i);
			}
		} else {
			let key = (call.tool.as_str(), call.arg_hash.as_str());
			if let Some(&before) = seen.get(&key)
				&& last_any_write.is_none_or(|write| write < before)
			{
				waste.repeated_calls += 1;
			}
			seen.insert(key, i);
		}
		if is_write_tool(&call.tool) && !call.error {
			for path in touched {
				last_write.insert(path, i);
			}
			last_any_write = Some(i);
		}
		if call.result_bytes.unwrap_or(0) >= OVERSIZED_RESULT_BYTES {
			waste.oversized_results += 1;
		}
		if call.error {
			let streak = failing.get(call.tool.as_str()).copied().unwrap_or(0);
			if streak > 0 {
				waste.repeated_failures += 1;
			}
			failing.insert(&call.tool, streak + 1);
		} else {
			failing.insert(&call.tool, 0);
		}
	}
	waste.cache_miss_idle_minutes = cache_miss_idle_minutes(&record.requests);
	waste.cache_misses = waste.cache_miss_idle_minutes.len();
	waste.compactions = record.compactions.len();
	waste
}

/// The idle minutes before each request that wrote a large cache after more than 5 minutes without a request on the
/// same provider and model.
fn cache_miss_idle_minutes(requests: &[RequestRow]) -> Vec<u64> {
	let mut minutes = Vec::new();
	let mut previous: HashMap<String, i64> = HashMap::new();
	for row in requests {
		let model = format!("{}/{}", row.provider.as_deref().unwrap_or(""), row.model);
		let Some(at) = row.at else { continue };
		if let Some(&before) = previous.get(&model)
			&& at - before > CACHE_MISS_IDLE_MS
			&& row.cache_write >= CACHE_MISS_WRITE_TOKENS
		{
			minutes.push(((at - before) as f64 / 60_000.0).round() as u64);
		}
		previous.insert(model, at);
	}
	minutes
}

/// The agent's own requests (side requests left out).
fn own_requests(record: &SessionRecord) -> impl Iterator<Item = &RequestRow> {
	record.requests.iter().filter(|row| row.purpose.is_none())
}

/// The agent's own requests' largest context (side requests left out), against the model's window.
fn peak_context_of(record: &SessionRecord, pricing: &dyn LedgerPricing) -> PeakContext {
	let mut peak: Option<&RequestRow> = None;
	let mut tokens = 0;
	for row in own_requests(record) {
		let size = row.input + row.cache_read + row.cache_write;
		if size > tokens || peak.is_none() {
			tokens = size;
			peak = Some(row);
		}
	}
	let window = peak.and_then(|row| pricing.window(row.provider.as_deref(), &row.model));
	// A window of 0 stays 0, with no percent (`window ? … : null`).
	PeakContext {
		tokens,
		window,
		percent: window
			.filter(|&window| window > 0)
			.map(|window| (tokens as f64 / window as f64 * 1000.0).round() / 10.0),
	}
}

fn totals_of(requests: &[&RequestRow], tool_calls: usize) -> Totals {
	let sum = |pick: fn(&RequestRow) -> u64| requests.iter().map(|row| pick(row)).sum::<u64>();
	let input = sum(|row| row.input);
	let cache_read = sum(|row| row.cache_read);
	let cache_write = sum(|row| row.cache_write);
	let all = input + cache_read + cache_write;
	let unpriced: BTreeSet<&String> = requests.iter().filter(|row| row.cost.is_none()).map(|row| &row.model).collect();
	Totals {
		requests: requests.len(),
		tool_calls,
		input,
		cache_read,
		cache_write,
		output: sum(|row| row.output),
		cost: requests.iter().fold(0.0, |total, row| total + row.cost.unwrap_or(0.0)),
		cache_read_share: (all != 0).then(|| cache_read as f64 / all as f64),
		unpriced_models: unpriced.into_iter().cloned().collect(),
	}
}

pub fn summarize_session(record: &SessionRecord, pricing: &dyn LedgerPricing) -> SessionSummary {
	let requests: Vec<&RequestRow> = record.requests.iter().collect();
	SessionSummary {
		harness: record.harness,
		session: record.session.clone(),
		agent: record.agent.clone(),
		totals: totals_of(&requests, record.tools.len()),
		peak_context: peak_context_of(record, pricing),
		waste: waste_of(record),
	}
}

/// The waste counts of several sessions, summed.
fn sum_waste<'a>(wastes: impl IntoIterator<Item = &'a Waste>) -> Waste {
	let mut waste = Waste::default();
	for w in wastes {
		waste.rereads += w.rereads;
		waste.repeated_calls += w.repeated_calls;
		waste.oversized_results += w.oversized_results;
		waste.repeated_failures += w.repeated_failures;
		waste.cache_misses += w.cache_misses;
		waste.cache_miss_idle_minutes.extend_from_slice(&w.cache_miss_idle_minutes);
		waste.compactions += w.compactions;
	}
	waste
}

fn request_key(row: &RequestRow) -> String {
	let at = row.at.map_or_else(|| "NaN".to_owned(), |at| at.to_string());
	format!("{at}|{}|{}|{}|{}|{}", row.model, row.input, row.cache_read, row.cache_write, row.output)
}

fn tool_key(row: &ToolRow) -> String {
	let at = row.at.map_or_else(|| "NaN".to_owned(), |at| at.to_string());
	format!("{at}|{}|{}", row.tool, row.arg_hash)
}

/// For each of `records`, `None` when it counts whole, else the record with copies left out (0538: each request once).
/// A file whose header names a parent (`parentSession`: a moved session's sibling, a `/tan` clone, a seance fork, a
/// subagent, or a subagent transcript omp copied along with a moved session's artifacts) may repeat entries another
/// file holds; its requests, calls and compactions an earlier record already counted are dropped. Records are walked
/// parent-first: those naming no parent, then the others by header time (a fork's, move's or subagent's header is
/// written after its parent's; a copied transcript keeps its original's header, and whichever of the two comes first
/// counts). A copy keeps its request's time, model and usage and its call's time, tool and arguments, which is what is
/// compared; omp's "entries older than the header" isn't a rule, since a move publishes the entry that triggered it
/// only to the new file. A record left whole stays `None`, so running this twice changes nothing.
fn without_copies(records: &[&SessionRecord]) -> Vec<Option<SessionRecord>> {
	let mut order: Vec<usize> = (0..records.len()).collect();
	// No header time sorts last, among the copies; the input order decides between equals.
	order.sort_by_key(|&i| {
		let record = records[i];
		let copies = record.parent_session.is_some();
		(copies, if copies { record.started_at.unwrap_or(i64::MAX) } else { 0 }, i)
	});
	let mut requests: HashSet<String> = HashSet::new();
	let mut tools: HashSet<String> = HashSet::new();
	let mut compactions: HashSet<Option<i64>> = HashSet::new();
	let mut result: Vec<Option<SessionRecord>> = vec![None; records.len()];
	for i in order {
		let record = records[i];
		let copies = record.parent_session.is_some();
		let kept_requests: Vec<&RequestRow> =
			record.requests.iter().filter(|r| !copies || !requests.contains(&request_key(r))).collect();
		let kept_tools: Vec<usize> = (0..record.tools.len())
			.filter(|&index| !copies || !tools.contains(&tool_key(&record.tools[index])))
			.collect();
		let kept_compactions: Vec<Option<i64>> =
			record.compactions.iter().filter(|at| !copies || !compactions.contains(at)).copied().collect();
		requests.extend(kept_requests.iter().map(|r| request_key(r)));
		tools.extend(kept_tools.iter().map(|&index| tool_key(&record.tools[index])));
		compactions.extend(kept_compactions.iter().copied());
		if kept_requests.len() != record.requests.len()
			|| kept_tools.len() != record.tools.len()
			|| kept_compactions.len() != record.compactions.len()
		{
			result[i] = Some(SessionRecord {
				requests: kept_requests.into_iter().cloned().collect(),
				tools: kept_tools.iter().map(|&index| record.tools[index].clone()).collect(),
				touched: kept_tools
					.iter()
					.map(|&index| record.touched.get(index).cloned().unwrap_or_default())
					.collect(),
				compactions: kept_compactions,
				..record.clone()
			});
		}
	}
	result
}

/// Every session's summary with copies left out ([`without_copies`]), their sums (waste summed, the largest peak) and
/// all rows.
pub fn trial_ledger(records: &[SessionRecord], pricing: &dyn LedgerPricing) -> TrialLedger {
	let whole: Vec<&SessionRecord> = records.iter().collect();
	let replaced = without_copies(&whole);
	let counted: Vec<&SessionRecord> =
		whole.iter().zip(&replaced).map(|(record, replacement)| replacement.as_ref().unwrap_or(*record)).collect();
	let sessions: Vec<SessionSummary> = counted.iter().map(|record| summarize_session(record, pricing)).collect();
	let requests: Vec<&RequestRow> = counted.iter().flat_map(|record| record.requests.iter()).collect();
	let mut peak = PeakContext { tokens: 0, window: None, percent: None };
	for session in &sessions {
		if session.peak_context.tokens > peak.tokens {
			peak = session.peak_context.clone();
		}
	}
	let mut rows: Vec<LedgerRow> = counted
		.iter()
		.flat_map(|record| {
			let requests = record.requests.iter().cloned().map(LedgerRow::Request);
			requests.chain(record.tools.iter().cloned().map(LedgerRow::Tool))
		})
		.collect();
	rows.sort_by_key(|row| row.at().unwrap_or(i64::MAX));
	TrialLedger {
		totals: TrialTotals {
			totals: totals_of(&requests, sessions.iter().map(|s| s.tool_calls).sum()),
			peak_context: peak,
			waste: sum_waste(sessions.iter().map(|s| &s.waste)),
		},
		sessions,
		rows,
	}
}

// --- The product's ledger: omp's own prices and windows, one thread's session files. ---

/// omp's prices for a model, in dollars per million tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
	pub input: f64,
	pub output: f64,
	pub cache_read: f64,
	pub cache_write: f64,
}

struct CachedModel {
	id: String,
	context_window: Option<u64>,
	cost: Option<ModelCost>,
}

struct CachedModels {
	provider: String,
	models: Vec<CachedModel>,
}

struct ModelCacheEntry {
	mtime_ms: f64,
	size: u64,
	rows: Arc<Vec<CachedModels>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `models.db`'s `model_cache` rows by the db's path, kept while the db's mtime and size stay the same (0546).
static MODEL_CACHES: LazyLock<Mutex<HashMap<PathBuf, ModelCacheEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A model's `cost` as omp's model cache has it; `None` without finite input and output prices.
fn model_cost(value: Option<&Value>) -> Option<ModelCost> {
	let cost = value.and_then(as_object)?;
	let price = |key: &str| cost.get(key).and_then(Value::as_f64).filter(|n| n.is_finite());
	Some(ModelCost {
		input: price("input")?,
		output: price("output")?,
		cache_read: finite_or_zero(cost.get("cacheRead")),
		cache_write: finite_or_zero(cost.get("cacheWrite")),
	})
}

/// The models of one `model_cache` row's JSON; those with neither a window nor a price are left out.
fn cached_models(json: &str) -> Vec<CachedModel> {
	let Ok(Value::Array(models)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
	models
		.iter()
		.filter_map(|model| {
			let model = as_object(model)?;
			let id = model.get("id").and_then(Value::as_str)?.to_owned();
			let context_window = model
				.get("contextWindow")
				.and_then(Value::as_f64)
				.filter(|&window| window > 0.0)
				.map(|window| window as u64);
			let cost = model_cost(model.get("cost"));
			(context_window.is_some() || cost.is_some()).then_some(CachedModel { id, context_window, cost })
		})
		.collect()
}

fn query_model_cache(path: &Path) -> rusqlite::Result<Vec<CachedModels>> {
	let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
	let mut statement = db.prepare("SELECT provider_id, models FROM model_cache")?;
	let mut rows = statement.query([])?;
	let mut found = Vec::new();
	while let Some(row) = rows.next()? {
		let (Ok(provider), Ok(models)) = (row.get_ref(0)?.as_str(), row.get_ref(1)?.as_str()) else { continue };
		found.push(CachedModels { provider: provider.to_owned(), models: cached_models(models) });
	}
	Ok(found)
}

/// omp's model cache rows, read again when the db's mtime or size changes (omp refreshes it; a fresh install has none
/// yet). A db that can't be opened or read (missing, locked, not omp's) yields none and isn't kept, so the next call
/// tries again.
fn read_model_cache(path: &Path) -> Arc<Vec<CachedModels>> {
	let Some(stat) = FileStat::of(path) else {
		locked(&MODEL_CACHES).remove(path);
		return Arc::default();
	};
	if let Some(cached) = locked(&MODEL_CACHES).get(path)
		&& cached.mtime_ms == stat.mtime_ms
		&& cached.size == stat.size
	{
		return Arc::clone(&cached.rows);
	}
	// Missing, locked or not omp's: none this time.
	let Ok(rows) = query_model_cache(path) else { return Arc::default() };
	let rows = Arc::new(rows);
	locked(&MODEL_CACHES).insert(
		path.to_path_buf(),
		ModelCacheEntry { mtime_ms: stat.mtime_ms, size: stat.size, rows: Arc::clone(&rows) },
	);
	rows
}

/// What `pick` takes from the first model `model` with that field set in omp's model cache, under `provider`
/// (`provider_id` equal to it or starting with `<provider>:`), or under any provider when it is `None`.
fn cached_model<T>(provider: Option<&str>, model: &str, pick: impl Fn(&CachedModel) -> Option<T>) -> Option<T> {
	let rows = read_model_cache(&omp_agent_dir(None).join("models.db"));
	rows.iter()
		.filter(|row| {
			provider.is_none_or(|p| {
				row.provider == p || row.provider.strip_prefix(p).is_some_and(|rest| rest.starts_with(':'))
			})
		})
		.find_map(|row| row.models.iter().filter(|m| m.id == model).find_map(&pick))
}

/// omp's price for `model` in dollars per million tokens, from its model cache (`<omp agent dir>/models.db`): under
/// `provider` as [`OMP_PRICING`]'s window matches it, or the first provider listing the model when `provider` is
/// `None`. `None` when the cache has no price for it.
pub fn omp_model_cost(provider: Option<&str>, model: &str) -> Option<ModelCost> {
	cached_model(provider, model, |m| m.cost.clone())
}

/// omp's own pricing: a request costs its `usage.cost.total` (`None` when it has none); the window is the model's
/// `contextWindow` in omp's model cache, `<omp agent dir>/models.db` (`model_cache`, read-only, read again when it
/// changes), under the request's provider (`provider_id` equal to it or starting with `<provider>:`).
pub struct OmpPricing;

/// The product's pricing (the TypeScript's `ompPricing`).
pub const OMP_PRICING: OmpPricing = OmpPricing;

impl LedgerPricing for OmpPricing {
	fn cost(&self, _request: &RequestFields, usage: &Map<String, Value>) -> Option<f64> {
		usage.get("cost").and_then(as_object)?.get("total")?.as_f64().filter(|total| total.is_finite())
	}

	fn window(&self, provider: Option<&str>, model: &str) -> Option<u64> {
		provider.and_then(|provider| cached_model(Some(provider), model, |m| m.context_window))
	}
}

/// One thread's ledger: every omp session of the thread and every subagent and advisor transcript under each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLedger {
	/// Dollars omp reported, all sessions.
	pub spend: f64,
	pub requests: usize,
	/// Cache read over all input, all sessions; `None` without input.
	pub cache_read_share: Option<f64>,
	/// The largest of the main sessions' own requests' peaks; `None` when they made none.
	pub peak_context: Option<PeakContext>,
	/// Summed over all sessions.
	pub waste: Waste,
}

struct CachedSession {
	size: u64,
	mtime_ms: f64,
	record: SessionRecord,
	summary: SessionSummary,
	/// Whether the agent made requests of its own (the peak means something).
	own_requests: bool,
}

/// Each session file's parse, kept while its size and mtime stay the same, until [`forget_stale_sessions`].
static SESSION_CACHE: LazyLock<Mutex<HashMap<PathBuf, Arc<CachedSession>>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached_session(path: &Path, agent: &str) -> Option<Arc<CachedSession>> {
	let Some(stat) = FileStat::of(path) else {
		locked(&SESSION_CACHE).remove(path);
		return None;
	};
	if let Some(cached) = locked(&SESSION_CACHE).get(path)
		&& cached.size == stat.size
		&& cached.mtime_ms == stat.mtime_ms
	{
		return Some(Arc::clone(cached));
	}
	let text = String::from_utf8_lossy(&fs::read(path).ok()?).into_owned();
	let record = read_omp_session(&text, &OMP_PRICING, Some(agent));
	let summary = summarize_session(&record, &OMP_PRICING);
	let has_own_requests = own_requests(&record).next().is_some();
	let entry = Arc::new(CachedSession {
		size: stat.size,
		mtime_ms: stat.mtime_ms,
		summary,
		own_requests: has_own_requests,
		record,
	});
	locked(&SESSION_CACHE).insert(path.to_path_buf(), Arc::clone(&entry));
	Some(entry)
}

/// Drops the ledger's kept parses of files last modified before `since_ms` (epoch ms) unless `keep(file)` holds, so the
/// cache doesn't grow for the service's lifetime. `file` is a session file as passed to [`thread_ledger`], or a
/// transcript under its artifacts dir (the session file's path without `.jsonl`).
pub fn forget_stale_sessions(since_ms: i64, keep: Option<&dyn Fn(&Path) -> bool>) {
	locked(&SESSION_CACHE)
		.retain(|file, entry| entry.mtime_ms >= since_ms as f64 || keep.is_some_and(|keep| keep(file)));
}

/// `read` over a thread's transcripts: each session file in order (agent `main`), and when it could be read, the
/// transcripts under the folder named like it without `.jsonl` (subagents, the advisor, forks; agent: the path
/// below that folder without `.jsonl`).
fn thread_transcripts<T>(session_files: &[PathBuf], mut read: impl FnMut(&Path, &str, bool) -> Option<T>) -> Vec<T> {
	let mut found = Vec::new();
	for session_file in session_files {
		let Some(main) = read(session_file, "main", true) else { continue };
		found.push(main);
		let dir = artifacts_dir(session_file);
		for path in transcripts_of(session_file) {
			let below = path.strip_prefix(&dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
			let agent = below.strip_suffix(".jsonl").unwrap_or(&below);
			if let Some(nested) = read(&path, agent, false) {
				found.push(nested);
			}
		}
	}
	found
}

/// The records of the files [`thread_ledger`] reads, read with `pricing` and with copies left out
/// ([`without_copies`]); files that can't be read are skipped. Nothing is cached.
pub fn thread_session_records(session_files: &[PathBuf], pricing: &dyn LedgerPricing) -> Vec<SessionRecord> {
	let records = thread_transcripts(session_files, |path, agent, _| {
		let text = String::from_utf8_lossy(&fs::read(path).ok()?).into_owned();
		Some(read_omp_session(&text, pricing, Some(agent)))
	});
	let replacements = without_copies(&records.iter().collect::<Vec<_>>());
	records.into_iter().zip(replacements).map(|(record, replacement)| replacement.unwrap_or(record)).collect()
}

/// A thread's ledger (0538, 0546) from its omp session files, oldest first, and the transcripts under the folder named
/// like each without `.jsonl` (subagents, the advisor, forks): spend, cache-read share and waste over all of them, a
/// copy counted once ([`without_copies`]), and peak context the largest of the session files' own. Unchanged files
/// aren't parsed again. `None` when none of the session files exists.
pub fn thread_ledger(session_files: &[PathBuf]) -> Option<ThreadLedger> {
	let mut peak_context: Option<PeakContext> = None;
	let sessions = thread_transcripts(session_files, |path, agent, main| {
		let session = cached_session(path, agent);
		if main
			&& let Some(session) = &session
			&& session.own_requests
			&& peak_context.as_ref().is_none_or(|peak| session.summary.peak_context.tokens > peak.tokens)
		{
			peak_context = Some(session.summary.peak_context.clone());
		}
		session
	});
	if sessions.is_empty() {
		return None;
	}
	let replacements = without_copies(&sessions.iter().map(|s| &s.record).collect::<Vec<_>>());
	let summaries: Vec<SessionSummary> = sessions
		.iter()
		.zip(&replacements)
		.map(|(session, replacement)| match replacement {
			None => session.summary.clone(),
			Some(record) => summarize_session(record, &OMP_PRICING),
		})
		.collect();
	let sum = |pick: fn(&SessionSummary) -> u64| summaries.iter().map(pick).sum::<u64>();
	let cache_read = sum(|s| s.cache_read);
	let all = sum(|s| s.input) + cache_read + sum(|s| s.cache_write);
	Some(ThreadLedger {
		spend: summaries.iter().fold(0.0, |total, s| total + s.cost),
		requests: summaries.iter().map(|s| s.requests).sum(),
		cache_read_share: (all != 0).then(|| cache_read as f64 / all as f64),
		peak_context,
		waste: sum_waste(summaries.iter().map(|s| &s.waste)),
	})
}

/// Tokens as the line shows them: `82k`, or `1M`/`1.5M` from a million up.
fn tokens_text(tokens: u64) -> String {
	let thousands = (tokens as f64 / 1000.0).round();
	if thousands < 1000.0 {
		return format!("{thousands}k");
	}
	format!("{}M", (tokens as f64 / 100_000.0).round() / 10.0)
}

fn counted(n: usize, one: &str, many: &str) -> String {
	format!("{n} {}", if n == 1 { one } else { many })
}

/// `x` with two decimals as JavaScript's `toFixed(2)` writes it: a tie (exactly representable, as 0.125) rounds up.
fn to_fixed_2(x: f64) -> String {
	let tie = (x * 8.0).fract() == 0.0 && (x * 8.0) % 2.0 != 0.0;
	if tie {
		return format!("{:.2}", (x.abs() * 100.0 + 0.5).floor() / 100.0 * x.signum());
	}
	format!("{x:.2}")
}

/// The thread's ledger line (0546):
/// `spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 repeated call · 1 cache miss · 2 compactions`.
/// Peak context reads `peak context 82k` when the window is unknown; a part with nothing to show (no input tokens
/// recorded, no cache traffic, a zero count) is left out.
pub fn ledger_line(ledger: &ThreadLedger) -> String {
	let mut parts = vec![format!("spend ${}", to_fixed_2(ledger.spend))];
	if let Some(peak) = ledger.peak_context.as_ref().filter(|peak| peak.tokens > 0) {
		parts.push(match peak.window.filter(|&window| window > 0) {
			Some(window) => format!(
				"peak context {}% ({} of {})",
				(peak.tokens as f64 / window as f64 * 100.0).round(),
				tokens_text(peak.tokens),
				tokens_text(window)
			),
			None => format!("peak context {}", tokens_text(peak.tokens)),
		});
	}
	if let Some(share) = ledger.cache_read_share {
		parts.push(format!("cache read {}%", (share * 100.0).round()));
	}
	let waste = &ledger.waste;
	if waste.rereads > 0 {
		parts.push(counted(waste.rereads, "re-read", "re-reads"));
	}
	if waste.repeated_calls > 0 {
		parts.push(counted(waste.repeated_calls, "repeated call", "repeated calls"));
	}
	if waste.cache_misses > 0 {
		parts.push(counted(waste.cache_misses, "cache miss", "cache misses"));
	}
	if waste.compactions > 0 {
		parts.push(counted(waste.compactions, "compaction", "compactions"));
	}
	parts.join(" · ")
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
