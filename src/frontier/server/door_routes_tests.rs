//! Tests of `door_routes.rs`: the MCP endpoint case of `server.test.ts`, and every door's answers. The TypeScript
//! reaches them through `withServer` and the real service; here each runs on the routes with a stand-in service, on a
//! listening server as `routes_tests.rs` does. The cases that need the real tools wait for `server.rs` (see the report).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use futures_util::FutureExt;
use serde_json::json;

use super::*;
use crate::frontier::server::routes::{Served, listen};
use crate::shared::errors::Error;
use crate::shared::testing::temp_jocasta;

type Tools = Vec<(String, Map<String, Value>, Caller)>;

#[derive(Default)]
struct Stand {
	tools: Mutex<Tools>,
	clis: Mutex<Vec<(String, Map<String, Value>)>>,
	pushes: Mutex<Vec<PushRequest>>,
	checks: Mutex<Vec<String>>,
	check_aborted: Arc<AtomicBool>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl DoorService for Stand {
	fn call_tool<'a>(
		&'a self,
		name: &'a str,
		args: Map<String, Value>,
		caller: Caller,
	) -> LocalBoxFuture<'a, Result<ToolReply>> {
		locked(&self.tools).push((name.to_owned(), args, caller));
		let reply = match name {
			"nope" => Ok(ToolReply { text: "no tool nope.".to_owned(), is_error: Some(true) }),
			"boom" => Err(Error::other("the tool broke")),
			_ => Ok(ToolReply { text: "TODAY · Mon".to_owned(), is_error: None }),
		};
		async move { reply }.boxed()
	}

	fn cli_call<'a>(&'a self, command: &'a str, args: Map<String, Value>) -> LocalBoxFuture<'a, Result<ToolReply>> {
		locked(&self.clis).push((command.to_owned(), args));
		async { Ok(ToolReply { text: "done".to_owned(), is_error: None }) }.boxed()
	}

	fn push_verdict(&self, request: PushRequest) -> LocalBoxFuture<'_, Result<PushAnswer>> {
		let refuse = request.remote == "evil";
		locked(&self.pushes).push(request);
		let answer = PushAnswer { allow: !refuse, reason: refuse.then(|| "not that remote".to_owned()) };
		async move { Ok(answer) }.boxed()
	}

	fn pane_check<'a>(&'a self, pane_key: &'a str, signal: AbortSignal) -> LocalBoxFuture<'a, Result<String>> {
		locked(&self.checks).push(pane_key.to_owned());
		if pane_key == "slow" {
			// The handler's future is dropped with its connection, so a task outside it sees the abort.
			let (mut watching, flag) = (signal, self.check_aborted.clone());
			tokio::spawn(async move {
				let _ = watching.wait_for(Option::is_some).await;
				flag.store(true, Ordering::SeqCst);
			});
			return std::future::pending().boxed();
		}
		async move { Ok(format!("check of {pane_key}")) }.boxed()
	}
}

struct Running {
	stand: Arc<Stand>,
	closed: Arc<AtomicUsize>,
	server: ServerHandle,
	client: reqwest::Client,
}

impl Running {
	fn url(&self, path: &str) -> String {
		format!("http://127.0.0.1:{}{path}", self.server.port())
	}

	async fn post(&self, path: &str, token: Option<&str>, body: &str) -> reqwest::Response {
		let mut request = self.client.post(self.url(path)).body(body.to_owned());
		if let Some(token) = token {
			request = request.bearer_auth(token);
		}
		request.send().await.unwrap()
	}
}

fn running() -> Running {
	let stand = Arc::new(Stand::default());
	let closed = Arc::new(AtomicUsize::new(0));
	let count = closed.clone();
	let routes = door_routes(DoorParts {
		token: "t".to_owned(),
		service: stand.clone(),
		client_closed: Arc::new(move || {
			count.fetch_add(1, Ordering::SeqCst);
		}),
	});
	let server = listen(
		0,
		Served {
			routes,
			page: Box::new(|_request, _url, _port| Box::pin(async { text_response(StatusCode::NOT_FOUND, "page") })),
		},
	)
	.unwrap();
	Running { stand, closed, server, client: reqwest::Client::new() }
}

async fn json_of(response: reqwest::Response) -> Value {
	response.json().await.unwrap()
}

#[tokio::test]
async fn the_mcp_endpoint_takes_the_service_token_as_a_bearer_and_answers_text_and_is_error() {
	let _j = temp_jocasta();
	let r = running();
	let body = json!({ "args": {}, "bossKey": "boss" }).to_string();
	let none = r.post("/api/mcp/today", None, &body).await;
	assert_eq!(none.status(), 401);
	assert_eq!(none.headers()["content-type"], "application/json;charset=utf-8");
	assert_eq!(json_of(none).await, json!({ "text": "invalid token", "isError": true }));
	assert_eq!(r.post("/api/mcp/today", Some("nope"), &body).await.status(), 401);
	assert!(locked(&r.stand.tools).is_empty());
	let ok = r.post("/api/mcp/today", Some("t"), &body).await;
	assert_eq!(ok.status(), 200);
	assert_eq!(ok.headers()["content-type"], "application/json;charset=utf-8");
	let reply = json_of(ok).await;
	assert_eq!(reply, json!({ "text": "TODAY · Mon" }));
	assert!(reply.get("isError").is_none());
	let unknown = r.post("/api/mcp/nope", Some("t"), &body).await;
	assert_eq!(json_of(unknown).await, json!({ "text": "no tool nope.", "isError": true }));
	r.server.stop(true).await;
}

#[tokio::test]
async fn the_mcp_endpoint_hands_the_tool_its_name_decoded_its_args_as_an_object_and_its_caller() {
	let _j = temp_jocasta();
	let r = running();
	let both = json!({ "args": { "a": 1 }, "paneKey": "k", "bossKey": "b" }).to_string();
	r.post("/api/mcp/thread_status", Some("t"), &both).await;
	r.post(
		"/api/mcp/the%20tool%2Fx%C3%A9",
		Some("t"),
		&json!({ "args": [1], "paneKey": 3, "bossKey": null }).to_string(),
	)
	.await;
	r.post("/api/mcp/bare", Some("t"), "[1]").await;
	r.server.stop(true).await;
	let tools = locked(&r.stand.tools);
	let (name, args, caller) = &tools[0];
	assert_eq!(name, "thread_status");
	assert_eq!(Value::Object(args.clone()), json!({ "a": 1 }));
	assert_eq!(*caller, Caller { pane_key: Some("k".to_owned()), boss_key: Some("b".to_owned()) });
	assert_eq!(tools[1].0, "the tool/xé");
	assert!(tools[1].1.is_empty());
	assert_eq!(tools[1].2, Caller::default());
	assert_eq!(tools[2].0, "bare");
	assert!(tools[2].1.is_empty());
	assert_eq!(tools[2].2, Caller::default());
	drop(tools);
}

#[tokio::test]
async fn a_door_refuses_another_method_a_body_that_is_not_json_and_answers_a_throw_as_a_500() {
	let _j = temp_jocasta();
	let r = running();
	let get = r.client.get(r.url("/api/cli")).bearer_auth("t").send().await.unwrap();
	assert_eq!(get.status(), 405);
	assert_eq!(json_of(get).await, json!({ "text": "POST only", "isError": true }));
	for path in ["/api/mcp/today", "/api/cli", "/api/push", "/api/pane-check"] {
		let bad = r.post(path, Some("t"), "{nope").await;
		assert_eq!(bad.status(), 400, "{path}");
		assert_eq!(json_of(bad).await, json!({ "text": "body is not JSON", "isError": true }), "{path}");
	}
	let empty = r.post("/api/cli", Some("t"), "").await;
	assert_eq!(empty.status(), 400);
	// A tool that throws, a `null` body and a malformed tool name are the server's 500.
	for (path, body) in [("/api/mcp/boom", "{}"), ("/api/mcp/today", "null"), ("/api/mcp/%zz", "{}")] {
		let thrown = r.post(path, Some("t"), body).await;
		assert_eq!(thrown.status(), 500, "{path}");
		assert_eq!(thrown.text().await.unwrap(), "Internal error", "{path}");
	}
	r.server.stop(true).await;
}

#[tokio::test]
async fn the_cli_door_needs_a_command_and_answers_the_services_reply() {
	let _j = temp_jocasta();
	let r = running();
	let missing = r.post("/api/cli", Some("t"), &json!({ "args": {} }).to_string()).await;
	assert_eq!(missing.status(), 400);
	assert_eq!(json_of(missing).await, json!({ "text": "command is required", "isError": true }));
	let numeric = r.post("/api/cli", Some("t"), &json!({ "command": 3 }).to_string()).await;
	assert_eq!(numeric.status(), 400);
	let ok = r.post("/api/cli", Some("t"), &json!({ "command": "thread", "args": { "x": 1 } }).to_string()).await;
	assert_eq!(json_of(ok).await, json!({ "text": "done" }));
	r.server.stop(true).await;
	let clis = locked(&r.stand.clis);
	assert_eq!(clis[0].0, "thread");
	assert_eq!(Value::Object(clis[0].1.clone()), json!({ "x": 1 }));
	drop(clis);
}

#[tokio::test]
async fn the_push_door_takes_a_push_request_and_answers_allow_and_a_reason() {
	let _j = temp_jocasta();
	let r = running();
	let not = r.post("/api/push", Some("t"), &json!({ "dir": "x" }).to_string()).await;
	assert_eq!(not.status(), 400);
	assert_eq!(json_of(not).await, json!({ "text": "not a push request", "isError": true }));
	let push = |remote: &str| {
		json!({
			"dir": "C:/work/acme", "remote": remote, "paneKey": "k",
			"refs": [{ "ref": "refs/heads/main", "local": "a", "remote": "b" }]
		})
		.to_string()
	};
	assert_eq!(json_of(r.post("/api/push", Some("t"), &push("origin")).await).await, json!({ "allow": true }));
	assert_eq!(
		json_of(r.post("/api/push", Some("t"), &push("evil")).await).await,
		json!({ "allow": false, "reason": "not that remote" })
	);
	r.server.stop(true).await;
	let pushes = locked(&r.stand.pushes);
	assert_eq!(pushes.len(), 2);
	assert_eq!(pushes[0].pane_key.as_deref(), Some("k"));
	assert_eq!(pushes[0].refs[0].r#ref, "refs/heads/main");
	drop(pushes);
}

#[tokio::test]
async fn the_pane_check_door_needs_a_pane_key_answers_text_and_aborts_the_check_when_its_request_ends() {
	let _j = temp_jocasta();
	let r = running();
	let missing = r.post("/api/pane-check", Some("t"), &json!({ "paneKey": 3 }).to_string()).await;
	assert_eq!(missing.status(), 400);
	assert_eq!(json_of(missing).await, json!({ "text": "paneKey is required", "isError": true }));
	let ok = r.post("/api/pane-check", Some("t"), &json!({ "paneKey": "pk" }).to_string()).await;
	assert_eq!(json_of(ok).await, json!({ "text": "check of pk" }));
	// A client that gives up ends the check: the handler is dropped with its connection.
	let impatient = reqwest::Client::builder().timeout(Duration::from_millis(200)).build().unwrap();
	let ended = impatient
		.post(r.url("/api/pane-check"))
		.bearer_auth("t")
		.body(json!({ "paneKey": "slow" }).to_string())
		.send()
		.await;
	assert!(ended.is_err());
	for _ in 0..200 {
		if r.stand.check_aborted.load(Ordering::SeqCst) {
			break;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	assert!(r.stand.check_aborted.load(Ordering::SeqCst));
	r.server.stop(true).await;
}

#[tokio::test]
async fn shutdown_is_answered_202_at_once_and_the_close_follows_after_the_answer() {
	let _j = temp_jocasta();
	let r = running();
	let none = r.post("/api/shutdown", None, "").await;
	assert_eq!(none.status(), 401);
	assert_eq!(none.headers()["content-type"], "text/plain;charset=utf-8");
	assert_eq!(none.text().await.unwrap(), "invalid token");
	let get = r.client.get(r.url("/api/shutdown")).bearer_auth("t").send().await.unwrap();
	assert_eq!((get.status().as_u16(), get.text().await.unwrap().as_str()), (405, "POST only"));
	assert_eq!(r.closed.load(Ordering::SeqCst), 0);
	let accepted = r.post("/api/shutdown", Some("t"), "").await;
	assert_eq!(accepted.status(), 202);
	assert_eq!(accepted.text().await.unwrap(), "");
	assert_eq!(r.closed.load(Ordering::SeqCst), 0);
	for _ in 0..200 {
		if r.closed.load(Ordering::SeqCst) > 0 {
			break;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	assert_eq!(r.closed.load(Ordering::SeqCst), 1);
	r.server.stop(true).await;
}

#[tokio::test]
async fn each_door_answers_its_own_path_only() {
	let r = running();
	for (path, expected) in [
		("/api/mcp/x", true),
		("/api/mcp", false),
		("/api/cli", true),
		("/api/cli/x", false),
		("/api/push", true),
		("/api/pane-check", true),
		("/api/shutdown", true),
		("/api/shutdowns", false),
	] {
		let routes =
			door_routes(DoorParts { token: "t".to_owned(), service: r.stand.clone(), client_closed: Arc::new(|| {}) });
		assert_eq!(routes.iter().filter(|route| route.matches(path)).count(), usize::from(expected), "{path}");
	}
	assert_eq!(r.client.get(r.url("/api/mcp")).send().await.unwrap().text().await.unwrap(), "page");
	r.server.stop(true).await;
}

#[test]
fn decode_uri_component_throws_where_javascript_does() {
	assert_eq!(decode_uri_component("a%20b%2Fx%C3%A9").as_deref(), Some("a b/xé"));
	assert_eq!(decode_uri_component("plain").as_deref(), Some("plain"));
	for bad in ["%", "%2", "%zz", "%+1", "%E0%A4%A", "%FF"] {
		assert_eq!(decode_uri_component(bad), None, "{bad}");
	}
}
