//! The brief windows (0721): `jocasta view <brief-id>` draws one stored brief in a native window of its own with iced
//! (tiny-skia, on the CPU). It reads `<data>/briefs/<id>.json`; its buttons go to the running service over the bridge
//! pipe (`frontier::client`), which stays the only writer of briefs. `model.rs` is what each section and button turns
//! into, `motion.rs` the timings, `flow.rs` the flow's canvas, `style.rs` the look, `app.rs` the window.

use std::process::{Command, ExitCode, Stdio};

use crate::frontier::brief::{BriefLinks, store};
use crate::frontier::tui::load_palette;
use crate::shared::env;
use crate::shared::install::self_command;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::config_dir;

mod app;
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

/// Starts `<this program> view <id>` on its own, not waiting for it (every OS).
fn spawn(id: &str) -> std::io::Result<()> {
	let program = self_command(None).into_iter().next().unwrap_or_default();
	let mut command = Command::new(program);
	command.args(["view", id]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
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

/// How the service shows a brief (0721): its own window, `jocasta view <id>`.
pub struct BriefWindows;

impl BriefLinks for BriefWindows {
	fn url(&self, id: &str) -> String {
		format!("{LINK}{id}")
	}

	fn open(&self, id: &str) {
		if let Err(error) = spawn(id) {
			log(LogLevel::Warn, "view", &format!("couldn't open the brief {id}: {error}"), None);
		}
	}
}
