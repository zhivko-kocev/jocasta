//! Ports of `test/mcp/server.test.ts`, the `jocasta mcp` case of `test/entry.test.ts`, and a pin of `tools/list`. The
//! server runs on rmcp (0632), so the cases that pinned the hand-written dispatch's lifecycle assert rmcp's.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, Uri};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::shared::paths::data_dir;
use crate::shared::service_file::service_file_path;
use crate::shared::testing::temp_jocasta;

/// A thread pane's environment (0577): `report_result` and `check` are listed there.
fn thread_env() -> HashMap<String, String> {
	HashMap::from([("JOCASTA_THREAD".to_owned(), "t-1".to_owned())])
}

/// What a test changes in the server's options: outside a thread, without a service, unless it says otherwise.
struct Options {
	boss: bool,
	service: Option<ServiceLookup>,
	pane_key: Option<String>,
	boss_key: Option<String>,
	env: Option<HashMap<String, String>>,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			boss: false,
			service: Some(Arc::new(|| None)),
			pane_key: None,
			boss_key: None,
			env: Some(HashMap::new()),
		}
	}
}

fn options_of(options: Options) -> McpServerOptions {
	McpServerOptions {
		boss: options.boss,
		service: options.service,
		pane_key: options.pane_key,
		boss_key: options.boss_key,
		env: options.env,
	}
}

/// Feeds `text` to a server and reads every line it answers until it ends.
async fn exchange(text: String, options: Options) -> Vec<Value> {
	let (mut to_server, input) = tokio::io::duplex(1 << 20);
	let (output, mut from_server) = tokio::io::duplex(1 << 20);
	let server = tokio::spawn(run_mcp_server(options_of(options), input, output));
	to_server.write_all(text.as_bytes()).await.unwrap();
	drop(to_server);
	let mut answered = String::new();
	tokio::time::timeout(std::time::Duration::from_secs(20), from_server.read_to_string(&mut answered))
		.await
		.expect("the server ends with its input")
		.unwrap();
	server.await.unwrap();
	answered.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// Feeds `lines` (raw text each, a newline appended) to a server as they are: no handshake.
async fn raw_session(lines: &[String], options: Options) -> Vec<Value> {
	exchange(lines.iter().map(|line| format!("{line}\n")).collect(), options).await
}

/// Feeds `lines` after omp's handshake (rmcp serves nothing before it), outside a thread unless `options.env` says
/// so; the handshake's own reply is left out.
async fn session(lines: &[String], options: Options) -> Vec<Value> {
	let mut all = vec![
		request(0, "initialize", Some(initialize_params(MCP_PROTOCOL_VERSION))),
		json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
	];
	all.extend_from_slice(lines);
	let mut replies = raw_session(&all, options).await;
	assert_eq!(replies.remove(0)["id"], 0);
	replies
}

fn initialize_params(version: &str) -> Value {
	json!({ "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } })
}

fn request(id: i64, method: &str, params: Option<Value>) -> String {
	let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method });
	if let Some(params) = params {
		message["params"] = params;
	}
	message.to_string()
}

fn call(id: i64, name: &str, arguments: Value) -> String {
	request(id, "tools/call", Some(json!({ "name": name, "arguments": arguments })))
}

/// omp's handshake (`mcp/host/legacy-session.ts:10-34`, after the `server/discover` probe of `mcp/host/peers.ts`).
fn handshake() -> Vec<String> {
	vec![
		request(
			1,
			"initialize",
			Some(json!({
				"protocolVersion": MCP_PROTOCOL_VERSION,
				"capabilities": { "roots": { "listChanged": false } },
				"clientInfo": { "name": "omp", "version": "18.10.3" },
			})),
		),
		json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
	]
}

type Calls = Arc<Mutex<Vec<(String, Value, Option<String>)>>>;

struct Fake {
	address: RunningService,
	calls: Calls,
}

/// A service on a port of its own: each call it gets is kept; `answer` gives the status and body for a tool.
async fn fake_service(answer: impl Fn(&str) -> (u16, String) + Send + Sync + 'static) -> Fake {
	let calls: Calls = Arc::default();
	let kept = Arc::clone(&calls);
	let answer = Arc::new(answer);
	let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap, body: Bytes| {
		let tool = uri.path().replace("/api/mcp/", "");
		let auth = headers.get("authorization").and_then(|value| value.to_str().ok()).map(str::to_owned);
		kept.lock().unwrap().push((tool.clone(), serde_json::from_slice(&body).unwrap(), auth));
		let (status, text) = answer(&tool);
		async move { (StatusCode::from_u16(status).unwrap(), text) }
	});
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let port = listener.local_addr().unwrap().port();
	tokio::spawn(async move { axum::serve(listener, app).await });
	Fake { address: RunningService { port, token: "tok-1".to_owned() }, calls }
}

fn at(fake: &Fake) -> Option<ServiceLookup> {
	let address = fake.address.clone();
	Some(Arc::new(move || Some(address.clone())))
}

fn not_running() -> Value {
	json!({ "content": [{ "type": "text", "text": "Jocasta isn't running; start it with jocasta." }], "isError": true })
}

fn tool_error(text: &str) -> Value {
	json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

fn names(reply: &Value) -> Vec<&str> {
	reply["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn initialize_answers_omps_protocol_version_and_a_tools_capability_the_notification_gets_no_reply() {
	let mut lines = handshake();
	lines.push(request(2, "ping", None));
	let replies = raw_session(&lines, Options::default()).await;
	assert_eq!(replies.len(), 2);
	assert_eq!(
		replies[0]["result"],
		json!({
			"protocolVersion": "2025-11-25",
			"capabilities": { "tools": { "listChanged": false } },
			"serverInfo": { "name": "jocasta", "version": env!("CARGO_PKG_VERSION") },
		})
	);
	assert_eq!(replies[0]["id"], 1);
	assert_eq!(replies[1]["result"], json!({}));
	assert_eq!(replies[1]["id"], 2);
}

/// omp 19's host probes a server's era with `server/discover` written right ahead of `initialize` (`mcp/host/peers.ts`
/// `probeStdio`); an unsupported-version error would make it a 2026-07-28 server shared by every pane, so the probe
/// gets "method not found" and the handshake goes on as in omp 18.
#[tokio::test]
async fn omps_era_probe_gets_method_not_found_and_the_handshake_after_it_works() {
	let discover = request(
		1,
		"server/discover",
		Some(json!({ "_meta": {
			"io.modelcontextprotocol/protocolVersion": "2026-07-28",
			"io.modelcontextprotocol/clientCapabilities": {},
			"io.modelcontextprotocol/clientInfo": { "name": "omp", "version": "19.1.0" },
		} })),
	);
	let mut lines = vec![discover, request(2, "initialize", Some(initialize_params(MCP_PROTOCOL_VERSION)))];
	lines.push(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string());
	lines.push(request(3, "tools/list", Some(json!({}))));
	let replies = raw_session(&lines, Options::default()).await;
	assert_eq!(replies.len(), 3);
	assert_eq!(replies[0]["id"], 1);
	assert_eq!(replies[0]["error"]["code"], -32601);
	assert_eq!(replies[1]["id"], 2);
	assert_eq!(replies[1]["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
	assert!(replies[2]["result"]["tools"].as_array().is_some_and(|tools| !tools.is_empty()));
}

/// What the server offers a client that asks for `version`.
async fn negotiated(version: &str) -> Value {
	let lines = [request(1, "initialize", Some(initialize_params(version)))];
	raw_session(&lines, Options::default()).await.remove(0)["result"]["protocolVersion"].clone()
}

#[tokio::test]
async fn initialize_echoes_an_older_supported_version_and_offers_its_own_for_an_unknown_one() {
	for older in ["2025-06-18", "2025-03-26", "2024-11-05", MCP_PROTOCOL_VERSION] {
		assert_eq!(negotiated(older).await, older);
	}
	// Newer than this server's methods, or unknown: its own.
	for other in ["1999-01-01", "2026-07-28", "2099-01-01"] {
		assert_eq!(negotiated(other).await, MCP_PROTOCOL_VERSION);
	}
}

async fn listed(options: Options) -> Vec<Value> {
	let replies = session(&[request(1, "tools/list", Some(json!({})))], options).await;
	let reply = &replies[0];
	assert!(reply["result"].get("nextCursor").is_none());
	let tools = reply["result"]["tools"].as_array().unwrap().clone();
	for tool in &tools {
		assert_eq!(tool["inputSchema"]["type"], "object");
		assert_eq!(tool["inputSchema"]["additionalProperties"], false);
	}
	tools
}

fn thread_options() -> Options {
	Options { env: Some(thread_env()), ..Options::default() }
}

fn boss_options() -> Options {
	Options { boss: true, ..Options::default() }
}

fn tool_names(tools: &[Value]) -> Vec<&str> {
	tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect()
}

fn find<'a>(tools: &'a [Value], name: &str) -> &'a Value {
	tools.iter().find(|tool| tool["name"] == name).unwrap_or_else(|| panic!("no tool {name}"))
}

#[tokio::test]
async fn tools_list_history_search_everywhere_report_result_and_check_only_in_a_thread_with_the_layer_on_the_bosss_tools_only_with_boss()
 {
	let thread = listed(thread_options()).await;
	assert_eq!(find(&thread, "history_search")["inputSchema"]["required"], json!(["query"]));
	assert!(find(&thread, "history_search")["inputSchema"]["properties"].get("project").is_none());
	let boss = listed(boss_options()).await;
	assert_eq!(find(&boss, "history_search")["inputSchema"]["required"], json!(["project", "query"]));
	assert_eq!(tool_names(&thread), ["history_search", "report_result", "check"]);
	// His other omp sessions and the project panes load the same server (0577), and the layer can be off (0576).
	assert_eq!(tool_names(&listed(Options::default()).await), ["history_search"]);
	let layer_off = HashMap::from([
		("JOCASTA_THREAD".to_owned(), "t-1".to_owned()),
		("JOCASTA_LAYER".to_owned(), "off".to_owned()),
	]);
	assert_eq!(tool_names(&listed(Options { env: Some(layer_off), ..Options::default() }).await), ["history_search"]);
	let boss_in_thread = Options { boss: true, env: Some(thread_env()), ..Options::default() };
	assert!(!tool_names(&listed(boss_in_thread).await).contains(&"check"));
	// report_result's schema (0539): the outcome, a summary of at most 200 characters, the lists.
	let report = &find(&thread, "report_result")["inputSchema"];
	assert_eq!(
		report["required"],
		json!(["outcome", "summary", "files", "tests", "decisions", "questions", "followUps"])
	);
	assert_eq!(report["properties"]["summary"]["maxLength"], 200);
	let outcomes: Vec<&Value> =
		report["properties"]["outcome"]["anyOf"].as_array().unwrap().iter().map(|entry| &entry["const"]).collect();
	assert_eq!(outcomes, [&json!("done"), &json!("blocked"), &json!("failed")]);
	let start = &find(&boss, "thread_start")["inputSchema"];
	assert_eq!(start["properties"]["done_when"]["type"], "array");
	assert_eq!(start["properties"]["done_when"]["items"], json!({ "type": "string" }));
	assert_eq!(start["properties"]["heavy"]["type"], "boolean");
	assert_eq!(start["required"], json!(["project", "title", "brief"]));
	assert_eq!(
		tool_names(&boss),
		[
			"history_search",
			"projects",
			"thread_start",
			"thread_status",
			"thread_send",
			"thread_stop",
			"thread_finish",
			"show",
			"today",
			"note",
			"remind",
			"inbox_done",
			"worklog",
			"lesson",
			"big_picture_review",
			"worktree_remove",
		]
	);
}

#[tokio::test]
async fn outside_a_thread_report_result_and_check_are_unknown_tools() {
	let lines = [call(1, "check", json!({})), call(2, "report_result", json!({}))];
	let replies = session(&lines, Options::default()).await;
	let codes: Vec<&Value> = replies.iter().map(|reply| &reply["error"]["code"]).collect();
	assert_eq!(codes, [&json!(-32602), &json!(-32602)]);
}

fn sorted(fake: &Fake) -> Vec<(String, Value, Option<String>)> {
	let mut calls = fake.calls.lock().unwrap().clone();
	calls.sort_by(|a, b| a.0.cmp(&b.0));
	calls
}

#[tokio::test]
async fn tools_call_posts_args_pane_key_and_boss_key_to_api_mcp_tool_with_the_bearer_token() {
	let fake = fake_service(|tool| {
		let body = if tool == "thread_stop" {
			json!({ "text": "No thread named x.", "isError": true })
		} else {
			json!({ "text": format!("ok {tool}") })
		};
		(200, body.to_string())
	})
	.await;
	let mut lines = handshake();
	lines.push(call(2, "history_search", json!({ "project": "demo", "query": "why sqlite", "limit": 5 })));
	lines.push(call(3, "thread_start", json!({ "project": "demo", "title": "Readme", "brief": "Add a line" })));
	lines.push(call(4, "thread_stop", json!({ "thread": "x" })));
	lines.push(request(5, "tools/call", Some(json!({ "name": "today" }))));
	let options = Options {
		boss: true,
		service: at(&fake),
		pane_key: Some("pk-7".to_owned()),
		boss_key: Some("key-1".to_owned()),
		..Options::default()
	};
	let replies = raw_session(&lines, options).await;
	let by_id: HashMap<i64, &Value> =
		replies.iter().map(|reply| (reply["id"].as_i64().unwrap(), &reply["result"])).collect();
	let text =
		|text: &str, is_error: bool| json!({ "content": [{ "type": "text", "text": text }], "isError": is_error });
	assert_eq!(by_id[&2], &text("ok history_search", false));
	assert_eq!(by_id[&3], &text("ok thread_start", false));
	assert_eq!(by_id[&4], &text("No thread named x.", true));
	assert_eq!(by_id[&5], &text("ok today", false));
	let from = |args: Value| json!({ "args": args, "paneKey": "pk-7", "bossKey": "key-1" });
	let bearer = Some("Bearer tok-1".to_owned());
	assert_eq!(
		sorted(&fake),
		vec![
			(
				"history_search".to_owned(),
				from(json!({ "project": "demo", "query": "why sqlite", "limit": 5 })),
				bearer.clone()
			),
			(
				"thread_start".to_owned(),
				from(json!({ "project": "demo", "title": "Readme", "brief": "Add a line" })),
				bearer.clone()
			),
			("thread_stop".to_owned(), from(json!({ "thread": "x" })), bearer.clone()),
			("today".to_owned(), from(json!({})), bearer),
		]
	);
}

#[tokio::test]
async fn a_thread_panes_call_sends_its_pane_key_and_no_boss_key() {
	let fake = fake_service(|_| (200, json!({ "text": "ok" }).to_string())).await;
	let options = Options { service: at(&fake), pane_key: Some("pk-9".to_owned()), ..Options::default() };
	session(&[call(1, "history_search", json!({ "query": "why" }))], options).await;
	let bodies: Vec<Value> = fake.calls.lock().unwrap().iter().map(|call| call.1.clone()).collect();
	assert_eq!(bodies, [json!({ "args": { "query": "why" }, "paneKey": "pk-9" })]);
}

#[tokio::test]
async fn a_service_reply_without_a_result_or_an_http_error_is_a_tool_error() {
	let fake = fake_service(|tool| {
		if tool == "note" { (401, "nope".to_owned()) } else { (403, json!({ "text": "denied" }).to_string()) }
	})
	.await;
	let lines = [call(1, "note", json!({ "text": "hi", "project": "demo" })), call(2, "today", json!({}))];
	let replies = session(&lines, Options { boss: true, service: at(&fake), ..Options::default() }).await;
	let by_id: HashMap<i64, &Value> =
		replies.iter().map(|reply| (reply["id"].as_i64().unwrap(), &reply["result"])).collect();
	assert_eq!(by_id[&1], &tool_error("note: Jocasta's service answered HTTP 401 without a result"));
	assert_eq!(by_id[&2], &tool_error("denied"));
}

#[tokio::test]
async fn no_service_every_tool_answers_the_not_running_text() {
	assert_eq!(NOT_RUNNING, "Jocasta isn't running; start it with jocasta.");
	let lines = [call(1, "history_search", json!({ "project": "demo", "query": "x" })), call(2, "projects", json!({}))];
	let replies = session(&lines, boss_options()).await;
	let results: Vec<&Value> = replies.iter().map(|reply| &reply["result"]).collect();
	assert_eq!(results, [&not_running(), &not_running()]);
}

#[tokio::test]
async fn a_refused_connection_is_the_not_running_text_too() {
	let port = crate::shared::fsx::free_port().unwrap();
	let service: ServiceLookup = Arc::new(move || Some(RunningService { port, token: "t".to_owned() }));
	let replies = session(
		&[call(1, "history_search", json!({ "query": "x" }))],
		Options { service: Some(service), ..Options::default() },
	)
	.await;
	assert_eq!(replies[0]["result"], not_running());
}

#[tokio::test]
async fn by_default_a_call_goes_to_the_service_the_data_dir_file_names_while_its_pid_lives_frontier_json_is_still_read()
{
	let _j = temp_jocasta();
	let fake = fake_service(|_| (200, json!({ "text": "ok" }).to_string())).await;
	let ask = || async {
		let options = Options { service: None, ..Options::default() };
		let replies = session(&[call(1, "history_search", json!({ "query": "x" }))], options).await;
		replies[0]["result"]["content"][0]["text"].as_str().unwrap().to_owned()
	};
	let file = |pid: u32| json!({ "port": fake.address.port, "token": fake.address.token, "pid": pid }).to_string();
	std::fs::create_dir_all(data_dir()).unwrap();
	assert_eq!(ask().await, NOT_RUNNING);
	std::fs::write(service_file_path(None), file(std::process::id())).unwrap();
	assert_eq!(ask().await, "ok");
	let auths: Vec<Option<String>> = fake.calls.lock().unwrap().iter().map(|call| call.2.clone()).collect();
	assert_eq!(auths, [Some("Bearer tok-1".to_owned())]);
	// A pid far above any real one on the test machine.
	std::fs::write(service_file_path(None), file(2_u32.pow(31) - 2)).unwrap();
	assert_eq!(ask().await, NOT_RUNNING);
	// The old name, from a service an older build started, is read when the new one is missing; the new one wins.
	std::fs::write(data_dir().join("frontier.json"), file(std::process::id())).unwrap();
	assert_eq!(ask().await, NOT_RUNNING);
	std::fs::remove_file(service_file_path(None)).unwrap();
	assert_eq!(ask().await, "ok");
}

#[tokio::test]
async fn malformed_input_and_bad_arguments_after_the_handshake_keep_the_server_serving() {
	let lines: Vec<String> = [
		"{not json".to_owned(),
		"[1,2]".to_owned(),
		"42".to_owned(),
		json!({ "jsonrpc": "2.0", "id": 1 }).to_string(),
		json!({ "jsonrpc": "2.0", "id": { "x": 1 }, "method": "ping" }).to_string(),
		json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
		String::new(),
		request(3, "resources/list", None),
		request(4, "tools/call", Some(json!({ "name": "nope" }))),
		call(5, "history_search", json!({})),
		call(6, "history_search", json!({ "query": "x", "limit": 0 })),
		call(7, "history_search", json!({ "query": "x", "extra": 1 })),
		call(8, "history_search", json!("x")),
		call(9, "thread_start", json!({ "project": "demo", "title": "t", "brief": "b" })),
		request(10, "ping", None),
	]
	.into();
	let replies = session(&lines, Options::default()).await;
	// rmcp's lifecycle differs from the hand-written server's here (see the port's Differences): text that isn't JSON
	// and a response to no request get no answer; well-formed JSON that is no request gets one "Invalid request"
	// (-32600) without an id.
	let invalid = json!({ "jsonrpc": "2.0", "error": { "code": -32600, "message": "Invalid request" } });
	assert_eq!(replies[..4], [invalid.clone(), invalid.clone(), invalid.clone(), invalid]);
	let by_id = |id: i64| replies.iter().find(|reply| reply["id"] == id).unwrap_or_else(|| panic!("no reply to {id}"));
	assert_eq!(replies.len(), 12);
	// A method this server doesn't have: rmcp's -32601 with the method as its message.
	assert_eq!(by_id(3)["error"], json!({ "code": -32601, "message": "resources/list" }));
	assert_eq!(by_id(4)["error"]["code"], -32602);
	assert_eq!(by_id(4)["error"]["message"], "Unknown tool: nope");
	assert_eq!(by_id(5)["result"], tool_error("history_search: missing query"));
	assert_eq!(by_id(6)["result"], tool_error("history_search: limit must be an integer in range"));
	assert_eq!(by_id(7)["result"], tool_error("history_search: unknown argument extra"));
	// Arguments that aren't an object don't parse as a `tools/call` at all.
	assert_eq!(by_id(8)["error"]["code"], -32601);
	// `thread_start` is the boss's: unknown outside `--boss`.
	assert_eq!(by_id(9)["error"], json!({ "code": -32602, "message": "Unknown tool: thread_start" }));
	assert_eq!(by_id(10)["result"], json!({}));
	assert!(replies.iter().all(|reply| reply["jsonrpc"] == "2.0"));
}

#[tokio::test]
async fn before_the_handshake_tools_list_is_refused_and_ping_answers_and_a_second_initialize_is_answered_again() {
	let replies = raw_session(&[request(1, "tools/list", None), request(2, "ping", None)], Options::default()).await;
	assert_eq!(replies[0]["id"], 1);
	assert_eq!(replies[0]["error"]["code"], -32602);
	assert!(replies[0]["error"]["message"].as_str().unwrap().starts_with("request _meta is missing"));
	assert_eq!(replies[1]["result"], json!({}));
	let mut lines = handshake();
	lines.push(request(2, "initialize", Some(initialize_params(MCP_PROTOCOL_VERSION))));
	lines.push(request(3, "tools/list", None));
	let replies = raw_session(&lines, Options::default()).await;
	assert_eq!(replies.len(), 3);
	assert_eq!(replies[0]["result"], replies[1]["result"]);
	assert_eq!(names(&replies[2]), ["history_search"]);
}

#[tokio::test]
async fn null_arguments_are_none_given() {
	let mut lines = handshake();
	lines.push(request(2, "tools/call", Some(json!({ "name": "history_search", "arguments": null }))));
	let replies = raw_session(&lines, Options::default()).await;
	assert_eq!(replies[1]["result"], tool_error("history_search: missing query"));
}

fn text_of(reply: &Value) -> &str {
	reply["result"]["content"][0]["text"].as_str().unwrap()
}

#[tokio::test]
async fn report_result_and_thread_starts_done_when_and_heavy_are_checked_before_the_service_is_asked() {
	let report = json!({
		"outcome": "done",
		"summary": "Login retries back off",
		"files": ["src/auth.ts"],
		"tests": [{ "command": "bun test", "passed": true }],
		"decisions": [{ "question": "Backoff cap", "choice": "30 s", "why": "the API's limit" }],
		"questions": [],
		"followUps": [],
	});
	let with = |key: &str, value: Option<Value>| {
		let mut changed = report.clone();
		match value {
			Some(value) => changed[key] = value,
			None => {
				changed.as_object_mut().unwrap().remove(key);
			}
		}
		changed
	};
	let lines = [
		call(1, "report_result", with("outcome", Some(json!("maybe")))),
		call(2, "report_result", with("summary", Some(json!("x".repeat(201))))),
		call(3, "report_result", with("extra", Some(json!(1)))),
		call(4, "report_result", with("files", None)),
		call(5, "report_result", report.clone()),
	];
	let replies = session(&lines, thread_options()).await;
	assert!(text_of(&replies[0]).starts_with("report_result: outcome "));
	assert!(text_of(&replies[1]).starts_with("report_result: summary "));
	assert_eq!(text_of(&replies[2]), "report_result: unknown field extra in the report");
	assert!(text_of(&replies[3]).starts_with("report_result: "));
	// A valid report goes on to the service; none runs here.
	assert_eq!(text_of(&replies[4]), NOT_RUNNING);
	let start = |id: i64, extra: Value| {
		let mut arguments = json!({ "project": "demo", "title": "t", "brief": "b" });
		arguments.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
		call(id, "thread_start", arguments)
	};
	let lines = [
		start(6, json!({ "done_when": ["bun test", ""] })),
		start(7, json!({ "heavy": "yes" })),
		start(8, json!({ "heavy": true })),
	];
	let boss = session(&lines, boss_options()).await;
	assert_eq!(text_of(&boss[0]), "thread_start: done_when must be a list of non-empty strings");
	assert_eq!(text_of(&boss[1]), "thread_start: heavy must be true or false");
	// A boolean heavy goes on to the service; none runs here.
	assert_eq!(text_of(&boss[2]), NOT_RUNNING);
}

#[tokio::test]
async fn a_stream_works_as_input_split_across_chunks_with_crlf_line_ends() {
	let (mut writer, reader) = tokio::io::duplex(16);
	let (output, mut from_server) = tokio::io::duplex(1 << 20);
	let opts = options_of(Options::default());
	let done = tokio::spawn(run_mcp_server(opts, reader, output));
	let mut lines = handshake();
	lines.push(request(2, "ping", None));
	lines.push(request(3, "tools/list", None));
	let text: String = lines.iter().map(|line| format!("{line}\r\n")).collect();
	let bytes = text.into_bytes();
	let feed = tokio::spawn(async move {
		writer.write_all(&bytes[..10]).await.unwrap();
		writer.write_all(&bytes[10..50]).await.unwrap();
		writer.write_all(&bytes[50..]).await.unwrap();
	});
	let mut answered = String::new();
	from_server.read_to_string(&mut answered).await.unwrap();
	feed.await.unwrap();
	done.await.unwrap();
	let mut ids: Vec<i64> =
		answered.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["id"].as_i64().unwrap()).collect();
	ids.sort_unstable();
	assert_eq!(ids, [1, 2, 3]);
}

#[tokio::test]
async fn jocasta_mcp_answers_initialize_and_its_tools_say_jocasta_isnt_running_without_the_service() {
	// `test/entry.test.ts`'s case, in process: the child's exit code and its stdout-only output wait for `entry.rs`.
	let _j = temp_jocasta();
	let mut lines = handshake();
	lines.push(call(2, "history_search", json!({ "project": "demo", "query": "x" })));
	let replies = raw_session(&lines, Options { boss: true, service: None, ..Options::default() }).await;
	assert_eq!(replies.len(), 2);
	assert_eq!(
		replies[0]["result"],
		json!({
			"protocolVersion": MCP_PROTOCOL_VERSION,
			"capabilities": { "tools": { "listChanged": false } },
			"serverInfo": { "name": "jocasta", "version": env!("CARGO_PKG_VERSION") },
		})
	);
	assert_eq!(replies[1]["id"], 2);
	assert_eq!(replies[1]["result"], not_running());
}

/// `initialize` and `tools/list` of the TypeScript build (`bun src/entry.ts mcp` with `JOCASTA_THREAD` unset, set to
/// `t-1`, and `mcp --boss`, run in a temp data folder), captured as `JSON.stringify` printed them.
const CAPTURED: &str = include_str!("tools_list_fixture.json");

#[tokio::test]
async fn initialize_and_tools_list_results_match_what_the_typescript_build_sent_in_every_mode_key_for_key_in_order() {
	let fixture: Value = serde_json::from_str(CAPTURED).unwrap();
	let modes = [("plain", Options::default()), ("thread", thread_options()), ("boss", boss_options())];
	for (mode, options) in modes {
		let mut lines = handshake();
		lines.push(request(2, "tools/list", Some(json!({}))));
		let mut replies = raw_session(&lines, options).await;
		replies.sort_by_key(|reply| reply["id"].as_i64());
		// The results are compared as text: serde_json keeps key order, so a reordered key fails here too. The
		// envelope (`jsonrpc`, `id`, `result`) is rmcp's.
		let result = |reply: &Value| reply["result"].to_string();
		let was = |name: &str| fixture[mode][name]["result"].to_string();
		assert_eq!(result(&replies[0]), was("initialize"), "{mode} initialize");
		assert_eq!(result(&replies[1]), was("tools_list"), "{mode} tools/list");
		assert!(!names(&replies[1]).is_empty());
	}
}
