//! The first-run checks (0703): once the service has started, a background task looks for what a fresh machine may
//! lack (omp, its login, a new enough git, Linux's `notify-send` and `xdg-open`, a terminal that reports Alt and
//! Shift) and says each problem once as a notification; `today`'s header keeps saying it for the rest of the run.

use crate::frontier::protocol::{ToastLevel, ToastReason};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::events::Toast;
use crate::setup::setup::git_too_old;
use crate::shared::process::{RunOptions, on_path, run_program};

/// What the checks found; a check this OS skips is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
	/// omp is on PATH.
	pub omp: bool,
	/// omp has a login: `omp usage` didn't fail asking for `/login`. `None` without omp.
	pub signed_in: Option<bool>,
	/// `git --version`'s output; `None` when git can't run.
	pub git: Option<String>,
	/// `notify-send` is on PATH (Linux).
	pub notify_send: Option<bool>,
	/// `xdg-open` is on PATH (Linux).
	pub xdg_open: Option<bool>,
	/// His terminal answered the kitty keyboard protocol's query (Linux and macOS).
	pub keys: Option<bool>,
}

/// Each problem's text, in the order the checks run.
pub fn problems(facts: &Facts) -> Vec<String> {
	let mut found = Vec::new();
	if !facts.omp {
		found.push("omp isn't on PATH: install omp, run omp, then /login".to_owned());
	}
	if facts.signed_in == Some(false) {
		found.push("omp isn't signed in: type /login in the boss pane".to_owned());
	}
	if let Some(has) = git_too_old(facts.git.as_deref()) {
		found.push(format!("{has}: install git 2.54 or newer, or pushes to main, master and develop go unasked"));
	}
	if facts.notify_send == Some(false) {
		found.push("notify-send isn't installed: install libnotify, or Jocasta's notifications stay off".to_owned());
	}
	if facts.xdg_open == Some(false) {
		found.push("xdg-open isn't installed: install xdg-utils, or links and screenshots won't open".to_owned());
	}
	if facts.keys == Some(false) {
		found.push(
			"this terminal doesn't send Alt and Shift keys Jocasta needs; use foot, kitty, WezTerm or Ghostty"
				.to_owned(),
		);
	}
	found
}

/// Whether `omp usage`'s run asks for a login: it exits non-zero saying `/login` or `No credentials` (omp 19.2.1).
pub fn asks_for_login(code: Option<i32>, output: &str) -> bool {
	code != Some(0) && (output.contains("/login") || output.contains("No credentials"))
}

/// Runs `argv` for at most a minute: its exit code and stdout and stderr together; `None` when it couldn't start.
async fn output_of(argv: &[&str]) -> Option<(Option<i32>, String)> {
	let run = run_program(argv, RunOptions { timeout_ms: Some(60_000), ..RunOptions::default() }).await.ok()?;
	Some((run.code, format!("{}\n{}", run.stdout, run.stderr)))
}

/// Looks at this machine; `keys` is the terminal's answer, asked before the shell took the console.
pub async fn gather(keys: bool) -> Facts {
	let omp = on_path("omp", None);
	let signed_in = match &omp {
		Some(omp) => {
			let omp = omp.to_string_lossy();
			// A run that can't start or ends otherwise (offline, a timeout) says nothing about the login.
			Some(output_of(&[&omp, "usage"]).await.is_none_or(|(code, output)| !asks_for_login(code, &output)))
		}
		None => None,
	};
	let git = output_of(&["git", "--version"]).await.filter(|(code, _)| *code == Some(0)).map(|(_, output)| output);
	let linux = cfg!(target_os = "linux");
	Facts {
		omp: omp.is_some(),
		signed_in,
		git,
		notify_send: linux.then(|| on_path("notify-send", None).is_some()),
		xdg_open: linux.then(|| on_path("xdg-open", None).is_some()),
		// Windows' console reports Alt and Shift without the protocol.
		keys: cfg!(unix).then_some(keys),
	}
}

/// The checks, run once off the start (0703): each problem one notification, and all of them in `today`'s header.
pub async fn check(c: std::sync::Weak<ServiceContext>, keys: bool) {
	let found = problems(&gather(keys).await);
	let Some(c) = c.upgrade() else { return };
	for text in &found {
		c.events.send(Toast::new(ToastReason::Event, ToastLevel::Warning, text.clone()).into());
	}
	c.status.set_problems(found);
}

#[cfg(test)]
#[path = "first_run_tests.rs"]
mod tests;
