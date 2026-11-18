use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::*;

fn words(list: &[&str]) -> Vec<String> {
	list.iter().map(|word| (*word).to_owned()).collect()
}

fn request(word: CliWord, list: &[&str]) -> CliRequest {
	cli_request(word, &words(list)).unwrap()
}

fn expected(command: &str, args: Value, asks: bool) -> CliRequest {
	CliRequest { command: command.to_owned(), args: args.as_object().unwrap().clone(), asks }
}

#[test]
fn thread_and_worktree_words_become_the_services_commands_the_changes_ask_him_0544() {
	use CliWord::{Thread, Worktree};
	assert_eq!(
		request(
			Thread,
			&[
				"new",
				"demo",
				"Fix the login",
				"--done-when",
				"bun test a",
				"--title",
				"Login",
				"--done-when",
				"bun test b"
			]
		),
		expected(
			"thread new",
			json!({ "project": "demo", "brief": "Fix the login", "title": "Login", "done_when": ["bun test a", "bun test b"] }),
			false
		)
	);
	assert_eq!(
		request(Thread, &["new", "demo", "Fix it"]),
		expected("thread new", json!({ "project": "demo", "brief": "Fix it" }), false)
	);
	// `--heavy` is a bare flag, anywhere among the others (0590).
	assert_eq!(
		request(Thread, &["new", "--heavy", "demo", "Migrate it", "--title", "Migrate"]),
		expected(
			"thread new",
			json!({ "project": "demo", "brief": "Migrate it", "title": "Migrate", "heavy": true }),
			false
		)
	);
	assert_eq!(request(Thread, &["list"]), expected("thread list", json!({}), false));
	assert_eq!(request(Thread, &["list", "demo"]), expected("thread list", json!({ "project": "demo" }), false));
	assert_eq!(request(Thread, &["status", "Login"]), expected("thread status", json!({ "thread": "Login" }), false));
	assert_eq!(
		request(Thread, &["send", "Login", "Go on."]),
		expected("thread send", json!({ "thread": "Login", "text": "Go on." }), true)
	);
	assert_eq!(request(Thread, &["stop", "Login"]), expected("thread stop", json!({ "thread": "Login" }), true));
	assert_eq!(
		request(Thread, &["finish", "Login", "pr"]),
		expected("thread finish", json!({ "thread": "Login", "how": "pr" }), true)
	);
	assert_eq!(request(Worktree, &["list"]), expected("worktree list", json!({}), false));
	assert_eq!(request(Worktree, &["remove", "C:/x"]), expected("worktree remove", json!({ "path": "C:/x" }), true));
	let usage = |word: CliWord, list: &[&str]| match cli_request(word, &words(list)) {
		Err(Error::User { message, exit_code }) => {
			assert_eq!(i32::from(exit_code), EXIT.usage);
			message
		}
		other => panic!("no usage error for {}: {other:?}", list.join(" ")),
	};
	for list in [
		&[][..],
		&["new", "demo"],
		&["new", "demo", "brief", "extra"],
		&["new", "demo", "brief", "--done-when"],
		&["new", "demo", "brief", "--title", "a", "--title", "b"],
		&["new", "demo", "brief", "--force"],
		&["new", "demo", "brief", "--heavy", "true"],
		&["new", "demo", "brief", "--heavy=true"],
		&["new", "demo", "brief", "--heavy", "--heavy"],
		&["list", "a", "b"],
		&["status"],
		&["send", "x"],
		&["stop", "x", "y"],
		&["finish", "x", "squash"],
		&["finish", "x"],
	] {
		assert_eq!(usage(Thread, list), THREAD_USAGE, "{list:?}");
	}
	for list in [&[][..], &["list", "x"], &["remove"], &["prune"]] {
		assert_eq!(usage(Worktree, list), WORKTREE_USAGE, "{list:?}");
	}
}

/// What a fake service saw: the path, the authorization header and the body.
type Calls = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

/// A service answering `POST /api/cli` with `reply`; its port and what it got.
async fn fake_service(reply: impl Fn() -> Response + Clone + Send + Sync + 'static) -> (u16, Calls) {
	let calls = Calls::default();
	let seen = Arc::clone(&calls);
	let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap, Json(body): Json<Value>| {
		let seen = Arc::clone(&seen);
		let reply = reply.clone();
		async move {
			let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).map(str::to_owned);
			seen.lock().unwrap().push((uri.path().to_owned(), auth, body));
			reply()
		}
	});
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let port = listener.local_addr().unwrap().port();
	tokio::spawn(async move { axum::serve(listener, app).await });
	(port, calls)
}

#[derive(Default)]
struct Deps {
	address: Option<(u16, String)>,
	out: RefCell<Vec<String>>,
	err: RefCell<Vec<String>>,
}

impl CliDeps for Deps {
	fn service(&self) -> Option<(u16, String)> {
		self.address.clone()
	}
	fn print(&self, line: &str) {
		self.out.borrow_mut().push(line.to_owned());
	}
	fn print_error(&self, line: &str) {
		self.err.borrow_mut().push(line.to_owned());
	}
}

fn at(port: u16) -> Deps {
	Deps { address: Some((port, "tok".to_owned())), ..Deps::default() }
}

#[tokio::test]
async fn a_command_goes_to_the_service_with_its_token_exit_0_done_1_refused_3_when_jocasta_isnt_running_0544() {
	let (port, calls) = fake_service(|| Json(json!({ "text": "Stopped Login's run." })).into_response()).await;
	let deps = at(port);
	let stop = request(CliWord::Thread, &["stop", "Login"]);
	assert_eq!(run_cli_command(&stop, &deps).await, EXIT.done);
	assert_eq!(
		*calls.lock().unwrap(),
		[(
			CLI_PATH.to_owned(),
			Some("Bearer tok".to_owned()),
			json!({ "command": "thread stop", "args": { "thread": "Login" } })
		)]
	);
	assert_eq!(*deps.out.borrow(), ["Stopped Login's run."]);
	assert_eq!(*deps.err.borrow(), ["jocasta: asking you first in herdr's popup (Allow with its code; 2 minutes)…"]);

	let (port, _) = fake_service(|| {
		Json(json!({ "text": "He chose Deny; nothing was stopped.", "isError": true })).into_response()
	})
	.await;
	let deps = at(port);
	let list = request(CliWord::Thread, &["list"]);
	assert_eq!(run_cli_command(&list, &deps).await, EXIT.refused);
	assert_eq!(*deps.err.borrow(), ["jocasta: He chose Deny; nothing was stopped."]);
	assert!(deps.out.borrow().is_empty());

	let deps = Deps::default();
	assert_eq!(run_cli_command(&list, &deps).await, EXIT.not_running);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta isn't running; start it with jocasta."]);

	// jocasta-service.json names a live process, but nothing listens on its port.
	let deps = at(crate::shared::fsx::free_port().unwrap());
	let worktrees = request(CliWord::Worktree, &["list"]);
	assert_eq!(run_cli_command(&worktrees, &deps).await, EXIT.not_running);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta isn't running; start it with jocasta."]);

	let (port, _) = fake_service(|| (StatusCode::INTERNAL_SERVER_ERROR, "nope").into_response()).await;
	let deps = at(port);
	assert_eq!(run_cli_command(&worktrees, &deps).await, EXIT.refused);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta's service answered HTTP 500 without a result"]);
}
