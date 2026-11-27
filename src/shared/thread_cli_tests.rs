use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

use super::*;
use crate::shared::bridge_wire::{MAX_LINE, Stream, VERSION, bridge_address, cli_hello, line_of};
use crate::shared::line_rpc::LineReader;

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
	assert_eq!(request(Worktree, &["leftover"]), expected("worktree list", json!({}), false));
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
	for list in [&[][..], &["leftover", "x"], &["list"], &["remove"], &["prune"]] {
		assert_eq!(usage(Worktree, list), WORKTREE_USAGE, "{list:?}");
	}
}

/// What a fake service read: the hello and each `cli` line, as sent.
type Calls = Arc<Mutex<Vec<Value>>>;

/// Serves one connection: a hello with the token `tok` is welcomed (any other refused), each `cli` line is kept and
/// answered with `reply` under its id.
async fn serve(stream: Stream, reply: Value, seen: Calls) {
	let (read, mut write) = tokio::io::split(stream);
	let mut lines = LineReader::new(read, MAX_LINE);
	while let Ok(Some(line)) = lines.next_line().await {
		let message: Value = serde_json::from_str(&line).unwrap();
		seen.lock().unwrap().push(message.clone());
		let answer = match message["t"].as_str() {
			Some("hello") if message["token"] == "tok" => json!({ "t": "welcome", "v": VERSION }),
			Some("hello") => json!({ "t": "refused" }),
			_ => {
				let mut answer = reply.clone();
				answer["t"] = json!("result");
				answer["id"] = message["id"].clone();
				answer
			}
		};
		write.write_all(line_of(&answer).as_bytes()).await.unwrap();
		write.flush().await.unwrap();
	}
}

/// A bridge answering `cli` lines with `reply`; the port its pipe or socket is named for, and what it read.
#[cfg(windows)]
async fn fake_service(reply: Value) -> (u16, Calls) {
	use tokio::net::windows::named_pipe::ServerOptions;
	let port = crate::shared::fsx::free_port().unwrap();
	let calls = Calls::default();
	let seen = Arc::clone(&calls);
	let server = ServerOptions::new().first_pipe_instance(true).create(bridge_address(port)).unwrap();
	tokio::spawn(async move {
		server.connect().await.unwrap();
		serve(Box::new(server), reply, seen).await;
	});
	(port, calls)
}

/// A bridge answering `cli` lines with `reply`; the port its pipe or socket is named for, and what it read.
#[cfg(unix)]
async fn fake_service(reply: Value) -> (u16, Calls) {
	let port = crate::shared::fsx::free_port().unwrap();
	let calls = Calls::default();
	let seen = Arc::clone(&calls);
	let address = bridge_address(port);
	std::fs::create_dir_all(std::path::Path::new(&address).parent().unwrap()).unwrap();
	let listener = tokio::net::UnixListener::bind(&address).unwrap();
	tokio::spawn(async move {
		let (stream, _) = listener.accept().await.unwrap();
		serve(Box::new(stream), reply, seen).await;
		let _ = std::fs::remove_file(address);
	});
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
async fn a_command_goes_over_the_bridge_with_the_token_exit_0_done_1_refused_3_when_jocasta_isnt_running_0544() {
	let (port, calls) = fake_service(json!({ "ok": true, "text": "Stopped Login's run." })).await;
	let deps = at(port);
	let stop = request(CliWord::Thread, &["stop", "Login"]);
	assert_eq!(run_cli_command(&stop, &deps).await, EXIT.done);
	assert_eq!(
		*calls.lock().unwrap(),
		[cli_hello("tok"), json!({ "t": "cli", "id": "1", "command": "thread stop", "args": { "thread": "Login" } }),]
	);
	assert_eq!(*deps.out.borrow(), ["Stopped Login's run."]);
	assert_eq!(*deps.err.borrow(), ["jocasta: asking you first in your terminal (Allow with its code; 2 minutes)…"]);

	let (port, _) = fake_service(json!({ "ok": false, "error": "He chose Deny; nothing was stopped." })).await;
	let deps = at(port);
	let list = request(CliWord::Thread, &["list"]);
	assert_eq!(run_cli_command(&list, &deps).await, EXIT.refused);
	assert_eq!(*deps.err.borrow(), ["jocasta: He chose Deny; nothing was stopped."]);
	assert!(deps.out.borrow().is_empty());

	let deps = Deps::default();
	assert_eq!(run_cli_command(&list, &deps).await, EXIT.not_running);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta isn't running; start it with jocasta."]);

	// jocasta-service.json names a live process, but nothing serves its bridge.
	let deps = at(crate::shared::fsx::free_port().unwrap());
	let worktrees = request(CliWord::Worktree, &["leftover"]);
	assert_eq!(run_cli_command(&worktrees, &deps).await, EXIT.not_running);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta isn't running; start it with jocasta."]);

	// A result without a text or an error is refused with a line of its own.
	let (port, _) = fake_service(json!({ "ok": true })).await;
	let deps = at(port);
	assert_eq!(run_cli_command(&worktrees, &deps).await, EXIT.refused);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta's service answered without a result"]);

	// The bridge refuses the hello: a wrong token.
	let (port, _) = fake_service(json!({ "ok": true, "text": "never" })).await;
	let deps = Deps { address: Some((port, "wrong".to_owned())), ..Deps::default() };
	assert_eq!(run_cli_command(&worktrees, &deps).await, EXIT.refused);
	assert_eq!(*deps.err.borrow(), ["jocasta: Jocasta's service failed: the bridge refused the hello"]);
}

#[tokio::test]
async fn status_json_prints_the_services_line_and_running_false_with_exit_3_when_jocasta_isnt_running_0700() {
	let line =
		r#"{"running":true,"version":"0.1.0","needsYou":0,"blocked":0,"underway":0,"spendToday":0.0,"threads":[]}"#;
	let (port, calls) = fake_service(json!({ "ok": true, "text": line })).await;
	let deps = at(port);
	assert_eq!(run_status_json(&deps).await, EXIT.done);
	assert_eq!(calls.lock().unwrap()[1], json!({ "t": "cli", "id": "1", "command": "status json", "args": {} }));
	assert_eq!(*deps.out.borrow(), [line]);
	assert!(deps.err.borrow().is_empty());
	// No service file, or a service file whose bridge nothing serves: JSON on stdout, nothing on stderr.
	for deps in [Deps::default(), at(crate::shared::fsx::free_port().unwrap())] {
		assert_eq!(run_status_json(&deps).await, EXIT.not_running);
		assert_eq!(*deps.out.borrow(), [r#"{"running":false}"#]);
		assert!(deps.err.borrow().is_empty());
	}
}

#[tokio::test]
async fn status_json_says_running_false_no_answer_with_exit_3_when_the_service_doesnt_answer_in_2_s() {
	let port = crate::shared::fsx::free_port().unwrap();
	let address = bridge_address(port);
	// The bridge is there and takes the connection, but the service never answers (a start that hangs).
	#[cfg(unix)]
	let _held = {
		std::fs::create_dir_all(std::path::Path::new(&address).parent().unwrap()).unwrap();
		tokio::net::UnixListener::bind(&address).unwrap()
	};
	#[cfg(windows)]
	let _held =
		tokio::net::windows::named_pipe::ServerOptions::new().first_pipe_instance(true).create(&address).unwrap();
	let deps = at(port);
	let asked = std::time::Instant::now();
	assert_eq!(run_status_json(&deps).await, EXIT.not_running);
	assert!(asked.elapsed() < std::time::Duration::from_secs(5), "{:?}", asked.elapsed());
	assert_eq!(*deps.out.borrow(), [r#"{"running":false,"reason":"no answer"}"#]);
	assert!(deps.err.borrow().is_empty());
	#[cfg(unix)]
	let _ = std::fs::remove_file(address);
}
