//! The internal role `jocasta pane-sleep <text>` (0676): what a sleeping pane runs instead of omp. It prints `<text>` and
//! `press any key to wake`, then reads keys in raw mode; a key asks the service to wake the pane (`wake`, over the
//! bridge pipe, by the pane's own key), and the service starts omp in the pane again, which ends this process.
//!
//! The role never ends by itself on a key: herdr closes a pane whose own command ends, and with it the tab, so a sleeper
//! that exited would take its project's workspace along. It ends only when its input does (no terminal, closed).

use std::io::{self, Read, Write};

/// The role's word (hidden from the help, like `push-hook`).
pub const PANE_SLEEP_ROLE: &str = "pane-sleep";

/// The second line of a sleeper's screen.
pub const WAKE_LINE: &str = "press any key to wake";

/// `text` without control characters: a title is outside text and must not move the cursor or recolour the pane.
fn plain(text: &str) -> String {
	text.chars().filter(|c| !c.is_control()).collect()
}

/// A sleeper's screen: `text`, then [`WAKE_LINE`], each line ended for a raw terminal.
pub fn screen(text: &str) -> String {
	format!("\r\n  {}\r\n  {WAKE_LINE}\r\n", plain(text))
}

/// Reads keys from the terminal one at a time: in raw mode when there is a terminal, else bytes from stdin.
pub struct Keys {
	raw: bool,
}

impl Keys {
	/// Puts the terminal in raw mode when it can (else keys are read as bytes from stdin).
	pub fn new() -> Self {
		Self { raw: ratatui::crossterm::terminal::enable_raw_mode().is_ok() }
	}

	/// Waits for one key press: `Ok(false)` when the input ended (no more keys will come).
	pub fn wait_key(&mut self) -> io::Result<bool> {
		use ratatui::crossterm::event::{Event, KeyEventKind, read};
		if !self.raw {
			let mut byte = [0_u8; 1];
			return Ok(io::stdin().read(&mut byte)? > 0);
		}
		loop {
			if let Event::Key(key) = read()?
				&& key.kind != KeyEventKind::Release
			{
				return Ok(true);
			}
		}
	}
}

impl Default for Keys {
	fn default() -> Self {
		Self::new()
	}
}

impl Drop for Keys {
	fn drop(&mut self) {
		if self.raw {
			let _ = ratatui::crossterm::terminal::disable_raw_mode();
		}
	}
}

/// Runs the role: shows `text`, then, on each key, asks `wake` (the service, which starts omp in this pane and so ends
/// this process) and says why it didn't when it couldn't; returns the exit code once `key` says the input ended.
pub async fn run_pane_sleep<W, K, F, Fut>(args: &[String], out: &mut W, mut key: K, mut wake: F) -> i32
where
	W: Write,
	K: FnMut() -> io::Result<bool>,
	F: FnMut() -> Fut,
	Fut: Future<Output = Result<(), String>>,
{
	let _ = out.write_all(screen(&args.join(" ")).as_bytes());
	let _ = out.flush();
	while key().unwrap_or(false) {
		let _ = out.write_all(b"\r\n  waking...\r\n");
		let _ = out.flush();
		if let Err(why) = wake().await {
			let _ = out.write_all(format!("  {}\r\n  {WAKE_LINE}\r\n", plain(&why)).as_bytes());
			let _ = out.flush();
		}
	}
	0
}

#[cfg(test)]
#[path = "pane_sleep_tests.rs"]
mod tests;
