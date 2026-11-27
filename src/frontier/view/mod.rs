//! The brief windows (0721): `jocasta view <brief-id>` draws one stored brief in a native window of its own with iced
//! (tiny-skia, on the CPU). It reads `<data>/briefs/<id>.json`; its buttons go to the running service over the bridge
//! pipe (`frontier::client`), which writes the briefs (only `jocasta brief open` without a running Jocasta stores its
//! own, 0716). `model.rs` is what each section and button turns into, `motion.rs` the timings, `flow.rs` the flow's
//! canvas, `style.rs` the look, `app.rs` the window.

use std::process::{Command, ExitCode, Stdio};

use serde_json::Value;

use crate::frontier::brief::spec::{self, Part};
use crate::frontier::brief::{BriefLinks, store};
use crate::frontier::client;
use crate::frontier::tui::load_palette;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::install::self_command;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::config_dir;
use crate::shared::service_file::NOT_RUNNING;

mod app;
pub mod board;
pub mod flow;
pub mod model;
pub mod motion;
pub mod style;

/// What a brief's link starts with: `jocasta:brief/<id>`.
pub const LINK: &str = "jocasta:brief/";

/// `jocasta view <id>`: the window until he closes it. An id that isn't stored is exit 1 with its reason.
pub fn run(id: &str) -> ExitCode {
	let Some(brief) = store::valid_id(id).then(|| store::load(id)).flatten() else {
		eprintln!("jocasta: no brief {id} is stored.");
		return ExitCode::from(1);
	};
	load_palette(&config_dir().join("palette.json"));
	match app::run(brief) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("jocasta: the window didn't open: {error}");
			ExitCode::from(1)
		}
	}
}

/// `jocasta brief open <spec.json | ->` (0716): checks the spec as `show` does, then the running service shows it as the
/// boss's `show`; without a running Jocasta it is stored here and its window opened all the same (a `show` brief has
/// no buttons), and only a section filled from a thread is refused. The `Shown: <title>` line, or a `show:` refusal
/// (exit 2).
pub async fn open_spec(source: &str) -> Result<String> {
	let text = if source == "-" { std::io::read_to_string(std::io::stdin()) } else { std::fs::read_to_string(source) }
		.map_err(|error| Error::user(format!("show: can't read {source}: {error}")))?;
	let Ok(Value::Object(given)) = serde_json::from_str::<Value>(&text) else {
		return Err(Error::user("show: the spec isn't a JSON object: {\"title\": …, \"sections\": […]}."));
	};
	let spec = spec::read(&given)?;
	match client::brief_open(&given).await {
		Ok(shown) => return Ok(shown),
		Err(why) if why != NOT_RUNNING => return Err(Error::user(why)),
		Err(_) => {}
	}
	if let Some((number, thread)) = spec.references().next() {
		return Err(Error::user(format!(
			"show: section {number} is filled from the thread {thread}, and Jocasta isn't running to fill it."
		)));
	}
	let sections =
		spec.sections.into_iter().filter_map(|part| if let Part::Given(section) = part { Some(section) } else { None });
	let brief = spec::show_brief(spec.title, sections.collect());
	store::save(&brief)?;
	spawn(&["view", &brief.id]).map_err(|error| Error::other(format!("the window didn't open: {error}")))?;
	Ok(format!("Shown: {}", brief.title))
}

/// Starts `<this program> <args>` on its own, not waiting for it (every OS): a brief's window or the board's.
fn spawn(args: &[&str]) -> std::io::Result<()> {
	let program = self_command(None).into_iter().next().unwrap_or_default();
	let mut command = Command::new(program);
	command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
	// Its own process group: Ctrl+C in Jocasta's terminal doesn't close his windows.
	#[cfg(unix)]
	std::os::unix::process::CommandExt::process_group(&mut command, 0);
	// No console window for it on Windows (CREATE_NO_WINDOW).
	#[cfg(windows)]
	std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
	env::apply_to(&mut command);
	let mut child = command.spawn()?;
	// Reaped in the background, so the child never lingers as a zombie.
	std::thread::spawn(move || child.wait());
	Ok(())
}

/// How the service shows a brief (0721) and the board (0718): their own windows, `jocasta view <id>` and
/// `jocasta board`.
pub struct BriefWindows;

impl BriefLinks for BriefWindows {
	fn url(&self, id: &str) -> String {
		format!("{LINK}{id}")
	}

	fn open(&self, id: &str) {
		if let Err(error) = spawn(&["view", id]) {
			log(LogLevel::Warn, "view", &format!("couldn't open the brief {id}: {error}"), None);
		}
	}

	fn open_board(&self) {
		if let Err(error) = spawn(board::WINDOW) {
			log(LogLevel::Warn, "view", &format!("couldn't open the board: {error}"), None);
		}
	}
}
