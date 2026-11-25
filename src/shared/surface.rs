//! Bare `jocasta`'s work surface (0387, 0434, 0549, 0683): Jocasta's own shell and the service in one process. What
//! stays here runs around it: a service that is closing is waited for first (LC-3), and a service that couldn't start
//! is said once the shell ends (0582).

use std::fs;
use std::path::Path;
use std::time::Duration;

use regex::Regex;

use crate::frontier::server::launch::{ServiceHealth, ServiceState};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::log::LOG_FILE;

/// What bare `jocasta` needs from outside.
pub trait SurfaceDeps {
	/// What the service `<data>/jocasta-service.json` names is doing (`service_health`): its `/health` with the file's
	/// token and pid, not merely whether that pid is alive, since a stale file's pid may belong to another program
	/// (CR-15).
	fn service_health(&self) -> impl Future<Output = ServiceHealth>;
	/// The service's last ERROR line in Jocasta's log since `since` (epoch ms), its message only (`last_logged_error`).
	fn last_error(&self, since: i64) -> Option<String>;
	fn print(&self, line: &str);
	fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
		tokio::time::sleep(Duration::from_millis(ms))
	}
}

/// A line of Jocasta's log: `<ISO time> <LEVEL> [<scope>] <message>`.
fn log_line() -> Regex {
	Regex::new(r"^(\S+) ERROR \[[^\]]*\] ([^\n\r\u{2028}\u{2029}]*)$").expect("the log line pattern")
}

/// The message of the last ERROR line in Jocasta's log folder `dir` written at or after `since` (epoch ms), from the
/// newest two day files; `None` when there is none. The service's "didn't start: " lead is left out (SHR-5).
pub fn last_logged_error(dir: &Path, since: i64) -> Option<String> {
	let mut names: Vec<String> = fs::read_dir(dir)
		.ok()?
		.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
		.filter(|name| LOG_FILE.is_match(name))
		.collect();
	names.sort();
	let pattern = log_line();
	let mut last: Option<String> = None;
	for name in names.iter().skip(names.len().saturating_sub(2)) {
		let Ok(text) = fs::read_to_string(dir.join(name)) else { continue };
		for line in text.lines() {
			let Some(found) = pattern.captures(line) else { continue };
			let at = chrono::DateTime::parse_from_rfc3339(&found[1]).map(|at| at.timestamp_millis());
			if at.is_ok_and(|at| at >= since - 1_000) {
				last = Some(found[2].to_owned());
			}
		}
	}
	last.map(|message| message.strip_prefix("didn't start: ").map_or_else(|| message.clone(), str::to_owned))
}

/// Before the shell opens: a service that is closing Jocasta is waited for, up to a minute (LC-3).
pub async fn before_surface(deps: &impl SurfaceDeps) -> Result<()> {
	let mut state = deps.service_health().await.state;
	if state == ServiceState::Closing {
		deps.print("jocasta: Jocasta is closing; it starts again once it has.");
		let end = now_ms() + 60_000;
		while state == ServiceState::Closing {
			if now_ms() >= end {
				return Err(Error::user_with_code(
					"Jocasta is still closing after a minute; run jocasta again in a moment.",
					1,
				));
			}
			deps.sleep(250).await;
			state = deps.service_health().await.state;
		}
	}
	Ok(())
}

/// How many times, 250 ms apart, the service is waited for once herdr's server went from under the client: its stop
/// steps are bounded, and a failed start's stop takes well under a second.
const CLOSING_POLLS: u32 = 60;

/// After herdr's client ended (`code`, its exit code; `since`, when bare `jocasta` started). A client that ended with a
/// failure lost its server, and the service in it goes too, though it may still answer for a moment: it is waited for
/// until it no longer does. A service that then doesn't answer and logged an error since the start couldn't start,
/// and its server stopped for it (0582): that is said, with exit 1. Otherwise (a detach, or Jocasta still running) the
/// client's code.
pub async fn after_surface(deps: &impl SurfaceDeps, since: i64, code: i32) -> i32 {
	let mut state = deps.service_health().await.state;
	for _ in 0..CLOSING_POLLS {
		let going = state == ServiceState::Closing || (code != 0 && state != ServiceState::Down);
		if !going {
			break;
		}
		deps.sleep(250).await;
		state = deps.service_health().await.state;
	}
	if state != ServiceState::Down {
		return code;
	}
	match deps.last_error(since) {
		Some(why) if !why.is_empty() => {
			deps.print(&format!("jocasta: Jocasta stopped while starting: {why}"));
			deps.print("jocasta: run `jocasta diagnose` for the full log.");
			1
		}
		_ => code,
	}
}

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
