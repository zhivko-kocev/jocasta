//! `jocasta thread …` and `jocasta worktree …` (0544): doors to the running service's own thread functions (the ones the
//! boss's tools use) that need no model, over its localhost API with its token (`POST /api/cli`). The changes (`thread
//! send|stop|finish`, `worktree remove`) wait for his Allow in herdr's popup; the rest run at once.

use serde_json::{Map, Value, json};

use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
use crate::shared::service_file::{NOT_RUNNING, connection_refused};

/// Where the service takes these commands: bearer token, body `{command, args}`, reply `{text, isError?}`.
pub const CLI_PATH: &str = "/api/cli";

/// Exit codes: done; refused or failed (Deny and no answer included); bad usage (0140); Jocasta isn't running.
pub struct Exit {
	pub done: i32,
	pub refused: i32,
	pub usage: i32,
	pub not_running: i32,
}

/// `EXIT`.
pub const EXIT: Exit = Exit { done: 0, refused: 1, usage: 2, not_running: 3 };

pub const THREAD_USAGE: &str = concat!(
	"usage: jocasta thread new <project> \"<brief>\" [--title <t>] [--done-when \"<cmd>\"]... [--heavy]\n",
	"       jocasta thread list [<project>]\n",
	"       jocasta thread status <thread>\n",
	"       jocasta thread send <thread> \"<text>\"\n",
	"       jocasta thread stop <thread>\n",
	"       jocasta thread finish <thread> merge|pr",
);

/// `leftover`, not `list`: `jocasta worktree list` is herdr's own (the fork built in).
pub const WORKTREE_USAGE: &str = "usage: jocasta worktree leftover | remove <path>";

/// The word `cli_request` is given: `thread` or `worktree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliWord {
	Thread,
	Worktree,
}

impl CliWord {
	fn as_str(self) -> &'static str {
		match self {
			Self::Thread => "thread",
			Self::Worktree => "worktree",
		}
	}
}

/// One command for the service: its name (`thread new`) and arguments, as the boss's tools name them.
#[derive(Debug, Clone, PartialEq)]
pub struct CliRequest {
	pub command: String,
	pub args: Map<String, Value>,
	/// It waits for his Allow.
	pub asks: bool,
}

/// JavaScript's `String.prototype.trim` emptiness: only white space and line terminators ECMAScript names.
fn blank(text: &str) -> bool {
	text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}').is_empty()
}

fn usage_error(word: CliWord) -> Error {
	Error::user_with_code(if word == CliWord::Thread { THREAD_USAGE } else { WORKTREE_USAGE }, EXIT.usage as u8)
}

fn request(command: &str, args: Value, asks: bool) -> CliRequest {
	let Value::Object(args) = args else { unreachable!("a request's arguments are an object") };
	CliRequest { command: command.to_owned(), args, asks }
}

/// `thread new <project> <brief> [--title t] [--done-when c]… [--heavy]` as a request; bad usage is `usage_error`.
fn new_thread_request(rest: &[String]) -> Result<CliRequest> {
	let usage = || usage_error(CliWord::Thread);
	let mut positional: Vec<&str> = Vec::new();
	let mut done_when: Vec<&str> = Vec::new();
	let mut title: Option<&str> = None;
	let mut heavy = false;
	let mut words = rest.iter().map(String::as_str);
	while let Some(arg) = words.next() {
		// `--heavy` (0590) takes no value: the thread runs on his slow role.
		if arg == "--heavy" {
			if heavy {
				return Err(usage());
			}
			heavy = true;
		} else if arg == "--title" || arg == "--done-when" {
			let value = words.next().filter(|value| !blank(value)).ok_or_else(usage)?;
			if arg == "--title" {
				if title.is_some() {
					return Err(usage());
				}
				title = Some(value);
			} else {
				done_when.push(value);
			}
		} else if arg.starts_with("--") {
			return Err(usage());
		} else {
			positional.push(arg);
		}
	}
	let &[project, brief] = positional.as_slice() else { return Err(usage()) };
	if blank(project) || blank(brief) {
		return Err(usage());
	}
	let mut args = Map::new();
	args.insert("project".to_owned(), json!(project));
	args.insert("brief".to_owned(), json!(brief));
	if let Some(title) = title.filter(|title| !title.is_empty()) {
		args.insert("title".to_owned(), json!(title));
	}
	if !done_when.is_empty() {
		args.insert("done_when".to_owned(), json!(done_when));
	}
	if heavy {
		args.insert("heavy".to_owned(), json!(true));
	}
	Ok(CliRequest { command: "thread new".to_owned(), args, asks: false })
}

/// `jocasta thread|worktree <args>` as a request; bad usage is an `Error::User` with exit 2 and the usage.
pub fn cli_request(word: CliWord, args: &[String]) -> Result<CliRequest> {
	let usage = || usage_error(word);
	let verb = args.first().map(String::as_str);
	let rest = args.get(1..).unwrap_or(&[]);
	let one = |key: &str, asks: bool| -> Result<CliRequest> {
		match rest {
			[value] if !blank(value) => {
				Ok(request(&format!("{} {}", word.as_str(), verb.unwrap_or("")), json!({ key: value }), asks))
			}
			_ => Err(usage()),
		}
	};
	if word == CliWord::Worktree {
		return match verb {
			Some("leftover") if rest.is_empty() => Ok(request("worktree list", json!({}), false)),
			Some("remove") => one("path", true),
			_ => Err(usage()),
		};
	}
	match (verb, rest) {
		(Some("new"), _) => new_thread_request(rest),
		(Some("list"), []) => Ok(request("thread list", json!({}), false)),
		(Some("list"), [project]) => {
			let args = if project.is_empty() { json!({}) } else { json!({ "project": project }) };
			Ok(request("thread list", args, false))
		}
		(Some("status"), _) => one("thread", false),
		(Some("stop"), _) => one("thread", true),
		(Some("send"), [thread, text]) if !blank(thread) && !blank(text) => {
			Ok(request("thread send", json!({ "thread": thread, "text": text }), true))
		}
		(Some("finish"), [thread, how]) if !blank(thread) && (how == "merge" || how == "pr") => {
			Ok(request("thread finish", json!({ "thread": thread, "how": how }), true))
		}
		_ => Err(usage()),
	}
}

/// What `run_cli_command` needs from outside.
pub trait CliDeps {
	/// The running service's port and token from `<data>/jocasta-service.json`, or `None` when its process is gone.
	fn service(&self) -> Option<(u16, String)>;
	fn print(&self, line: &str);
	fn print_error(&self, line: &str);
}

/// No request timeout: his Allow waits 2 minutes and a merge's hooks may take 10; the service bounds every step itself.
fn client() -> Option<reqwest::Client> {
	reqwest::Client::builder().no_proxy().build().ok()
}

/// Runs one command against the service and prints its answer; resolves the exit code (`EXIT`).
pub async fn run_cli_command(request: &CliRequest, deps: &impl CliDeps) -> i32 {
	let Some((port, token)) = deps.service() else {
		deps.print_error(&format!("jocasta: {NOT_RUNNING}"));
		return EXIT.not_running;
	};
	if request.asks {
		deps.print_error("jocasta: asking you first in herdr's popup (Allow with its code; 2 minutes)…");
	}
	let Some(client) = client() else {
		deps.print_error("jocasta: Jocasta's service failed: the HTTP client could not start");
		return EXIT.refused;
	};
	let body = json!({ "command": request.command, "args": request.args });
	let sent = client.post(format!("http://127.0.0.1:{port}{CLI_PATH}")).bearer_auth(&token).json(&body).send().await;
	let response = match sent {
		Ok(response) => response,
		Err(error) => {
			if connection_refused(&error) {
				deps.print_error(&format!("jocasta: {NOT_RUNNING}"));
				return EXIT.not_running;
			}
			// Anything but a refused connection is unexpected: it goes to the log before the one-line reply (DUP-12).
			log(LogLevel::Error, "cli", &format!("{error:?}"), None);
			deps.print_error(&format!("jocasta: Jocasta's service failed: {error}"));
			return EXIT.refused;
		}
	};
	let status = response.status();
	let body: Option<Value> = response.json().await.ok();
	let object = body.as_ref().and_then(Value::as_object);
	let Some(text) = object.and_then(|object| object.get("text")).and_then(Value::as_str) else {
		deps.print_error(&format!("jocasta: Jocasta's service answered HTTP {} without a result", status.as_u16()));
		return EXIT.refused;
	};
	if !status.is_success() || object.and_then(|object| object.get("isError")) == Some(&Value::Bool(true)) {
		deps.print_error(&format!("jocasta: {text}"));
		return EXIT.refused;
	}
	deps.print(text);
	EXIT.done
}

#[cfg(test)]
#[path = "thread_cli_tests.rs"]
mod tests;
