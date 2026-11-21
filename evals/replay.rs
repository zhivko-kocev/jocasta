//! Step B of the settings trials (fork plan section 4): which settings change what omp does, with no model spend.
//! omp runs against a loopback stand-in of Anthropic's Messages API (its `models.yml` points `anthropic`'s `baseUrl`
//! there) that plays one fixed session of about ten tool calls on a small fixture folder, the turn read from the count
//! of assistant messages so the stand-in keeps no state. Every request body is hashed after the parts that change from
//! run to run are taken out ([`normalize`]). Five base runs, then one run per arm value of every C and Q setting: an
//! arm whose requests hash as the base's, with its time and memory inside the base's spread (one rerun allowed), is
//! inert; any other is live and goes on to step C. Rows go to `<run>/replay.jsonl`, one per arm, and a rerun skips the
//! arms already there.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::jsonl::append_jsonl;
use jocasta::shared::process::on_path;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::settings::{Class, Scratch, arm_name, base_variant, overlay_of, read_settings};
use super::variants::{Variant, variant_args};
use super::watch::{Watched, run_watched, wait_for_start_floor, wait_while_paused};

/// What a subagent's task carries, so its requests are told from the main session's.
pub const SUBAGENT_MARK: &str = "REPLAY-SUBAGENT";
const PROMPT: &str = "Replay the scripted session.";
const BASE_RUNS: usize = 5;
/// How far past the base's lowest and highest a run's time or memory may fall, as a share of the base's median.
const SPREAD_SLACK: f64 = 0.25;
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// One content block of the stand-in's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
	Text(String),
	Tool { name: String, input: Value },
}

/// Every string in `value`, joined: a message's text, its tool results and its tool calls' input.
fn strings_of(value: &Value, out: &mut String) {
	match value {
		Value::String(text) => {
			out.push_str(text);
			out.push('\n');
		}
		Value::Array(items) => items.iter().for_each(|item| strings_of(item, out)),
		Value::Object(map) => map.values().for_each(|item| strings_of(item, out)),
		_ => {}
	}
}

fn text_of(message: Option<&Value>) -> String {
	let mut out = String::new();
	if let Some(message) = message {
		strings_of(&message["content"], &mut out);
	}
	out
}

static NOTES_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[notes\.md#([0-9A-Za-z]+)\]").expect("pattern"));

/// The tool call of the main session's turn `turn`, or its last words. `last` is the latest user message's text, where
/// the edit finds the tag the read gave.
pub fn main_turn(turn: usize, last: &str) -> Block {
	let tool = |name: &str, input: Value| {
		let mut input = input;
		input["i"] = json!("Replaying a step");
		Block::Tool { name: name.to_owned(), input }
	};
	match turn {
		0 => tool("read", json!({ "path": "src/big.rs" })),
		1 => tool("grep", json!({ "pattern": "fn step_", "path": "src" })),
		2 => tool("bash", json!({ "command": "echo replay" })),
		3 => tool("read", json!({ "path": "notes.md" })),
		4 => {
			let tag = NOTES_TAG.captures(last).and_then(|found| found.get(1)).map_or("0000", |tag| tag.as_str());
			tool("edit", json!({ "input": format!("[notes.md#{tag}]\nPUT 1.=1:\n+# Notes, replayed\n") }))
		}
		5 => tool(
			"task",
			json!({
				"context": "# Goal\nReplay.",
				"tasks": [{ "name": "Replay", "task": format!("{SUBAGENT_MARK}: answer done."), "solutionSpace": "one answer" }],
			}),
		),
		6 => tool("glob", json!({ "path": "**/*.md" })),
		7 => tool("write", json!({ "path": "out.txt", "content": "replayed\n" })),
		_ => Block::Text("Replay done.".to_owned()),
	}
}

/// A tool input that fills `schema`'s required fields with plain values.
fn filled(schema: &Value) -> Value {
	let mut input = serde_json::Map::new();
	for name in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
		let value = match schema["properties"][name]["type"].as_str() {
			Some("object") => json!({}),
			Some("array") => json!([]),
			Some("boolean") => json!(true),
			Some("number" | "integer") => json!(0),
			_ => json!("done"),
		};
		input.insert(name.to_owned(), value);
	}
	Value::Object(input)
}

/// A subagent ends at once: through the tool it reports with when it has one, else in words.
pub fn subagent_turn(turn: usize, tools: &[Value]) -> Block {
	let ends = tools.iter().find(|tool| {
		let name = tool["name"].as_str().unwrap_or("");
		["yield", "submit", "result", "complete", "finish"].iter().any(|word| name.contains(word))
	});
	match ends {
		Some(tool) if turn == 0 => {
			Block::Tool { name: tool["name"].as_str().unwrap_or("").to_owned(), input: filled(&tool["input_schema"]) }
		}
		_ => Block::Text("done".to_owned()),
	}
}

/// The stand-in's answer to a request: the scripted turn for the main session or a subagent, a short text for a
/// request without tools (titles, skill summaries).
pub fn reply(request: &Value) -> Vec<Block> {
	let tools = request["tools"].as_array().cloned().unwrap_or_default();
	if tools.is_empty() {
		return vec![Block::Text("ok".to_owned())];
	}
	let messages = request["messages"].as_array().cloned().unwrap_or_default();
	let turn = messages.iter().filter(|message| message["role"] == "assistant").count();
	if text_of(messages.first()).contains(SUBAGENT_MARK) {
		return vec![subagent_turn(turn, &tools)];
	}
	vec![main_turn(turn, &text_of(messages.last()))]
}

/// The answer as the Messages API streams it.
pub fn sse(blocks: &[Block], input_tokens: u64) -> String {
	let mut out = String::new();
	let mut event = |data: Value| {
		out.push_str(&format!("event: {}\ndata: {data}\n\n", data["type"].as_str().unwrap_or("")));
	};
	event(json!({ "type": "message_start", "message": {
		"id": "msg_replay", "type": "message", "role": "assistant", "model": "claude-opus-5-5", "content": [],
		"stop_reason": null, "stop_sequence": null,
		"usage": { "input_tokens": input_tokens, "output_tokens": 1, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0 },
	}}));
	let mut uses_tool = false;
	for (index, block) in blocks.iter().enumerate() {
		match block {
			Block::Text(text) => {
				event(
					json!({ "type": "content_block_start", "index": index, "content_block": { "type": "text", "text": "" } }),
				);
				event(
					json!({ "type": "content_block_delta", "index": index, "delta": { "type": "text_delta", "text": text } }),
				);
			}
			Block::Tool { name, input } => {
				uses_tool = true;
				let id = format!("toolu_replay_{index}");
				event(json!({ "type": "content_block_start", "index": index,
					"content_block": { "type": "tool_use", "id": id, "name": name, "input": {} } }));
				event(json!({ "type": "content_block_delta", "index": index,
					"delta": { "type": "input_json_delta", "partial_json": input.to_string() } }));
			}
		}
		event(json!({ "type": "content_block_stop", "index": index }));
	}
	let stop = if uses_tool { "tool_use" } else { "end_turn" };
	event(
		json!({ "type": "message_delta", "delta": { "stop_reason": stop, "stop_sequence": null }, "usage": { "output_tokens": 20 } }),
	);
	event(json!({ "type": "message_stop" }));
	out
}

/// The answer as one JSON message, for a request that doesn't stream.
fn message(blocks: &[Block], input_tokens: u64) -> Value {
	let content: Vec<Value> = blocks
		.iter()
		.enumerate()
		.map(|(index, block)| match block {
			Block::Text(text) => json!({ "type": "text", "text": text }),
			Block::Tool { name, input } => {
				json!({ "type": "tool_use", "id": format!("toolu_replay_{index}"), "name": name, "input": input })
			}
		})
		.collect();
	let stop = if blocks.iter().any(|block| matches!(block, Block::Tool { .. })) { "tool_use" } else { "end_turn" };
	json!({ "id": "msg_replay", "type": "message", "role": "assistant", "model": "claude-opus-5-5", "content": content,
		"stop_reason": stop, "stop_sequence": null, "usage": { "input_tokens": input_tokens, "output_tokens": 20 } })
}

static VOLATILE: LazyLock<[(Regex, &'static str); 5]> = LazyLock::new(|| {
	let pattern = |text: &str| Regex::new(text).expect("pattern");
	[
		(pattern(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"), "<uuid>"),
		(pattern(r"\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?"), "<date>"),
		(pattern(r"\b\d{1,2}:\d{2}(:\d{2})?( ?[AP]M)?\b"), "<time>"),
		(pattern(r"\b\d+(\.\d+)? ?(ms|s|sec|seconds|minutes?)\b"), "<duration>"),
		(pattern(r"\b1[78]\d{11}\b"), "<epoch-ms>"),
	]
});

/// A request body without what changes between two runs of the same session: `metadata` (the session's id), ids,
/// dates, clock times, durations and epoch times.
pub fn normalize(body: &Value) -> String {
	let mut body = body.clone();
	if let Some(map) = body.as_object_mut() {
		map.remove("metadata");
	}
	let mut text = body.to_string();
	for (pattern, mark) in VOLATILE.iter() {
		text = pattern.replace_all(&text, *mark).into_owned();
	}
	text
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What the stand-in saw during one run.
#[derive(Debug, Default, Clone)]
pub struct Seen {
	/// Each request's path and normalized body's hash, in arrival order.
	pub requests: Vec<(String, String)>,
	/// Every normalized body, for the base's diagnosis.
	pub bodies: Vec<String>,
	pub request_bytes: u64,
}

impl Seen {
	/// The run's signature: its requests' paths and hashes in sorted order, so side requests that race the session
	/// don't reorder it.
	pub fn signature(&self) -> String {
		let mut lines: Vec<String> = self.requests.iter().map(|(path, hash)| format!("{path} {hash}")).collect();
		lines.sort();
		hex(&Sha256::digest(lines.join("\n")))
	}
}

type Shared = Arc<Mutex<Seen>>;

async fn messages(State(seen): State<Shared>, uri: Uri, body: Bytes) -> Response {
	let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
	let normalized = normalize(&request);
	{
		let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		seen.requests.push((uri.path().to_owned(), hex(&Sha256::digest(normalized.as_bytes()))));
		seen.bodies.push(normalized);
		seen.request_bytes += body.len() as u64;
	}
	let input_tokens = body.len() as u64 / 4;
	if uri.path().ends_with("/count_tokens") {
		return axum::Json(json!({ "input_tokens": input_tokens })).into_response();
	}
	let blocks = reply(&request);
	if request["stream"] == true {
		([(header::CONTENT_TYPE, "text/event-stream")], sse(&blocks, input_tokens)).into_response()
	} else {
		axum::Json(message(&blocks, input_tokens)).into_response()
	}
}

async fn models() -> Response {
	axum::Json(json!({
		"data": [{ "id": "claude-opus-5-5", "type": "model", "display_name": "Claude Opus 5.5", "created_at": "2026-01-01T00:00:00Z" }],
		"has_more": false, "first_id": "claude-opus-5-5", "last_id": "claude-opus-5-5",
	}))
	.into_response()
}

async fn elsewhere(State(seen): State<Shared>, uri: Uri) -> Response {
	seen.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.requests
		.push((uri.path().to_owned(), "unexpected".to_owned()));
	StatusCode::NOT_FOUND.into_response()
}

/// The stand-in, on a loopback port, until it is dropped.
pub struct StandIn {
	pub port: u16,
	seen: Shared,
	server: tokio::task::JoinHandle<()>,
}

impl StandIn {
	/// Listens on a free loopback port and answers omp's provider requests with the scripted session.
	pub async fn start() -> Result<Self> {
		let seen: Shared = Arc::default();
		let app = Router::new()
			.route("/v1/messages", post(messages))
			.route("/v1/messages/count_tokens", post(messages))
			.route("/v1/models", get(models))
			.fallback(elsewhere)
			.with_state(seen.clone());
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
		let port = listener.local_addr()?.port();
		let server = tokio::spawn(async move {
			let _ = axum::serve(listener, app).await;
		});
		Ok(Self { port, seen, server })
	}

	/// What it saw since the last call.
	pub fn take(&self) -> Seen {
		std::mem::take(&mut *self.seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
	}
}

impl Drop for StandIn {
	fn drop(&mut self) {
		self.server.abort();
	}
}

/// The fixture folder the session works in, made anew for every run.
pub fn make_fixture(work: &Path) -> Result<()> {
	remove_tree(work)?;
	fs::create_dir_all(work.join("src"))?;
	let big: String =
		(0..1500).map(|i| format!("pub fn step_{i}(x: u64) -> u64 {{\n\tx.wrapping_mul({i}) + 1\n}}\n\n")).collect();
	fs::write(work.join("src").join("big.rs"), big)?;
	fs::write(work.join("src").join("lib.rs"), "pub mod big;\n")?;
	fs::write(work.join("notes.md"), "# Notes\n\nA fixture for the settings replay.\n")?;
	fs::write(work.join("README.md"), "# Fixture\n")?;
	Ok(())
}

/// One run's row in `replay.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRow {
	pub arm: String,
	pub key: Option<String>,
	pub class: Option<Class>,
	/// `inert`, `live` or `base`.
	pub verdict: String,
	/// Why an arm is live: `requests`, `exit`, `time`, `memory`.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub because: Vec<String>,
	pub code: Option<i32>,
	pub requests: usize,
	pub request_bytes: u64,
	pub wall_ms: u64,
	pub peak_tree_bytes: Option<u64>,
	pub signature: String,
}

/// The base's spread of one measure: lowest to highest, widened on both sides by [`SPREAD_SLACK`] of the median; `None`
/// without a value.
pub fn spread_of(values: &[f64]) -> Option<RangeInclusive<f64>> {
	if values.is_empty() {
		return None;
	}
	let mut sorted = values.to_vec();
	sorted.sort_by(f64::total_cmp);
	let median = sorted[sorted.len() / 2];
	Some((sorted[0] - SPREAD_SLACK * median)..=(sorted[sorted.len() - 1] + SPREAD_SLACK * median))
}

/// What makes an arm's run differ from the base: its requests, its exit, its time, its memory.
pub fn differences(run: &Watched, seen: &Seen, base: &Base) -> Vec<String> {
	let mut because = Vec::new();
	if seen.signature() != base.signature {
		because.push("requests".to_owned());
	}
	if run.code != Some(0) {
		because.push("exit".to_owned());
	}
	if !base.wall.contains(&(run.wall_ms as f64)) {
		because.push("time".to_owned());
	}
	if let (Some(memory), Some(peak)) = (&base.memory, run.peak_tree_bytes)
		&& !memory.contains(&(peak as f64))
	{
		because.push("memory".to_owned());
	}
	because
}

/// The base runs' agreement.
pub struct Base {
	pub signature: String,
	pub wall: RangeInclusive<f64>,
	pub memory: Option<RangeInclusive<f64>>,
}

/// The run's omp: its folders, the stand-in, the base arm.
struct Replay {
	/// The settings run's folder, where its `PAUSE` file goes.
	dir: PathBuf,
	scratch: Scratch,
	work: PathBuf,
	omp: String,
	variant: Variant,
	stand_in: StandIn,
}

impl Replay {
	/// One run of the session with `arm` over the base.
	async fn run(&self, arm: &[(String, Value)]) -> Result<(Watched, Seen)> {
		make_fixture(&self.work)?;
		let agent = self.scratch.agent();
		remove_tree(&agent)?;
		fs::create_dir_all(&agent)?;
		fs::write(
			agent.join("models.yml"),
			format!(
				"providers:\n  anthropic:\n    baseUrl: http://127.0.0.1:{}\n    apiKey: replay-key\n",
				self.stand_in.port
			),
		)?;
		let overlay = self.scratch.root.join("replay-run.yml");
		fs::write(&overlay, serde_json::to_string_pretty(&Value::Object(overlay_of(&self.variant, arm)))?)?;
		let mut argv = vec![self.omp.clone()];
		argv.extend(variant_args(&self.variant));
		argv.extend(["--config".to_owned(), overlay.display().to_string(), "--no-session".to_owned()]);
		argv.extend(["-p".to_owned(), PROMPT.to_owned()]);
		wait_while_paused(&self.dir).await;
		wait_for_start_floor().await;
		self.stand_in.take();
		let watched = run_watched(&argv, &self.work, &self.scratch.env(), RUN_LIMIT).await?;
		// A side request may still be in flight when omp exits.
		tokio::time::sleep(Duration::from_millis(200)).await;
		Ok((watched, self.stand_in.take()))
	}
}

/// A run's measures as a row of the base; an arm's row sets its class and verdict over it.
fn row(arm: &[(String, Value)], run: &Watched, seen: &Seen) -> ReplayRow {
	ReplayRow {
		arm: arm_name(arm),
		key: arm.first().map(|(key, _)| key.clone()),
		class: None,
		verdict: "base".to_owned(),
		because: Vec::new(),
		code: run.code,
		requests: seen.requests.len(),
		request_bytes: seen.request_bytes,
		wall_ms: run.wall_ms,
		peak_tree_bytes: run.peak_tree_bytes,
		signature: seen.signature(),
	}
}

/// The first request where two base runs part, both bodies written beside the rows for the diagnosis.
fn base_disagreement(dir: &Path, first: &Seen, other: &Seen) -> Result<Error> {
	let mut a = first.bodies.clone();
	let mut b = other.bodies.clone();
	a.sort();
	b.sort();
	let only_a: Vec<&String> = a.iter().filter(|body| !b.contains(body)).collect();
	let only_b: Vec<&String> = b.iter().filter(|body| !a.contains(body)).collect();
	fs::write(dir.join("base-a.txt"), only_a.iter().map(|body| format!("{body}\n\n")).collect::<String>())?;
	fs::write(dir.join("base-b.txt"), only_b.iter().map(|body| format!("{body}\n\n")).collect::<String>())?;
	Ok(Error::other(format!(
		"the base runs' requests differ ({} and {} requests apart); see base-a.txt and base-b.txt in {}",
		only_a.len(),
		only_b.len(),
		dir.display()
	)))
}

/// `jocasta-bench evals settings replay --dir <run>`.
pub async fn main(dir: &Path) -> Result<()> {
	let settings = read_settings(dir)?;
	let replay_dir = dir.join("replay");
	fs::create_dir_all(&replay_dir)?;
	let omp = on_path("omp", None).ok_or_else(|| Error::user("omp is not on PATH"))?.display().to_string();
	let replay = Replay {
		dir: dir.to_owned(),
		scratch: Scratch::new(replay_dir.join("scratch"))?,
		work: replay_dir.join("work"),
		omp,
		variant: base_variant()?,
		stand_in: StandIn::start().await?,
	};
	let rows_file = dir.join("replay.jsonl");
	let done: HashSet<String> = fs::read_to_string(&rows_file)
		.unwrap_or_default()
		.lines()
		.filter_map(|line| serde_json::from_str::<ReplayRow>(line).ok())
		.filter(|row| row.verdict != "base")
		.map(|row| row.arm)
		.collect();
	let mut bases: Vec<(Watched, Seen)> = Vec::with_capacity(BASE_RUNS);
	for at in 0..BASE_RUNS {
		let (run, seen) = replay.run(&[]).await?;
		if run.code != Some(0) {
			return Err(Error::other(format!("base run {at} exited {:?}: {}", run.code, run.stderr.trim())));
		}
		append_jsonl(&rows_file, &row(&[], &run, &seen))?;
		println!("base {at}: {} requests, {} ms", seen.requests.len(), run.wall_ms);
		bases.push((run, seen));
	}
	let signature = bases[0].1.signature();
	if let Some((_, other)) = bases.iter().find(|(_, seen)| seen.signature() != signature) {
		return Err(base_disagreement(&replay_dir, &bases[0].1, other)?);
	}
	let walls: Vec<f64> = bases.iter().map(|(run, _)| run.wall_ms as f64).collect();
	let peaks: Vec<f64> = bases.iter().filter_map(|(run, _)| run.peak_tree_bytes.map(|peak| peak as f64)).collect();
	let base = Base { signature, wall: spread_of(&walls).unwrap_or(0.0..=f64::MAX), memory: spread_of(&peaks) };
	let arms: Vec<(&str, Class, &Value)> = settings
		.iter()
		.filter(|setting| matches!(setting.class, Class::C | Class::Q))
		.flat_map(|setting| setting.arms.iter().map(move |value| (setting.key.as_str(), setting.class, value)))
		.collect();
	let mut counts: HashMap<&str, usize> = HashMap::new();
	for (at, (key, class, value)) in arms.iter().enumerate() {
		let arm = [((*key).to_owned(), (*value).clone())];
		if done.contains(&arm_name(&arm)) {
			continue;
		}
		let (mut run, mut seen) = replay.run(&arm).await?;
		let mut because = differences(&run, &seen, &base);
		// Time or memory alone may be noise: one rerun decides.
		if !because.is_empty() && because.iter().all(|reason| reason == "time" || reason == "memory") {
			(run, seen) = replay.run(&arm).await?;
			because = differences(&run, &seen, &base);
		}
		let verdict = if run.memory_floor {
			"infra"
		} else if because.is_empty() {
			"inert"
		} else {
			"live"
		};
		*counts.entry(verdict).or_default() += 1;
		let judged = ReplayRow {
			class: Some(*class),
			verdict: verdict.to_owned(),
			because: because.clone(),
			..row(&arm, &run, &seen)
		};
		append_jsonl(&rows_file, &judged)?;
		println!("[{}/{}] {verdict} {} {}", at + 1, arms.len(), arm_name(&arm), because.join(","));
	}
	println!("replay done: {counts:?} (this pass; rows in {})", rows_file.display());
	Ok(())
}
