//! Bench threads start and are read as `jocasta thread new` and `jocasta thread list` do (0576, 0544): a `cli` message on
//! the service's bridge (0675) with the service's token, which starts a thread without his Allow and needs no boss key.
//! Threads still open in plan mode and wait for his approval of their plan in their pane.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::lib::sandbox::cli_request;
use jocasta::shared::errors::{Error, Result};

static STARTED_BRANCH: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?m) on (?:branch )?(\S+?)(?:[.,:]|\r?$)").expect("a fixed pattern"));

/// The branch a start's reply names (`Started <title> on branch <branch>…` for the boss, `Started <project>/<title> on
/// <branch>: …` for the command line), or `None`.
pub fn started_branch(reply: &str) -> Option<String> {
	STARTED_BRANCH.captures(reply).map(|found| found[1].to_owned())
}

/// `jocasta thread new <project> "<brief>" --title <title>`; resolves the new thread's branch, fails on a refusal.
pub async fn start_thread(project: &str, title: &str, brief: &str) -> Result<String> {
	let args = json!({ "project": project, "title": title, "brief": brief });
	let Value::Object(args) = args else { unreachable!("a literal object") };
	let reply = cli_request("thread new", args).await?;
	match started_branch(&reply.text) {
		Some(branch) if reply.is_error != Some(true) => Ok(branch),
		_ => Err(Error::other(format!("thread new {title}: {}", reply.text))),
	}
}

/// Prints `message` and waits for Enter on stdin: stopping a thread and removing its worktree ask him first (0484), so
/// the bench leaves closing its threads to the operator between runs.
pub async fn wait_for_enter(message: &str) {
	println!("{message}");
	let mut line = String::new();
	let _ = BufReader::new(tokio::io::stdin()).read_line(&mut line).await;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThreadLine {
	pub title: String,
	pub branch: String,
	pub state: String,
	pub overlaps: Vec<String>,
	/// The outcome of the report it handed in for its current task (`· reported done`), or `None`.
	pub reported: Option<String>,
}

static THREAD_LINE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^[^/\s][^/]*/(.+?) \[([^\]]+)\] (done|plan ready|awaiting you|idle|working|blocked|unknown)(.*)$")
		.expect("a fixed pattern")
});
static REPORTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" · reported (\S+)$").expect("a fixed pattern"));

const OVERLAPS: &str = " · overlaps ";

/// One `jocasta thread list` line: `<project>/<title> [<branch>] <state>[ — …][ · overlaps <title>: files; …][ ·
/// reported <outcome>]`; `None` for any other line (its indented ledger line, "No threads.").
pub fn parse_thread_line(line: &str) -> Option<ThreadLine> {
	let found = THREAD_LINE.captures(line.trim_end())?;
	let mut rest = found.get(4).map_or("", |rest| rest.as_str());
	let report = REPORTED.captures(rest);
	if let Some(report) = &report {
		rest = &rest[..report.get(0).map_or(0, |whole| whole.start())];
	}
	let overlaps = match rest.find(OVERLAPS) {
		None => Vec::new(),
		Some(at) => rest[at + OVERLAPS.len()..]
			.split("; ")
			.map(|part| {
				// `part.slice(0, part.lastIndexOf(": "))`: without a ": " the index is -1 and the last character goes.
				let end = part.rfind(": ").unwrap_or_else(|| part.char_indices().next_back().map_or(0, |(i, _)| i));
				part[..end].trim().to_owned()
			})
			.filter(|title| !title.is_empty())
			.collect(),
	};
	Some(ThreadLine {
		title: found[1].to_owned(),
		branch: found[2].to_owned(),
		state: found[3].to_owned(),
		overlaps,
		reported: report.map(|report| report[1].to_owned()),
	})
}

/// `jocasta thread list [<project>]`, parsed; fails on a refusal.
pub async fn list_threads(project: Option<&str>) -> Result<Vec<ThreadLine>> {
	let mut args = Map::new();
	if let Some(project) = project.filter(|project| !project.is_empty()) {
		args.insert("project".to_owned(), Value::String(project.to_owned()));
	}
	let reply = cli_request("thread list", args).await?;
	if reply.is_error == Some(true) {
		return Err(Error::other(format!("thread list: {}", reply.text)));
	}
	Ok(reply.text.split('\n').filter_map(parse_thread_line).collect())
}

#[cfg(test)]
#[path = "threads_tests.rs"]
mod tests;
