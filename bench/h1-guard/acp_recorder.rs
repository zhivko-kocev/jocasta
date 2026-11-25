//! H1's live recorder on omp 19.2.0 (0585, 0623, 0690): an ACP client that gives omp a terminal and records what omp sends to
//! it.
//!
//! omp's ACP mode (`omp --mode acp`, `modes/acp/acp-mode.ts`) talks JSON-RPC 2.0 over stdio, one JSON object per line.
//! A client that advertises the `terminal` capability at `initialize` gets every bash call that passed omp's approval
//! routed to `terminal/create` instead of omp's own shell (`tools/bash.ts:2107-2109,2236-2516`): the approval gate runs
//! first, in the ACP permission proxy (`session/session-tools.ts:1243-1295`, `resolveApproval` with the configured
//! mode; with `tools.approvalMode: yolo` set explicitly a deny throws, an allow runs, a prompt asks the client with
//! `session/request_permission`) and in the tool wrapper (`extensibility/extensions/wrapper.ts:372-389`). So the client
//! sees exactly what omp let through: a `terminal/create` is "ran", a permission request is "asked" (the recorder
//! rejects it once), and a call that ends `failed` with omp's policy text and neither is "refused". The terminal this
//! client creates runs nothing: it answers `terminal/output` and `terminal/wait_for_exit` with the recorder's marker and
//! exit code 0.
//! Three ways in omp's bash tool skip the client terminal and run in omp's own process: `pty: true`, a service (`name`)
//! and an async job, plus a working folder that is an internal URL (`skill://…`), as a `cwd` or a leading `cd`. The
//! recorder extension ([`RECORDER_EXTENSION`], loaded with `-e`) closes them on the `tool_call` event, which omp emits
//! before the approval gate and whose returned `input` replaces the call's arguments (`session/agent-session.ts:
//! 5871-5911`): it drops `pty`, `name`, `ready` and `async` (none of them changes the approval of the command) and
//! blocks a call whose working folder is a URL. The overlay also turns service launch and async jobs off.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value, json};

use jocasta::shared::env;
use jocasta::shared::jsonl::parse_line;

/// What the recorder's terminal shows for every command; nothing runs.
pub const LIVE_MARKER: &str = "H1 live check: recorded, not run.";

/// The `-e` extension: every bash call keeps to the client terminal (see the header). Plain JavaScript, so omp loads it
/// as written.
pub const RECORDER_EXTENSION: &str = r#"// H1 live check (0585): keeps every bash call on the ACP client's terminal, where it is recorded and nothing runs.
const URL_FOLDER = /^[a-z][a-z0-9+.-]*:\/\//i;
const LEADING_CD = /^\s*cd\s+["']?([^\s"';&|]+)/;
const OFF_ROUTE = ["pty", "name", "ready", "async"];
export default function h1LiveRecorder(pi) {
	pi.on("tool_call", (event) => {
		if (event.toolName !== "bash") return { block: true, reason: "H1 live check: only bash is recorded" };
		const input = { ...event.input };
		const cd = LEADING_CD.exec(String(input.command ?? ""))?.[1];
		if ((typeof input.cwd === "string" && URL_FOLDER.test(input.cwd)) || (cd !== undefined && URL_FOLDER.test(cd)))
			return { block: true, reason: "H1 live check: a URL working folder runs in omp's own shell, so it is not recorded" };
		const dropped = OFF_ROUTE.filter((key) => key in input);
		for (const key of dropped) delete input[key];
		return dropped.length > 0 ? { input } : undefined;
	});
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
	Refused,
	Asked,
	Ran,
	NoCall,
	Other,
}

impl Outcome {
	/// The outcome as the rows write it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Refused => "refused",
			Self::Asked => "asked",
			Self::Ran => "ran",
			Self::NoCall => "no-call",
			Self::Other => "other",
		}
	}
}

/// One thing the client saw, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpEvent {
	ToolCall { tool_call_id: String, name: Option<String>, raw_input: Option<Map<String, Value>> },
	ToolResult { tool_call_id: String, status: String, text: String },
	Permission { tool_call_id: Option<String> },
	Terminal { command: String, args: Vec<String>, cwd: Option<String> },
}

#[derive(Debug, Clone)]
pub struct AcpTurn {
	pub events: Vec<AcpEvent>,
	pub stop_reason: Option<String>,
	/// Why the turn didn't finish normally (timeout, omp exited, an error answer); `None` when it did.
	pub failure: Option<String>,
	pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRun {
	pub outcome: Outcome,
	/// The command the model passed to bash, if it called it.
	pub called_with: Option<String>,
	pub same_command: bool,
	pub bash_calls: usize,
	/// The first call's arguments other than `command` (the recorder extension drops the off-route ones).
	pub other_args: Vec<String>,
	pub result_text: Option<String>,
	/// The command line omp gave the recorder's terminal for the first call (its shell and the line).
	pub terminal: Option<String>,
}

fn as_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
	value?.as_object()
}

/// The text parts of an ACP `tool_call_update`'s `content` (`{type: "content", content: {type: "text", text}}`).
fn content_text(content: Option<&Value>) -> String {
	let Some(items) = content.and_then(Value::as_array) else { return String::new() };
	items
		.iter()
		.filter_map(|item| {
			let inner = as_record(as_record(Some(item))?.get("content"))?;
			let text = inner.get("text").and_then(Value::as_str)?;
			(inner.get("type").and_then(Value::as_str) == Some("text") && !text.is_empty()).then_some(text)
		})
		.collect::<Vec<_>>()
		.join("\n")
}

/// The client's record of one `session/update` notification, or `None` when it isn't about a tool call.
pub fn event_of_update(params: Option<&Value>) -> Option<AcpEvent> {
	let update = as_record(as_record(params)?.get("update"))?;
	let tool_call_id = update.get("toolCallId")?.as_str()?.to_owned();
	let kind = update.get("sessionUpdate").and_then(Value::as_str);
	let status = update.get("status").and_then(Value::as_str);
	if kind == Some("tool_call") {
		return Some(AcpEvent::ToolCall {
			tool_call_id,
			name: update.get("name").and_then(Value::as_str).map(str::to_owned),
			raw_input: as_record(update.get("rawInput")).cloned(),
		});
	}
	match (kind, status) {
		(Some("tool_call_update"), Some(status @ ("completed" | "failed"))) => Some(AcpEvent::ToolResult {
			tool_call_id,
			status: status.to_owned(),
			text: content_text(update.get("content")),
		}),
		_ => None,
	}
}

/// What omp did with the first bash call of a turn: a permission request for it is "asked", a terminal created while it
/// ran is "ran", a failed result with omp's policy text is "refused", omp's own "requires approval" error is "asked"
/// (the wrapper's gate without a client dialog), anything else "other".
pub fn classify_turn(events: &[AcpEvent], intended: &str) -> LiveRun {
	let is_bash = |event: &AcpEvent| matches!(event, AcpEvent::ToolCall { name: Some(name), .. } if name == "bash");
	let Some(start) = events.iter().position(is_bash) else {
		return LiveRun {
			outcome: Outcome::NoCall,
			called_with: None,
			same_command: false,
			bash_calls: 0,
			other_args: Vec::new(),
			result_text: None,
			terminal: None,
		};
	};
	let AcpEvent::ToolCall { tool_call_id: first_id, raw_input, .. } = &events[start] else {
		unreachable!("the position matched a tool call")
	};
	let called_with =
		raw_input.as_ref().and_then(|input| input.get("command")).and_then(Value::as_str).map(str::to_owned);
	let end_at = events
		.iter()
		.enumerate()
		.skip(start + 1)
		.find(|(_, event)| matches!(event, AcpEvent::ToolResult { tool_call_id, .. } if tool_call_id == first_id))
		.map(|(at, _)| at);
	let during = &events[start + 1..end_at.unwrap_or(events.len())];
	let end = end_at.map(|at| &events[at]);
	let terminal = during.iter().find_map(|event| match event {
		AcpEvent::Terminal { command, args, .. } => {
			Some(std::iter::once(command.clone()).chain(args.clone()).collect::<Vec<_>>().join(" "))
		}
		_ => None,
	});
	let asked =
		during.iter().any(|event| matches!(event, AcpEvent::Permission { tool_call_id: Some(id) } if id == first_id));
	let (status, text) = match end {
		Some(AcpEvent::ToolResult { status, text, .. }) => (Some(status.as_str()), Some(text.as_str())),
		_ => (None, None),
	};
	let outcome = if asked {
		Outcome::Asked
	} else if terminal.is_some() {
		Outcome::Ran
	} else if status == Some("failed")
		&& text.is_some_and(|text| {
			text.contains("is blocked by tool policy") || text.contains("is blocked by user policy")
		}) {
		Outcome::Refused
	} else if status == Some("failed")
		&& text.is_some_and(|text| text.contains("requires approval but no interactive UI available"))
	{
		Outcome::Asked
	} else {
		Outcome::Other
	};
	LiveRun {
		outcome,
		same_command: called_with.as_deref() == Some(intended),
		called_with,
		bash_calls: events.iter().filter(|event| is_bash(event)).count(),
		other_args: raw_input.iter().flat_map(|input| input.keys()).filter(|key| *key != "command").cloned().collect(),
		result_text: text.map(str::to_owned),
		terminal,
	}
}

/// A refusal the recorder gives omp's request: JSON-RPC's code and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
	pub code: i64,
	pub message: String,
}

/// The recorder's answer to one request omp makes of its client; `events` gets what it saw.
pub fn answer_agent_request(
	method: &str,
	params: Option<&Value>,
	events: &mut Vec<AcpEvent>,
) -> Result<Value, RpcError> {
	let empty = Map::new();
	let fields = as_record(params).unwrap_or(&empty);
	match method {
		"session/request_permission" => {
			let tool_call = as_record(fields.get("toolCall"));
			events.push(AcpEvent::Permission {
				tool_call_id: tool_call
					.and_then(|call| call.get("toolCallId"))
					.and_then(Value::as_str)
					.map(str::to_owned),
			});
			// Reject once: the call doesn't run, and nothing is remembered for later calls.
			let reject = fields
				.get("options")
				.and_then(Value::as_array)
				.into_iter()
				.flatten()
				.filter_map(|option| option.as_object())
				.find(|option| option.get("kind").and_then(Value::as_str) == Some("reject_once"))
				.and_then(|option| option.get("optionId"))
				.and_then(Value::as_str);
			Ok(match reject {
				Some(option_id) => json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
				None => json!({ "outcome": { "outcome": "cancelled" } }),
			})
		}
		"terminal/create" => {
			let made = events.iter().filter(|event| matches!(event, AcpEvent::Terminal { .. })).count();
			events.push(AcpEvent::Terminal {
				command: fields.get("command").and_then(Value::as_str).unwrap_or("").to_owned(),
				args: fields
					.get("args")
					.and_then(Value::as_array)
					.map(|args| args.iter().map(js_string).collect())
					.unwrap_or_default(),
				cwd: fields.get("cwd").and_then(Value::as_str).map(str::to_owned),
			});
			Ok(json!({ "terminalId": format!("h1-terminal-{}", made + 1) }))
		}
		"terminal/output" => Ok(
			json!({ "output": format!("{LIVE_MARKER}\n"), "truncated": false, "exitStatus": { "exitCode": 0, "signal": null } }),
		),
		"terminal/wait_for_exit" => Ok(json!({ "exitCode": 0, "signal": null })),
		"terminal/kill" | "terminal/release" => Ok(json!({})),
		_ => Err(RpcError { code: -32601, message: format!("the H1 recorder doesn't answer {method}") }),
	}
}

/// JavaScript's `String(value)` for a JSON value.
fn js_string(value: &Value) -> String {
	match value {
		Value::String(text) => text.clone(),
		Value::Null => "null".to_owned(),
		other => other.to_string(),
	}
}

/// What one turn is given.
pub struct TurnOptions<'a> {
	/// omp in ACP mode: the program and its arguments.
	pub argv: &'a [String],
	pub cwd: &'a Path,
	pub prompt: &'a str,
	/// The program's whole environment; this process's when `None`.
	pub env: Option<HashMap<String, String>>,
	pub timeout_ms: u64,
}

/// A line of omp's stdout, or the end of it.
enum Piece {
	Line(String),
	Closed,
}

fn start(options: &TurnOptions<'_>) -> std::io::Result<Child> {
	let (program, rest) = options.argv.split_first().map_or(("", &[][..]), |(first, rest)| (first.as_str(), rest));
	let mut command = Command::new(program);
	command.args(rest).current_dir(options.cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
	match &options.env {
		Some(vars) => {
			command.env_clear().envs(vars);
		}
		None => env::apply_to(&mut command),
	}
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		command.creation_flags(0x0800_0000);
	}
	command.spawn()
}

/// What a request's answer leaves for the next step, or why the turn can't go on.
enum Step {
	Next,
	Done(Option<String>),
	Failed(String),
}

/// One turn's conversation with omp: `initialize` with the terminal capability, `session/new`, then `session/prompt`.
struct Conversation<'a> {
	stdin: std::process::ChildStdin,
	prompt: &'a str,
	cwd: &'a Path,
	next_id: u64,
	/// The request awaiting its answer: its id and method.
	pending: Option<(u64, &'static str)>,
	events: Vec<AcpEvent>,
}

impl Conversation<'_> {
	fn write(&mut self, message: Value) {
		let mut line = json!({ "jsonrpc": "2.0" });
		if let (Some(target), Some(fields)) = (line.as_object_mut(), message.as_object()) {
			target.extend(fields.clone());
		}
		let _ = writeln!(self.stdin, "{line}");
		let _ = self.stdin.flush();
	}

	fn request(&mut self, method: &'static str, params: Value) {
		let id = self.next_id;
		self.next_id += 1;
		self.pending = Some((id, method));
		self.write(json!({ "id": id, "method": method, "params": params }));
	}

	fn begin(&mut self) {
		self.request(
			"initialize",
			json!({
				"protocolVersion": 1,
				"clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": true }
			}),
		);
	}

	/// One message from omp: a notification, a request to answer, or the answer to ours.
	fn on_message(&mut self, message: &Map<String, Value>) -> Step {
		let id = message.get("id");
		if let Some(method) = message.get("method").and_then(Value::as_str) {
			let Some(id) = id else {
				if method == "session/update" {
					self.events.extend(event_of_update(message.get("params")));
				}
				return Step::Next;
			};
			let params = message.get("params");
			match answer_agent_request(method, params, &mut self.events) {
				Ok(result) => self.write(json!({ "id": id, "result": result })),
				Err(error) => {
					self.write(json!({ "id": id, "error": { "code": error.code, "message": error.message } }))
				}
			}
			return Step::Next;
		}
		let Some((pending_id, method)) = self.pending else { return Step::Next };
		if id.and_then(Value::as_u64) != Some(pending_id) {
			return Step::Next;
		}
		self.pending = None;
		if let Some(error) = as_record(message.get("error")) {
			let shown = |field: &str| error.get(field).map_or_else(|| "undefined".to_owned(), js_string);
			return Step::Failed(format!("omp answered {}: {}", shown("code"), shown("message")));
		}
		self.answered(method, message.get("result"))
	}

	fn answered(&mut self, method: &str, result: Option<&Value>) -> Step {
		match method {
			"initialize" => {
				let cwd = self.cwd.to_string_lossy().into_owned();
				self.request("session/new", json!({ "cwd": cwd, "mcpServers": [] }));
				Step::Next
			}
			"session/new" => {
				let Some(session) =
					as_record(result).and_then(|fields| fields.get("sessionId")).and_then(Value::as_str)
				else {
					return Step::Failed("omp's session/new answered no sessionId".to_owned());
				};
				let params = json!({ "sessionId": session, "prompt": [{ "type": "text", "text": self.prompt }] });
				self.request("session/prompt", params);
				Step::Next
			}
			_ => Step::Done(
				as_record(result)
					.and_then(|fields| fields.get("stopReason"))
					.and_then(Value::as_str)
					.map(str::to_owned),
			),
		}
	}
}

fn pump(reader: impl BufRead + Send + 'static) -> Receiver<Piece> {
	let (send, receive) = mpsc::channel();
	thread::spawn(move || {
		for line in reader.lines() {
			let Ok(line) = line else { break };
			if send.send(Piece::Line(line)).is_err() {
				return;
			}
		}
		let _ = send.send(Piece::Closed);
	});
	receive
}

/// How a turn ended before its answer came, when it did.
enum Ending {
	Timeout,
	Exited(Option<i32>),
}

/// One turn of `options.argv` (omp in ACP mode) in `options.cwd`: `initialize` with the terminal capability,
/// `session/new`, then `session/prompt` with `options.prompt`; answers omp's requests with [`answer_agent_request`]
/// until the prompt's answer comes, or `options.timeout_ms` passes. omp is ended afterwards either way.
pub fn run_acp_turn(options: &TurnOptions<'_>) -> AcpTurn {
	let mut child = match start(options) {
		Ok(child) => child,
		Err(_) => {
			return AcpTurn {
				events: Vec::new(),
				stop_reason: None,
				failure: Some("omp exited (null) before the turn ended".to_owned()),
				exit_code: None,
			};
		}
	};
	let stdout = child.stdout.take().map(|pipe| pump(BufReader::new(pipe)));
	let stdin = child.stdin.take().expect("a piped stdin");
	let mut talk =
		Conversation { stdin, prompt: options.prompt, cwd: options.cwd, next_id: 1, pending: None, events: Vec::new() };
	talk.begin();
	let deadline = Instant::now() + Duration::from_millis(options.timeout_ms);
	let mut finished: Option<Step> = None;
	let mut ending: Option<Ending> = None;
	while finished.is_none() && ending.is_none() {
		if Instant::now() >= deadline {
			ending = Some(Ending::Timeout);
			break;
		}
		let piece = stdout
			.as_ref()
			.map_or(Err(RecvTimeoutError::Disconnected), |out| out.recv_timeout(Duration::from_millis(10)));
		match piece {
			Ok(Piece::Line(line)) => {
				if let Some(message) = parse_line(&line) {
					finished = match talk.on_message(&message) {
						Step::Next => None,
						done => Some(done),
					};
				}
			}
			Ok(Piece::Closed) | Err(RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(10)),
			Err(RecvTimeoutError::Timeout) => {}
		}
		if finished.is_none()
			&& let Ok(Some(status)) = child.try_wait()
		{
			finished = drain(stdout.as_ref(), &mut talk);
			if finished.is_none() {
				ending = Some(Ending::Exited(status.code()));
			}
		}
	}
	let (stop_reason, failure) = match (finished, ending) {
		(Some(Step::Done(reason)), _) => (reason, None),
		(Some(Step::Failed(why)), _) => (None, Some(why)),
		(_, Some(Ending::Timeout)) => (None, Some("timeout".to_owned())),
		(_, Some(Ending::Exited(code))) => (
			None,
			Some(format!("omp exited ({}) before the turn ended", code.map_or("null".to_owned(), |c| c.to_string()))),
		),
		_ => (None, None),
	};
	drop(talk.stdin);
	let exit_code = match child.try_wait() {
		Ok(Some(status)) => status.code(),
		_ => {
			let _ = child.kill();
			let _ = child.wait();
			None
		}
	};
	AcpTurn { events: talk.events, stop_reason, failure, exit_code }
}

/// What omp wrote before it exited: its last lines still count (the turn may have ended in them). Waits briefly for
/// the pipe to close, as a grandchild may hold it open.
fn drain(stdout: Option<&Receiver<Piece>>, talk: &mut Conversation<'_>) -> Option<Step> {
	let out = stdout?;
	while let Ok(Piece::Line(line)) = out.recv_timeout(Duration::from_millis(200)) {
		if let Some(message) = parse_line(&line) {
			match talk.on_message(&message) {
				Step::Next => {}
				done => return Some(done),
			}
		}
	}
	None
}

#[cfg(test)]
#[path = "acp_recorder_tests.rs"]
mod tests;
