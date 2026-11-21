//! The harnesses a trial runs (0386, 0482 E4, 0537), one adapter each (EVL-14): how a turn's arguments are built, which
//! of a variant's omp keys it takes, where its records are and how they're saved, and how its transcript reads (usage,
//! assistant text, the error it ended on). The runner and the task benchmark ask the adapter instead of branching on
//! the harness's name.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use jocasta::frontier::protocol::js_number;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::jsonl::json_lines;
use jocasta::shared::process::{RunOptions, run_program_sync};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::evals::variants::Harness;

/// Where A7's Anthropic API key waits (0537): the only API key of the run, given to opencode's process as
/// `ANTHROPIC_API_KEY` and removed from every other process a trial starts (and an `ANTHROPIC_API_KEY` of the bench's
/// own environment from every trial).
pub const A7_API_KEY_ENV: &str = "JOCASTA_A7_ANTHROPIC_API_KEY";

/// A trial's token counts and cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
	#[serde(serialize_with = "js_number")]
	pub input: f64,
	#[serde(serialize_with = "js_number")]
	pub output: f64,
	#[serde(serialize_with = "js_number")]
	pub cache_read: f64,
	#[serde(serialize_with = "js_number")]
	pub cache_write: f64,
	#[serde(serialize_with = "js_number")]
	pub total_tokens: f64,
	#[serde(serialize_with = "js_number")]
	pub cost: f64,
}

impl Usage {
	fn plus(&self, other: &Usage) -> Usage {
		Usage {
			input: self.input + other.input,
			output: self.output + other.output,
			cache_read: self.cache_read + other.cache_read,
			cache_write: self.cache_write + other.cache_write,
			total_tokens: self.total_tokens + other.total_tokens,
			cost: self.cost + other.cost,
		}
	}
}

/// What a trial's transcript says about its model, usage, turns and tool calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessResponse {
	pub model: Option<String>,
	pub usage: Usage,
	pub total_usage: Usage,
	pub requests: u64,
	pub turns: u64,
	pub tool_calls: u64,
}

/// One turn as the runner builds its arguments (see [`HarnessAdapter::args`]).
#[derive(Debug, Clone, Default)]
pub struct Turn {
	pub prompt: String,
	pub follow_up: bool,
	pub overlay: String,
	pub profile: Option<String>,
	pub session_dir: Option<String>,
	pub session: Option<String>,
	pub extension: Option<String>,
}

/// Takes a secret out of an export's text.
pub type Scrub = Box<dyn Fn(&str) -> String>;

/// What [`save_sessions`] needs of a finished trial.
pub struct TrialRecords {
	pub transcript: String,
	pub session_dir: Option<PathBuf>,
	pub launcher: Vec<String>,
	pub env: HashMap<String, String>,
	pub cwd: PathBuf,
	pub scrub: Option<Scrub>,
}

/// The variant keys omp alone takes unless a harness lists them in [`HarnessAdapter::accepts`], in report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmpKey {
	PlanFirst,
	PlanReview,
	PlanReviewRole,
	Configs,
	Extensions,
	Tools,
	Settings,
	Layer,
	AppendSystemPrompt,
}

pub const OMP_KEYS: [OmpKey; 9] = [
	OmpKey::PlanFirst,
	OmpKey::PlanReview,
	OmpKey::PlanReviewRole,
	OmpKey::Configs,
	OmpKey::Extensions,
	OmpKey::Tools,
	OmpKey::Settings,
	OmpKey::Layer,
	OmpKey::AppendSystemPrompt,
];

impl OmpKey {
	/// The key as a variant file spells it.
	pub fn name(self) -> &'static str {
		match self {
			Self::PlanFirst => "plan_first",
			Self::PlanReview => "plan_review",
			Self::PlanReviewRole => "plan_review_role",
			Self::Configs => "configs",
			Self::Extensions => "extensions",
			Self::Tools => "tools",
			Self::Settings => "settings",
			Self::Layer => "layer",
			Self::AppendSystemPrompt => "append_system_prompt",
		}
	}
}

/// What [`HarnessAdapter::response`] reads a trial's usage from.
pub struct ResponseSource<'a> {
	pub transcript: &'a str,
	pub saved_sessions: Option<&'a Path>,
	pub session_dir: Option<&'a Path>,
	pub plan_review: bool,
}

pub struct HarnessAdapter {
	/// The keys of [`OMP_KEYS`] this harness takes too; a variant setting another one for it is refused.
	pub accepts: &'static [OmpKey],
	/// One turn's arguments after the launcher, `flags` being the variant's (`variant_args`).
	pub args: fn(&[String], &Turn) -> Result<Vec<String>>,
	/// Writes its session files to the trial's `--session-dir`; can answer omp's interactive plan review (0567).
	pub session_files: bool,
	/// The variable whose value is its Anthropic API key (A7, 0537), given to it as `ANTHROPIC_API_KEY`.
	pub api_key_env: Option<&'static str>,
	/// The session a follow-up continues, from the turns' output so far; `None` when it continues the last.
	pub session: fn(&str) -> Option<String>,
	/// The trial-root folder its records are saved to when the run keeps none: its usage is read from them.
	pub records_dir: Option<&'static str>,
	/// Copies or exports the trial's records into `dir` (emptied first by [`save_sessions`]).
	pub save_sessions: fn(&Path, &TrialRecords) -> Result<()>,
	/// The trial's usage: from its transcript, or from its records (`saved_sessions`, the session files of a plan review).
	pub response: fn(&ResponseSource) -> Result<HarnessResponse>,
	/// The assistant's text in one turn's transcript, joined by newlines.
	pub assistant_text: fn(&str) -> String,
	/// The error its own records say the trial ended on, or "" (EVL-2; see `endSignal` in tasks.ts).
	pub end_signal: fn(&str) -> String,
}

/// `value[key]` for an object value.
fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
	value.get(key)
}

/// A number the way `used.input ?? 0` adds it: anything that isn't a number counts as 0.
fn number(value: Option<&Value>) -> f64 {
	value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(value: Option<&Value>) -> Option<&str> {
	value.and_then(Value::as_str)
}

/// How many of `content`'s blocks have `kind` as their `type`.
fn blocks_of(content: Option<&Value>, kind: &str) -> u64 {
	let Some(Value::Array(blocks)) = content else { return 0 };
	blocks.iter().filter(|block| text(field(block, "type")) == Some(kind)).count() as u64
}

/// omp's usage shape: Jocasta's, with `cost` an object whose `total` is the dollars.
fn add_usage(target: &mut Usage, used: Option<&Value>) {
	let Some(used) = used.filter(|used| !used.is_null()) else { return };
	target.input += number(field(used, "input"));
	target.output += number(field(used, "output"));
	target.cache_read += number(field(used, "cacheRead"));
	target.cache_write += number(field(used, "cacheWrite"));
	target.total_tokens += number(field(used, "totalTokens"));
	target.cost += number(field(used, "cost").and_then(|cost| cost.get("total")));
}

/// The text blocks of a message's `content`.
fn text_blocks(content: Option<&Value>) -> Vec<String> {
	let Some(Value::Array(blocks)) = content else { return Vec::new() };
	blocks
		.iter()
		.filter(|block| text(field(block, "type")) == Some("text"))
		.filter_map(|block| text(field(block, "text")).map(str::to_owned))
		.collect()
}

/// What a transcript's assistant messages add up to, shared by the JSON stream's and the session files' readers.
#[derive(Default)]
struct Tally {
	usage: Usage,
	subagent_usage: Usage,
	subagent_requests: u64,
	model: Option<String>,
	turns: u64,
	tool_calls: u64,
}

impl Tally {
	/// One assistant message of the trial's own session.
	fn own(&mut self, message: &Value) {
		self.turns += 1;
		self.tool_calls += blocks_of(field(message, "content"), "toolCall");
		if let Some(model) = text(field(message, "model")) {
			self.model = Some(model.to_owned());
		}
		add_usage(&mut self.usage, field(message, "usage"));
	}

	fn response(self) -> HarnessResponse {
		HarnessResponse {
			model: self.model,
			total_usage: self.usage.plus(&self.subagent_usage),
			usage: self.usage,
			requests: self.turns + self.subagent_requests,
			turns: self.turns,
			tool_calls: self.tool_calls,
		}
	}
}

/// From omp's `--mode json` events: the model as the responses name it, the main session's usage summed over its
/// assistant `message_end` events, those turns and their `toolCall` blocks, and `total_usage`/`requests` with the
/// foreground subagents of each `task` call added from its `tool_execution_end` details (`usage`, `results[].requests`).
pub fn read_response(transcript: &str) -> HarnessResponse {
	let mut tally = Tally::default();
	// A line torn by a kill is skipped (DUP-1): a timed-out trial is graded as such, not thrown as infra.
	for event in json_lines::<Value>(transcript) {
		let kind = text(field(&event, "type"));
		if kind == Some("tool_execution_end") && text(field(&event, "toolName")) == Some("task") {
			let details = field(&event, "result").and_then(|result| result.get("details"));
			add_usage(&mut tally.subagent_usage, details.and_then(|d| d.get("usage")));
			if let Some(Value::Array(results)) = details.and_then(|d| d.get("results")) {
				tally.subagent_requests += results.iter().map(|r| number(field(r, "requests"))).sum::<f64>() as u64;
			}
			continue;
		}
		let message = field(&event, "message");
		if kind != Some("message_end") || message.and_then(|m| text(field(m, "role"))) != Some("assistant") {
			continue;
		}
		if let Some(message) = message {
			tally.own(message);
		}
	}
	tally.response()
}

/// One `.jsonl` file under omp's session folder.
struct SessionFile {
	path: PathBuf,
	/// The folder's own file (the trial's sessions), not a subagent's in a subfolder.
	main: bool,
}

fn collect_jsonl(dir: &Path, depth: usize, found: &mut Vec<SessionFile>) {
	let Ok(entries) = fs::read_dir(dir) else { return };
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			collect_jsonl(&path, depth + 1, found);
		} else if path.extension().is_some_and(|ext| ext == "jsonl") {
			found.push(SessionFile { path, main: depth == 0 });
		}
	}
}

/// The `.jsonl` files under omp's session folder, the folder's own (the trial's sessions) first, then subagents'.
fn omp_session_files(dir: &Path) -> Vec<SessionFile> {
	let mut files = Vec::new();
	if dir.exists() {
		collect_jsonl(dir, 0, &mut files);
	}
	let key = |file: &SessionFile| file.path.to_string_lossy().into_owned();
	files.sort_by(|a, b| {
		b.main
			.cmp(&a.main)
			.then_with(|| key(a).to_lowercase().cmp(&key(b).to_lowercase()))
			.then_with(|| key(a).cmp(&key(b)))
	});
	files
}

fn read_text(path: &Path) -> Result<String> {
	Ok(String::from_utf8_lossy(&fs::read(path)?).into_owned())
}

/// The trial's own omp session files' text, joined: a plan-review trial's transcript (0567).
pub fn omp_session_text(dir: &Path) -> Result<String> {
	let mut joined = String::new();
	for file in omp_session_files(dir).iter().filter(|file| file.main) {
		let text = read_text(&file.path)?;
		joined.push_str(&text);
		if !text.is_empty() && !text.ends_with('\n') {
			joined.push('\n');
		}
	}
	Ok(joined)
}

/// From omp's session files in `dir` (a plan-review trial, whose first turn had a terminal, not JSON): as
/// [`read_response`] counts the JSON stream, the trial's own assistant messages give the model, `usage`, the turns
/// and their `toolCall` blocks; the subagents' files beside them add to `total_usage` and `requests`.
fn read_omp_sessions_response(dir: &Path) -> Result<HarnessResponse> {
	let mut tally = Tally::default();
	for file in omp_session_files(dir) {
		for entry in json_lines::<Value>(&read_text(&file.path)?) {
			let message = field(&entry, "message");
			if text(field(&entry, "type")) != Some("message")
				|| message.and_then(|m| text(field(m, "role"))) != Some("assistant")
			{
				continue;
			}
			let Some(message) = message else { continue };
			if file.main {
				tally.own(message);
			} else {
				tally.subagent_requests += 1;
				add_usage(&mut tally.subagent_usage, field(message, "usage"));
			}
		}
	}
	Ok(tally.response())
}

/// Whether omp's prewalk switched the trial's model (0608): the trial's own session files in `dir` hold a `model_change`
/// entry with the role `prewalk`. omp 18.22.0 writes that entry only when the hand-off fires: `PrewalkCoordinator.
/// advanceAtTurnEnd` (`session/prewalk.ts`) calls `setModelTemporary` after the first `edit`/`write` once the plan's
/// todo list exists, and the session's host passes `{ prewalk: true }` (`session/agent-session.ts`), so
/// `ModelControls.setModelTemporary` (`session/model-controls.ts`) appends it with `PREWALK_MODEL_CHANGE_ROLE`
/// (`session/session-entries.ts`). A noop (the target is already the active model) or a never-armed prewalk writes none.
/// Since 18.22.0 it is written only when the model switch landed.
pub fn read_prewalk_switch(dir: &Path) -> Result<bool> {
	for file in omp_session_files(dir).iter().filter(|file| file.main) {
		let switched = json_lines::<Value>(&read_text(&file.path)?).iter().any(|entry| {
			text(field(entry, "type")) == Some("model_change") && text(field(entry, "role")) == Some("prewalk")
		});
		if switched {
			return Ok(true);
		}
	}
	Ok(false)
}

/// From Claude Code's `-p --output-format stream-json --verbose` events (one `result` event per invocation, with
/// `total_cost_usd` and `usage`; `assistant` events carry `message.model` and `tool_use` blocks): usage summed over the
/// results, turns from `num_turns`, tool calls from the `tool_use` blocks. A run killed at the limit has no `result`
/// (EVL-3): its turns are then its distinct assistant messages, and its usage is the ledger's, from the saved
/// transcript. Claude Code's subagents are inside its own totals, so `total_usage` equals `usage`.
pub fn read_claude_response(transcript: &str) -> HarnessResponse {
	let mut usage = Usage::default();
	let mut model: Option<String> = None;
	let mut turns = 0.0;
	let mut results = 0;
	let mut tool_calls = 0;
	// Claude Code streams one `assistant` event per content block, each with its message's id.
	let mut messages: BTreeSet<String> = BTreeSet::new();
	for event in json_lines::<Value>(transcript) {
		let message = field(&event, "message");
		match text(field(&event, "type")) {
			Some("assistant") => {
				let id = message
					.and_then(|m| text(field(m, "id")))
					.or_else(|| text(field(&event, "uuid")))
					.map_or_else(|| format!("#{}", messages.len()), str::to_owned);
				messages.insert(id);
				if let Some(name) = message.and_then(|m| text(field(m, "model"))) {
					model = Some(name.to_owned());
				}
				tool_calls += blocks_of(message.and_then(|m| m.get("content")), "tool_use");
			}
			Some("result") => {
				results += 1;
				let used = field(&event, "usage");
				let count = |key: &str| number(used.and_then(|u| u.get(key)));
				usage.input += count("input_tokens");
				usage.output += count("output_tokens");
				usage.cache_read += count("cache_read_input_tokens");
				usage.cache_write += count("cache_creation_input_tokens");
				usage.cost += number(field(&event, "total_cost_usd"));
				turns += number(field(&event, "num_turns"));
			}
			_ => {}
		}
	}
	if results == 0 {
		turns = messages.len() as f64;
	}
	usage.total_tokens = usage.input + usage.output + usage.cache_read + usage.cache_write;
	let turns = turns as u64;
	HarnessResponse { model, usage, total_usage: usage, requests: turns, turns, tool_calls }
}

/// One opencode message's assistant turn into `response`'s tally.
fn tally_opencode_message(message: &Value, usage: &mut Usage, response: &mut HarnessResponse) {
	let info = field(message, "info");
	if info.and_then(|i| text(field(i, "role"))) != Some("assistant") {
		return;
	}
	response.turns += 1;
	if let Some(model) = info.and_then(|i| text(field(i, "modelID"))) {
		response.model = Some(model.to_owned());
	}
	let Some(Value::Array(parts)) = field(message, "parts") else { return };
	for part in parts {
		let kind = text(field(part, "type"));
		if kind == Some("tool") {
			response.tool_calls += 1;
		}
		if kind != Some("step-finish") {
			continue;
		}
		response.requests += 1;
		let tokens = field(part, "tokens");
		let count = |key: &str| number(tokens.and_then(|t| t.get(key)));
		let cache = |key: &str| number(tokens.and_then(|t| t.get("cache")).and_then(|c| c.get(key)));
		usage.input += count("input");
		usage.output += count("output") + count("reasoning");
		usage.cache_read += cache("read");
		usage.cache_write += cache("write");
		usage.cost += field(part, "cost").and_then(Value::as_f64).unwrap_or(0.0);
	}
}

/// From opencode's exports in `dir` (`opencode export` JSON, one per session): the model, each `step-finish` part's
/// tokens and cost summed (reasoning counted as output), assistant messages as turns and steps as requests, `tool`
/// parts as tool calls. The cost is opencode's own: on A7's API key, the bill.
pub fn read_opencode_response(dir: &Path) -> Result<HarnessResponse> {
	let mut usage = Usage::default();
	let mut response = HarnessResponse { model: None, usage, total_usage: usage, requests: 0, turns: 0, tool_calls: 0 };
	let mut names: Vec<String> = fs::read_dir(dir)
		.map(|entries| {
			entries
				.flatten()
				.filter_map(|entry| entry.file_name().into_string().ok())
				.filter(|name| name.ends_with(".json"))
				.collect()
		})
		.unwrap_or_default();
	names.sort_by_key(|name| name.to_lowercase());
	for name in names {
		let Ok(exported) = serde_json::from_str::<Value>(&read_text(&dir.join(&name))?) else {
			eprintln!("opencode export {name} isn't JSON; its usage is left out");
			continue;
		};
		if let Some(Value::Array(messages)) = field(&exported, "messages") {
			for message in messages {
				tally_opencode_message(message, &mut usage, &mut response);
			}
		}
	}
	usage.total_tokens = usage.input + usage.output + usage.cache_read + usage.cache_write;
	response.usage = usage;
	response.total_usage = usage;
	Ok(response)
}

/// Every distinct first group of `pattern` in `transcript`, in first-seen order.
fn sessions_named(pattern: &Regex, transcript: &str) -> Vec<String> {
	let mut seen: Vec<String> = Vec::new();
	for found in pattern.captures_iter(transcript) {
		let id = found[1].to_owned();
		if !seen.contains(&id) {
			seen.push(id);
		}
	}
	seen
}

static OPENCODE_SESSION: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#""sessionID"\s*:\s*"(ses_[A-Za-z0-9]+)""#).expect("a valid pattern"));
static CLAUDE_SESSION: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#""session_id"\s*:\s*"([0-9a-f-]{36})""#).expect("a valid pattern"));

/// The opencode sessions a `--format json` transcript names (`sessionID`), in first-seen order.
pub fn opencode_sessions(transcript: &str) -> Vec<String> {
	sessions_named(&OPENCODE_SESSION, transcript)
}

/// The Claude Code sessions a stream-json transcript names (`session_id`), in first-seen order.
pub fn claude_sessions(transcript: &str) -> Vec<String> {
	sessions_named(&CLAUDE_SESSION, transcript)
}

/// `cpSync(from, to, { recursive: true })`: `from`'s files and folders into `to`, made when missing.
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
	fs::create_dir_all(to)?;
	for entry in fs::read_dir(from)? {
		let entry = entry?;
		let target = to.join(entry.file_name());
		if entry.path().is_dir() {
			copy_tree(&entry.path(), &target)?;
		} else {
			fs::copy(entry.path(), target)?;
		}
	}
	Ok(())
}

fn omp_args(flags: &[String], turn: &Turn) -> Result<Vec<String>> {
	let mut args: Vec<String> = Vec::new();
	if let Some(profile) = &turn.profile {
		args.extend(["--profile".to_owned(), profile.clone()]);
	}
	if turn.follow_up {
		args.push("-c".to_owned());
	}
	args.extend(flags.iter().cloned());
	if let Some(dir) = &turn.session_dir {
		args.extend(["--session-dir".to_owned(), dir.clone()]);
	}
	args.extend(["--config".to_owned(), turn.overlay.clone()]);
	if let Some(extension) = &turn.extension {
		args.extend(["-e".to_owned(), extension.clone(), turn.prompt.clone()]);
	} else {
		args.extend(["-p".to_owned(), turn.prompt.clone(), "--mode".to_owned(), "json".to_owned()]);
	}
	Ok(args)
}

fn omp_save_sessions(dir: &Path, trial: &TrialRecords) -> Result<()> {
	match &trial.session_dir {
		Some(session_dir) if session_dir.exists() => copy_tree(session_dir, dir),
		_ => Ok(()),
	}
}

fn omp_response(trial: &ResponseSource) -> Result<HarnessResponse> {
	if trial.plan_review {
		return read_omp_sessions_response(trial.session_dir.unwrap_or(Path::new("")));
	}
	Ok(read_response(trial.transcript))
}

fn omp_assistant_text(transcript: &str) -> String {
	let mut texts: Vec<String> = Vec::new();
	for event in json_lines::<Value>(transcript) {
		let message = field(&event, "message");
		if text(field(&event, "type")) == Some("message_end")
			&& message.and_then(|m| text(field(m, "role"))) == Some("assistant")
		{
			texts.extend(text_blocks(message.and_then(|m| m.get("content"))));
		}
	}
	texts.join("\n")
}

/// omp's last assistant message (a `--mode json` `message_end`, or a session file's `message`) when its `stopReason`
/// is `error`, with its `errorMessage`.
fn omp_end_signal(transcript: &str) -> String {
	let mut last: Option<Value> = None;
	for event in json_lines::<Value>(transcript) {
		let kind = text(field(&event, "type"));
		let message = field(&event, "message");
		if (kind == Some("message_end") || kind == Some("message"))
			&& message.and_then(|m| text(field(m, "role"))) == Some("assistant")
		{
			last = message.cloned();
		}
	}
	let Some(last) = last.filter(|last| text(field(last, "stopReason")) == Some("error")) else { return String::new() };
	text(field(&last, "errorMessage")).unwrap_or("error").to_owned()
}

/// omp (0386): `[--profile <p>] [-c] <variant flags> [--session-dir <dir>] --config <overlay> -p <prompt> --mode json`;
/// a follow-up continues the trial's session (`-c`). An interactive turn (`extension` set: the plan-review extension,
/// 0567) has no `-p`/`--mode`: `<flags> [--session-dir <dir>] --config <overlay> -e <extension> <prompt>`. Its records
/// are the `--session-dir` folder as it is.
static OMP: HarnessAdapter = HarnessAdapter {
	accepts: &OMP_KEYS,
	args: omp_args,
	session_files: true,
	api_key_env: None,
	session: |_| None,
	records_dir: None,
	save_sessions: omp_save_sessions,
	response: omp_response,
	assistant_text: omp_assistant_text,
	end_signal: omp_end_signal,
};

fn claude_args(flags: &[String], turn: &Turn) -> Result<Vec<String>> {
	let mut args: Vec<String> = Vec::new();
	if turn.follow_up {
		args.push("-c".to_owned());
	}
	args.extend(flags.iter().cloned());
	for word in ["-p", &turn.prompt, "--output-format", "stream-json", "--verbose"] {
		args.push(word.to_owned());
	}
	Ok(args)
}

fn claude_save_sessions(dir: &Path, trial: &TrialRecords) -> Result<()> {
	let home = trial
		.env
		.get("CLAUDE_CONFIG_DIR")
		.map_or_else(|| std::env::home_dir().unwrap_or_default().join(".claude"), PathBuf::from);
	let projects = home.join("projects");
	let folders: Vec<PathBuf> = fs::read_dir(&projects)
		.map(|entries| entries.flatten().map(|entry| entry.path()).collect())
		.unwrap_or_default();
	for id in claude_sessions(&trial.transcript) {
		let file = format!("{id}.jsonl");
		let Some(folder) = folders.iter().find(|path| path.join(&file).exists()) else {
			eprintln!("no Claude Code transcript for session {id} under {}", projects.display());
			continue;
		};
		fs::copy(folder.join(&file), dir.join(&file))?;
		let subagents = folder.join(&id).join("subagents");
		if subagents.exists() {
			copy_tree(&subagents, &dir.join(&id).join("subagents"))?;
		}
	}
	Ok(())
}

fn claude_response(trial: &ResponseSource) -> Result<HarnessResponse> {
	Ok(read_claude_response(trial.transcript))
}

fn claude_assistant_text(transcript: &str) -> String {
	let mut texts: Vec<String> = Vec::new();
	for event in json_lines::<Value>(transcript) {
		if text(field(&event, "type")) == Some("assistant") {
			texts.extend(text_blocks(field(&event, "message").and_then(|m| m.get("content"))));
		}
	}
	texts.join("\n")
}

/// `String(value)` for a JSON value, as the claude end signal joins and prints them.
fn js_string(value: Option<&Value>) -> String {
	match value {
		None => "undefined".to_owned(),
		Some(Value::Null) => "null".to_owned(),
		Some(Value::String(text)) => text.clone(),
		Some(Value::Object(_)) => "[object Object]".to_owned(),
		Some(Value::Array(items)) => items.iter().map(|item| js_string(Some(item))).collect::<Vec<_>>().join(","),
		Some(other) => other.to_string(),
	}
}

/// Its `result` with `is_error` (its text and `api_error_status`) and a `rate_limit_event` whose status isn't
/// `allowed`.
fn claude_end_signal(transcript: &str) -> String {
	let mut said: Vec<String> = Vec::new();
	for event in json_lines::<Value>(transcript) {
		let kind = text(field(&event, "type"));
		if kind == Some("result") && field(&event, "is_error") == Some(&Value::Bool(true)) {
			let status = field(&event, "api_error_status").filter(|status| !status.is_null());
			let result = text(field(&event, "result")).unwrap_or("");
			let mut parts = vec!["result error".to_owned()];
			parts.extend(status.map(|status| js_string(Some(status))));
			parts.push(result.to_owned());
			said.push(parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" "));
		}
		let status = field(&event, "rate_limit_info").and_then(|info| info.get("status"));
		if kind == Some("rate_limit_event") && text(status) != Some("allowed") {
			said.push(format!("rate limit {}", js_string(status)));
		}
	}
	said.join("\n")
}

/// Claude Code (0482 E4): `[-c] <variant flags> -p <prompt> --output-format stream-json --verbose` (stream-json needs
/// `--verbose` in print mode). Its records are `<session>.jsonl` and `<session>/subagents/*.jsonl` from
/// `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*`.
static CLAUDE: HarnessAdapter = HarnessAdapter {
	accepts: &[OmpKey::AppendSystemPrompt],
	args: claude_args,
	session_files: false,
	api_key_env: None,
	session: |_| None,
	records_dir: None,
	save_sessions: claude_save_sessions,
	response: claude_response,
	assistant_text: claude_assistant_text,
	end_signal: claude_end_signal,
};

fn opencode_args(flags: &[String], turn: &Turn) -> Result<Vec<String>> {
	if turn.follow_up && turn.session.is_none() {
		return Err(Error::other("an opencode follow-up needs the first turn's session"));
	}
	let mut args = vec!["run".to_owned()];
	if turn.follow_up {
		args.extend(["--session".to_owned(), turn.session.clone().unwrap_or_default()]);
	}
	args.extend(flags.iter().cloned());
	args.extend(["--format".to_owned(), "json".to_owned(), turn.prompt.clone()]);
	Ok(args)
}

fn opencode_save_sessions(dir: &Path, trial: &TrialRecords) -> Result<()> {
	let Some((command, launcher_args)) = trial.launcher.split_first() else { return Ok(()) };
	for id in opencode_sessions(&trial.transcript) {
		let mut argv = vec![command.clone()];
		argv.extend(launcher_args.iter().cloned());
		argv.extend(["export".to_owned(), id.clone()]);
		let options = RunOptions {
			cwd: Some(trial.cwd.clone()),
			env: Some(trial.env.clone()),
			timeout_ms: Some(120_000),
			max_bytes: Some(256 * 1024 * 1024),
			..RunOptions::default()
		};
		let exported = match run_program_sync(&argv, options) {
			Ok(run) if run.code == Some(0) && run.stdout.trim_start().starts_with('{') => run,
			Ok(run) => {
				eprintln!("opencode export {id} failed: {}", run.stderr);
				continue;
			}
			Err(error) => {
				eprintln!("opencode export {id} failed: {error}");
				continue;
			}
		};
		let body = trial.scrub.as_ref().map_or_else(|| exported.stdout.clone(), |scrub| scrub(&exported.stdout));
		fs::write(dir.join(format!("{id}.json")), body)?;
	}
	Ok(())
}

fn opencode_response(trial: &ResponseSource) -> Result<HarnessResponse> {
	read_opencode_response(trial.saved_sessions.unwrap_or(Path::new("")))
}

fn opencode_assistant_text(transcript: &str) -> String {
	let mut texts: Vec<String> = Vec::new();
	for event in json_lines::<Value>(transcript) {
		let part = field(&event, "part");
		if part.and_then(|p| text(field(p, "type"))) == Some("text")
			&& let Some(body) = part.and_then(|p| text(field(p, "text")))
		{
			texts.push(body.to_owned());
		}
	}
	texts.join("\n")
}

/// Its `error` events.
fn opencode_end_signal(transcript: &str) -> String {
	let mut said: Vec<String> = Vec::new();
	for event in json_lines::<Value>(transcript) {
		if text(field(&event, "type")) == Some("error") {
			let error = field(&event, "error").filter(|error| !error.is_null()).unwrap_or(&event);
			said.push(error.to_string());
		}
	}
	said.join("\n")
}

/// opencode (A7, 0537): `run [--session <id>] <variant flags> --format json <prompt>`; a follow-up names the first
/// turn's session, since `--continue` takes the machine's last session, which may be another trial's. Its records are
/// `opencode export <session>` of each session into `<session>.json`, through `scrub` (A7's key out, EVL-13), and its
/// usage is read from them.
static OPENCODE: HarnessAdapter = HarnessAdapter {
	accepts: &[],
	args: opencode_args,
	session_files: false,
	api_key_env: Some(A7_API_KEY_ENV),
	session: |output| opencode_sessions(output).into_iter().next(),
	records_dir: Some("opencode-sessions"),
	save_sessions: opencode_save_sessions,
	response: opencode_response,
	assistant_text: opencode_assistant_text,
	end_signal: opencode_end_signal,
};

/// The adapter of `harness`.
pub fn adapter(harness: Harness) -> &'static HarnessAdapter {
	match harness {
		Harness::Omp => &OMP,
		Harness::Claude => &CLAUDE,
		Harness::Opencode => &OPENCODE,
	}
}

/// The assistant's text in one turn's transcript, joined by newlines: omp's assistant `message_end` text blocks, Claude
/// Code's `assistant` text blocks, opencode's `text` parts.
pub fn assistant_text(transcript: &str, harness: Harness) -> String {
	(adapter(harness).assistant_text)(transcript)
}

/// Saves the trial's records into `dir` (emptied first) for the ledger (0538), as the harness keeps them (see each
/// adapter). A session that can't be found or exported is reported on stderr; the ledger then lacks it.
pub fn save_sessions(harness: Harness, dir: &Path, trial: &TrialRecords) -> Result<()> {
	remove_tree(dir)?;
	fs::create_dir_all(dir)?;
	(adapter(harness).save_sessions)(dir, trial)
}
