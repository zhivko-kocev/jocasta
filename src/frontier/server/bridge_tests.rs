//! The bridge's Phase 1 checks (0675, 0681, 0684) on real pipes: a hello, the roles and the tools' refusal, a prompt
//! before the hello, the turn-end check, the degrade by capabilities, sealing, `cancel`, `compact`, the line cap and the
//! events. A stand-in service answers where the service's own behavior isn't the point; the refusal of the boss's tools
//! and the abort's degrade run on the real server.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};

use super::*;
use crate::frontier::server::helpers::{
	TestServer, ThreadOptions, call, demo_project, start_thread, until, with_server,
};
use crate::frontier::server::server::ServerOptions;
use crate::shared::bridge_wire::{CliPeer, connect, line_of};
use crate::shared::fsx::free_port;
use crate::shared::testing::temp_jocasta;

const SECRET: &str = "abcdefgh12345678";
const TOKEN: &str = "cli-token-0123456789";
const KEY: &str = "pane-key-1";
const PANE: &str = "p-1";
const LONG: Duration = Duration::from_secs(10);

/// Sets a flag when it is dropped: a call's task that `cancel` ended.
struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
	fn drop(&mut self) {
		self.0.store(true, Ordering::SeqCst);
	}
}

/// A stand-in service: one thread pane under [`KEY`], tools that echo, leak a secret or hang, a stop that names its key.
#[derive(Default)]
struct Stand {
	events: Mutex<Vec<PaneEvent>>,
	started: Arc<AtomicBool>,
	ended: Arc<AtomicBool>,
}

impl BridgeService for Stand {
	fn resolve(&self, key: &str) -> Option<Resolved> {
		(key == KEY).then(|| Resolved {
			pane_id: PANE.to_owned(),
			role: Role::Thread,
			caller: Caller { pane_key: Some(KEY.to_owned()), boss_key: None },
		})
	}

	fn tools(&self, _role: Role) -> Vec<Value> {
		vec![json!({ "name": "probe", "label": "Probe", "description": "d", "parameters": { "type": "object" } })]
	}

	fn call(&self, who: Resolved, tool: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move {
			match tool.as_str() {
				"leak" => ToolReply { text: format!("found {SECRET} in it"), is_error: None },
				"leak_error" => ToolReply { text: format!("failed with {SECRET}"), is_error: Some(true) },
				"hang" => {
					let _ended = Dropped(self.ended.clone());
					self.started.store(true, Ordering::SeqCst);
					std::future::pending::<()>().await;
					unreachable!("a hanging call never returns")
				}
				_ => {
					ToolReply { text: format!("{} {tool} {}", who.role.as_str(), Value::Object(args)), is_error: None }
				}
			}
		})
	}

	fn cli(&self, command: String, _args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move { ToolReply { text: format!("cli {command}"), is_error: None } })
	}

	fn push(&self, _request: PushRequest) -> BoxFuture<'_, PushAnswer> {
		Box::pin(async { PushAnswer { allow: true, reason: None } })
	}

	fn stop(&self, pane_key: String, _signal: AbortSignal) -> BoxFuture<'_, String> {
		Box::pin(async move { format!("gate for {pane_key}: {SECRET}") })
	}

	fn event(&self, _who: &Resolved, event: &PaneEvent, _agent: &PaneAgent) {
		self.events.lock().unwrap().push(event.clone());
	}
}

/// A listening bridge over the stand-in service, stopped when dropped.
struct Rig {
	bridge: Bridge,
	stand: Arc<Stand>,
	address: String,
	port: u16,
	_service: Arc<dyn BridgeService>,
}

impl Rig {
	fn new() -> Self {
		let stand = Arc::new(Stand::default());
		let service: Arc<dyn BridgeService> = stand.clone();
		let bridge = Bridge::new();
		bridge.set_secrets(Secrets::new([SECRET.to_owned()]));
		let port = free_port().unwrap();
		bridge.listen(port, TOKEN, &service).unwrap();
		let address = bridge.address().unwrap();
		Self { bridge, stand, address, port, _service: service }
	}

	async fn raw(&self) -> Raw {
		Raw::open(&self.address).await
	}

	/// A pane's omp with the hello it sends, welcomed; `caps` as its hello says.
	async fn pane(&self, caps: &[&str]) -> Raw {
		let mut raw = self.raw().await;
		raw.send(&pane_hello(KEY, caps)).await;
		let welcome = raw.line().await;
		assert_eq!(welcome["t"], "welcome");
		raw
	}
}

impl Drop for Rig {
	fn drop(&mut self) {
		self.bridge.stop();
	}
}

fn pane_hello(key: &str, caps: &[&str]) -> Value {
	json!({
		"t": "hello", "v": VERSION, "key": key, "pid": 4242, "session": "session.jsonl",
		"harness": "omp", "harness_version": "15.0.0", "caps": caps,
	})
}

#[derive(Debug)]
enum Next {
	Line(Value),
	Closed,
	Quiet,
}

/// A raw peer of the bridge, as the extension or the CLI is.
struct Raw {
	read: LineReader<ReadHalf<Stream>>,
	write: WriteHalf<Stream>,
}

impl Raw {
	async fn open(address: &str) -> Self {
		let stream = connect(address).await.expect("the bridge listens");
		let (read, write) = tokio::io::split(stream);
		Self { read: LineReader::new(read, MAX_LINE), write }
	}

	async fn send(&mut self, message: &Value) {
		self.write.write_all(line_of(message).as_bytes()).await.unwrap();
		self.write.flush().await.unwrap();
	}

	async fn next(&mut self, wait: Duration) -> Next {
		match tokio::time::timeout(wait, self.read.next_line()).await {
			Err(_) => Next::Quiet,
			Ok(Ok(Some(line))) => Next::Line(serde_json::from_str(&line).expect("a JSON line")),
			Ok(Ok(None) | Err(_)) => Next::Closed,
		}
	}

	async fn line(&mut self) -> Value {
		match self.next(LONG).await {
			Next::Line(line) => line,
			other => panic!("wanted a line, got {other:?}"),
		}
	}

	/// The connection ended (nothing more comes).
	async fn closed(&mut self) -> bool {
		matches!(self.next(LONG).await, Next::Closed)
	}

	/// Nothing arrives for a moment.
	async fn quiet(&mut self) -> bool {
		matches!(self.next(Duration::from_millis(200)).await, Next::Quiet)
	}
}

fn id_of(message: &Value) -> Value {
	message["id"].clone()
}

// ---- (a) the hello ----

#[tokio::test(flavor = "multi_thread")]
async fn a_hello_with_an_unknown_key_a_wrong_version_or_no_hello_closes_the_connection() {
	let rig = Rig::new();
	for hello in [
		pane_hello("not-a-pane-key", &["prompt"]),
		json!({ "t": "hello", "v": VERSION + 1, "key": KEY }),
		json!({ "t": "hello", "v": VERSION }),
		json!({ "t": "call", "id": "1", "tool": "probe", "args": {} }),
	] {
		let mut raw = rig.raw().await;
		raw.send(&hello).await;
		assert!(raw.closed().await, "{hello} was welcomed");
	}
	assert!(rig.bridge.panes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cli_hello_with_the_wrong_token_closes_and_the_right_token_is_welcomed() {
	let rig = Rig::new();
	for token in ["", "wrong-token-0123456789", &TOKEN[..TOKEN.len() - 1]] {
		let mut raw = rig.raw().await;
		raw.send(&json!({ "t": "hello", "v": VERSION, "token": token, "role": "cli" })).await;
		assert!(raw.closed().await, "the token {token:?} was welcomed");
		assert!(CliPeer::open(rig.port, token).await.is_err());
	}
	// A pane's key is no CLI token and the CLI token is no pane key.
	let mut raw = rig.raw().await;
	raw.send(&json!({ "t": "hello", "v": VERSION, "token": KEY, "role": "cli" })).await;
	assert!(raw.closed().await);
	let mut raw = rig.raw().await;
	raw.send(&pane_hello(TOKEN, &["prompt"])).await;
	assert!(raw.closed().await);

	let mut raw = rig.raw().await;
	raw.send(&json!({ "t": "hello", "v": VERSION, "token": TOKEN, "role": "cli" })).await;
	let welcome = raw.line().await;
	assert_eq!((welcome["t"].as_str(), welcome["role"].as_str()), (Some("welcome"), Some("cli")));
	let mut cli = CliPeer::open(rig.port, TOKEN).await.unwrap();
	let answer = cli.request(json!({ "t": "cli", "command": "ping", "args": {} })).await.unwrap();
	assert_eq!((answer["ok"].as_bool(), answer["text"].as_str()), (Some(true), Some("cli ping")));
	assert!(rig.bridge.panes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pane_with_its_key_is_welcomed_with_its_role_and_tools_and_is_connected() {
	let rig = Rig::new();
	let mut raw = rig.raw().await;
	raw.send(&pane_hello(KEY, &["prompt"])).await;
	let welcome = raw.line().await;
	assert_eq!((welcome["t"].as_str(), welcome["role"].as_str()), (Some("welcome"), Some("thread")));
	assert_eq!(welcome["tools"][0]["name"], "probe");
	assert!(rig.bridge.connected(PANE));
	assert_eq!(rig.bridge.pid(PANE), Some(4242));
	assert_eq!(rig.bridge.session(PANE).as_deref(), Some("session.jsonl"));
	assert_eq!(rig.bridge.version(PANE).as_deref(), Some("15.0.0"));
	// A call from the pane is answered with its role.
	raw.send(&json!({ "t": "call", "id": "1", "tool": "probe", "args": { "x": 1 } })).await;
	let answer = raw.line().await;
	assert_eq!((answer["ok"].as_bool(), answer["text"].as_str()), (Some(true), Some("thread probe {\"x\":1}")));
	drop(raw);
	assert!(rig.bridge.gone(PANE, 10_000).await);
	until(|| !rig.bridge.connected(PANE)).await;
}

// ---- (b) the boss's tools are refused to a thread's pane, on the real service ----

/// The key `pane_id` was started with, and a thread's peer on the server's own bridge.
async fn peer_of(server: &TestServer, pane_id: &str, caps: &[&str]) -> Raw {
	let key = server
		.herdr
		.env_of(pane_id)
		.and_then(|env| env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key))
		.expect("the pane has a key");
	let mut raw = Raw::open(&server.bridge.address().expect("the bridge listens")).await;
	raw.send(&pane_hello(&key, caps)).await;
	assert_eq!(raw.line().await["t"], "welcome");
	until(|| server.bridge.connected(pane_id)).await;
	raw
}

fn started_options() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_peer_calling_thread_start_gets_ok_false_and_no_thread_starts() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(started_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let mut raw = peer_of(&server, &started.pane_id, &["prompt"]).await;
		raw.send(&json!({
			"t": "call", "id": "7", "tool": "thread_start",
			"args": { "project": "demo", "title": "Another", "brief": "Do it." },
		}))
		.await;
		let answer = raw.line().await;
		assert_eq!((id_of(&answer), answer["ok"].as_bool()), (json!("7"), Some(false)));
		assert!(answer["error"].as_str().unwrap().contains("boss's tool"), "{answer}");
		assert_eq!(server.herdr.of("createWorktree").len(), 1);
		// The thread's own tool still answers it.
		raw.send(&json!({ "t": "call", "id": "8", "tool": "history_search", "args": { "query": "nothing" } })).await;
		assert_eq!(raw.line().await["ok"].as_bool(), Some(true));
	})
	.await;
}

// ---- (c) a prompt before the hello ----

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_before_the_hello_waits_and_is_delivered_after_it_and_the_acks_resolve_it() {
	let rig = Rig::new();
	let (bridge, text) = (rig.bridge.clone(), rig.bridge.seal("go on"));
	let sent = tokio::spawn(async move { bridge.prompt(PANE, text, Deliver::Steer).await });
	tokio::time::sleep(Duration::from_millis(150)).await;
	assert!(!sent.is_finished(), "the prompt returned with no omp connected");

	let mut raw = rig.pane(&["prompt"]).await;
	let prompt = raw.line().await;
	assert_eq!(
		(prompt["t"].as_str(), prompt["text"].as_str(), prompt["deliver"].as_str()),
		(Some("prompt"), Some("go on"), Some("steer"))
	);
	assert!(!sent.is_finished(), "the prompt returned before omp acked it");
	raw.send(&json!({ "t": "ack", "id": id_of(&prompt) })).await;
	sent.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_nacked_by_omp_fails_with_its_text_and_one_to_an_omp_without_the_prompt_cap_fails_at_once() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	let (bridge, text) = (rig.bridge.clone(), rig.bridge.seal("hello"));
	let sent = tokio::spawn(async move { bridge.prompt(PANE, text, Deliver::NextTurn).await });
	let prompt = raw.line().await;
	assert_eq!(prompt["deliver"], "nextTurn");
	raw.send(&json!({ "t": "nack", "id": id_of(&prompt), "error": "omp is closing" })).await;
	assert_eq!(sent.await.unwrap().unwrap_err().to_string(), "omp is closing");

	let rig = Rig::new();
	let mut raw = rig.pane(&[]).await;
	let error = rig.bridge.prompt(PANE, rig.bridge.seal("hello"), Deliver::Steer).await.unwrap_err();
	assert!(error.to_string().contains("can't take prompts from Jocasta"), "{error}");
	assert!(raw.quiet().await, "a prompt line was sent to an omp that can't take it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_with_no_omp_connecting_fails_after_the_connect_wait_naming_the_pane_and_the_wait() {
	let rig = Rig::new();
	rig.bridge.set_connect_wait(Duration::from_secs(1));
	let started = std::time::Instant::now();
	let error = rig.bridge.prompt("p-9", rig.bridge.seal("hello"), Deliver::Steer).await.unwrap_err();
	assert_eq!(error.to_string(), "omp in p-9 didn't connect within 1 s");
	assert!(started.elapsed() >= Duration::from_millis(900), "gave up after {:?}", started.elapsed());
	// The message names the wait that was configured (rounded to whole seconds).
	rig.bridge.set_connect_wait(Duration::from_millis(30));
	let error = rig.bridge.prompt("p-9", rig.bridge.seal("hello"), Deliver::Steer).await.unwrap_err();
	assert_eq!(error.to_string(), "omp in p-9 didn't connect within 0 s");
	assert_eq!(CONNECT_WAIT_MS, 30_000);
}

// ---- (d) the turn-end check ----

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_returns_the_gates_text_sealed_in_a_stop_result() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	raw.send(&json!({ "t": "stop", "id": "s1" })).await;
	let result = raw.line().await;
	assert_eq!((result["t"].as_str(), id_of(&result)), (Some("stop_result"), json!("s1")));
	assert_eq!(result["context"], format!("gate for {KEY}: [secret]"));
}

// ---- (e) the degrade by capabilities ----

/// A pane's omp whose `caps` event says it can do `caps` (and prompt).
async fn pane_with(rig: &Rig, caps: &[&str]) -> Raw {
	let mut raw = rig.pane(&["prompt"]).await;
	raw.send(&json!({ "t": "event", "name": "caps", "caps": caps })).await;
	raw
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_whose_caps_lack_shutdown_or_abort_or_compact_degrades_and_sends_nothing() {
	let rig = Rig::new();
	let mut raw = pane_with(&rig, &["usage"]).await;
	until(|| !rig.bridge.can(PANE, "shutdown")).await;
	assert!(!rig.bridge.shutdown(PANE).unwrap());
	assert!(!rig.bridge.abort(PANE).unwrap());
	assert!(!rig.bridge.compact(PANE, rig.bridge.seal("guidance")).await.unwrap());
	assert!(rig.bridge.can(PANE, "usage"));
	assert!(raw.quiet().await, "a control line was sent to an omp that can't take it");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_with_abort_and_shutdown_gets_them_as_lines() {
	let rig = Rig::new();
	let mut raw = pane_with(&rig, &["abort", "shutdown"]).await;
	until(|| rig.bridge.can(PANE, "abort")).await;
	assert!(rig.bridge.abort(PANE).unwrap());
	assert_eq!(raw.line().await["t"], "abort");
	assert!(rig.bridge.shutdown(PANE).unwrap());
	assert_eq!(raw.line().await["t"], "shutdown");
	assert!(rig.bridge.abort("p-404").is_err(), "an unconnected pane has no abort");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abort_of_an_omp_without_abort_ends_its_tree_and_relaunches_the_pane() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(started_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let mut raw = peer_of(&server, &started.pane_id, &["prompt"]).await;
		raw.send(&json!({ "t": "event", "name": "caps", "caps": ["usage", "shutdown"] })).await;
		until(|| !server.bridge.can(&started.pane_id, "abort")).await;
		let stopped = call(&server, "thread_stop", &json!({ "thread": "Readme line" })).await;
		assert_eq!(stopped.text, "Stopped demo/Readme line's run.");
		assert_eq!(server.herdr.of("respawn").len(), 1);
		assert_eq!(server.herdr.of("respawn")[0][0], json!(started.pane_id));
		assert!(raw.quiet().await, "an abort line was sent to an omp that can't abort");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abort_of_an_omp_with_abort_sends_the_line_and_relaunches_nothing() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(started_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let mut raw = peer_of(&server, &started.pane_id, &["prompt"]).await;
		raw.send(&json!({ "t": "event", "name": "caps", "caps": ["abort"] })).await;
		let stopped = call(&server, "thread_stop", &json!({ "thread": "Readme line" })).await;
		assert_eq!(stopped.text, "Stopped demo/Readme line's run.");
		assert_eq!(raw.line().await["t"], "abort");
		assert!(server.herdr.of("respawn").is_empty());
	})
	.await;
}

// ---- (f) sealing ----

#[test]
fn a_stored_secret_in_text_is_blanked_and_one_under_eight_characters_is_left() {
	let secrets = Secrets::new([SECRET.to_owned(), "short".to_owned(), "1234567".to_owned()]);
	assert_eq!(ModelText::seal("token abcdefgh12345678 here", &secrets).as_str(), "token [secret] here");
	assert_eq!(
		ModelText::seal("twice abcdefgh12345678 abcdefgh12345678", &secrets).as_str(),
		"twice [secret] [secret]"
	);
	assert_eq!(ModelText::seal("a short 1234567 word", &secrets).as_str(), "a short 1234567 word");
	assert_eq!(ModelText::seal("nothing to hide", &Secrets::default()).as_str(), "nothing to hide");
}

#[test]
fn the_longest_secret_is_blanked_first_and_a_secret_of_exactly_eight_characters_counts() {
	let secrets = Secrets::new(["abcdefgh".to_owned(), SECRET.to_owned(), "abcdefgh".to_owned()]);
	assert_eq!(ModelText::seal("x abcdefgh12345678 y abcdefgh z", &secrets).as_str(), "x [secret] y [secret] z");
}

#[test]
fn a_bridges_seal_uses_the_secrets_it_was_given() {
	let bridge = Bridge::new();
	assert_eq!(bridge.seal(SECRET).as_str(), SECRET);
	bridge.set_secrets(Secrets::new([SECRET.to_owned()]));
	assert_eq!(bridge.seal(&format!("<{SECRET}>")).as_str(), "<[secret]>");
}

// ---- (g) cancel, compact, sealed results, the line cap and the events ----

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_ends_a_running_call_and_nothing_answers_it() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	raw.send(&json!({ "t": "call", "id": "c1", "tool": "hang", "args": {} })).await;
	until(|| rig.stand.started.load(Ordering::SeqCst)).await;
	assert!(!rig.stand.ended.load(Ordering::SeqCst));
	raw.send(&json!({ "t": "cancel", "id": "c1" })).await;
	until(|| rig.stand.ended.load(Ordering::SeqCst)).await;
	assert!(raw.quiet().await, "a cancelled call was answered");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_ends_ends_its_running_calls() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	raw.send(&json!({ "t": "call", "id": "c1", "tool": "hang", "args": {} })).await;
	until(|| rig.stand.started.load(Ordering::SeqCst)).await;
	drop(raw);
	until(|| rig.stand.ended.load(Ordering::SeqCst)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_compact_is_acked_or_nacked_by_omp() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	let (bridge, text) = (rig.bridge.clone(), rig.bridge.seal("keep the plan"));
	let first = tokio::spawn(async move { bridge.compact(PANE, text).await });
	let compact = raw.line().await;
	assert_eq!(
		(compact["t"].as_str(), compact["mode"].as_str(), compact["instructions"].as_str()),
		(Some("compact"), Some("handoff"), Some("keep the plan"))
	);
	raw.send(&json!({ "t": "ack", "id": id_of(&compact) })).await;
	assert!(first.await.unwrap().unwrap());

	let (bridge, text) = (rig.bridge.clone(), rig.bridge.seal("again"));
	let second = tokio::spawn(async move { bridge.compact(PANE, text).await });
	let compact = raw.line().await;
	raw.send(&json!({ "t": "nack", "id": id_of(&compact), "error": "omp can't compact" })).await;
	assert_eq!(second.await.unwrap().unwrap_err().to_string(), "omp can't compact");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_calls_result_and_its_error_are_sealed() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	raw.send(&json!({ "t": "call", "id": "1", "tool": "leak", "args": {} })).await;
	let answer = raw.line().await;
	assert_eq!((answer["ok"].as_bool(), answer["text"].as_str()), (Some(true), Some("found [secret] in it")));
	raw.send(&json!({ "t": "call", "id": "2", "tool": "leak_error", "args": {} })).await;
	let answer = raw.line().await;
	assert_eq!((answer["ok"].as_bool(), answer["error"].as_str()), (Some(false), Some("failed with [secret]")));
	assert!(!answer.to_string().contains(SECRET));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_over_four_mebibytes_closes_the_connection_and_the_pane_is_gone() {
	let rig = Rig::new();
	let mut raw = rig.pane(&["prompt"]).await;
	assert_eq!(MAX_LINE, 4 * 1024 * 1024);
	let line = format!("{{\"t\":\"event\",\"name\":\"todo\",\"pad\":\"{}\"}}\n", "x".repeat(MAX_LINE));
	// The service closes while it is still being written to, which fails the write.
	let _ = raw.write.write_all(line.as_bytes()).await;
	assert!(raw.closed().await);
	until(|| !rig.bridge.connected(PANE)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn events_update_what_the_bridge_knows_of_the_pane_and_the_service_hears_each() {
	let rig = Rig::new();
	assert!(rig.bridge.agent(PANE).is_none());
	let mut raw = rig.pane(&["prompt"]).await;
	let first = rig.bridge.agent(PANE).expect("connected");
	assert_eq!((first.working, first.jobs, first.usage_percent, first.todo), (false, None, None, None));

	raw.send(&json!({ "t": "event", "name": "agent_start" })).await;
	until(|| rig.bridge.agent(PANE).is_some_and(|agent| agent.working)).await;
	raw.send(&json!({ "t": "event", "name": "usage", "tokens": 1200, "percent": 12.5 })).await;
	raw.send(&json!({ "t": "event", "name": "todo", "done": 1, "total": 3 })).await;
	raw.send(&json!({ "t": "event", "name": "input", "open": true, "kind": "approval" })).await;
	until(|| rig.bridge.agent(PANE).is_some_and(|agent| agent.input_open)).await;
	let agent = rig.bridge.agent(PANE).unwrap();
	assert_eq!(
		agent,
		PaneAgent {
			working: true,
			input_open: true,
			input_kind: Some("approval".to_owned()),
			jobs: None,
			usage_percent: Some(12.5),
			usage_tokens: Some(1200),
			todo: Some((1, 3)),
			idle_reported: false,
		}
	);

	raw.send(&json!({ "t": "event", "name": "input", "open": false, "kind": "approval" })).await;
	raw.send(&json!({ "t": "event", "name": "agent_end", "idle": true, "jobs": 2 })).await;
	until(|| rig.bridge.agent(PANE).is_some_and(|agent| agent.idle_reported)).await;
	let agent = rig.bridge.agent(PANE).unwrap();
	assert_eq!(
		(agent.working, agent.input_open, agent.input_kind, agent.jobs, agent.idle_reported),
		(false, false, None, Some(2), true)
	);
	// An event the bridge doesn't know changes nothing and doesn't end the connection.
	raw.send(&json!({ "t": "event", "name": "unheard-of" })).await;
	raw.send(&json!({ "t": "event", "name": "shutdown" })).await;
	until(|| rig.stand.events.lock().unwrap().contains(&PaneEvent::Shutdown)).await;

	drop(raw);
	until(|| rig.stand.events.lock().unwrap().last() == Some(&PaneEvent::Gone)).await;
	let events = rig.stand.events.lock().unwrap().clone();
	assert!(matches!(events[0], PaneEvent::Connected { .. }));
	assert_eq!(
		events[1..],
		[
			PaneEvent::AgentStart,
			PaneEvent::Usage { tokens: 1200, percent: 12.5 },
			PaneEvent::Todo { done: 1, total: 3 },
			PaneEvent::Input { open: true, kind: Some("approval".to_owned()) },
			PaneEvent::Input { open: false, kind: Some("approval".to_owned()) },
			PaneEvent::AgentEnd { idle: true, jobs: 2 },
			PaneEvent::Shutdown,
			PaneEvent::Gone,
		]
	);
	assert!(rig.bridge.agent(PANE).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_that_starts_again_takes_the_place_of_the_one_before_it() {
	let rig = Rig::new();
	let old = rig.pane(&["prompt"]).await;
	let mut new = rig.pane(&["prompt"]).await;
	assert_eq!(rig.bridge.panes(), [PANE]);
	// The older omp's end doesn't take the newer one's place away.
	drop(old);
	tokio::time::sleep(Duration::from_millis(200)).await;
	assert!(rig.bridge.connected(PANE));
	// A prompt goes to the newer omp only.
	let (bridge, text) = (rig.bridge.clone(), rig.bridge.seal("hi"));
	let sent = tokio::spawn(async move { bridge.prompt(PANE, text, Deliver::Steer).await });
	let prompt = new.line().await;
	new.send(&json!({ "t": "ack", "id": id_of(&prompt) })).await;
	sent.await.unwrap().unwrap();
}
