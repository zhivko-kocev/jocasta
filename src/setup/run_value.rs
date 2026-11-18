//! The logon Run value an install wrote (0367): uninstall removes it while it still starts that install.

use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;

use crate::shared::env;

pub const RUN_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// `reg.exe`, with no console window.
fn reg() -> Command {
	let mut command = Command::new("reg.exe");
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		command.creation_flags(0x0800_0000);
	}
	env::apply_to(&mut command);
	command
}

/// The data of Run value `name` in `reg query`'s output, when it lists one.
fn run_value_data(stdout: &str, name: &str) -> Option<String> {
	let line = stdout.lines().find(|line| line.trim_start().starts_with(&format!("{name} ")))?;
	static REG_SZ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+REG_SZ\s+").expect("a fixed pattern"));
	REG_SZ.splitn(line, 2).nth(1).map(|data| data.trim().to_owned())
}

/// The Run value's data now, or `None` when there is none.
pub fn read_run_value(name: &str) -> Option<String> {
	let output = reg().args(["query", RUN_KEY, "/v", name]).stdin(Stdio::null()).output().ok()?;
	if !output.status.success() {
		return None;
	}
	run_value_data(&String::from_utf8_lossy(&output.stdout), name)
}

/// Deletes Run value `name`; true once it is gone.
pub fn delete_run_value(name: &str) -> bool {
	reg()
		.args(["delete", RUN_KEY, "/v", name, "/f"])
		.stdin(Stdio::null())
		.output()
		.is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_value_is_read_from_reg_querys_listing() {
		let out = format!("\r\n{RUN_KEY}\r\n    Jocasta    REG_SZ    \"C:\\J\\app\\bin\\launcher.exe\"\r\n\r\n");
		assert_eq!(run_value_data(&out, "Jocasta").as_deref(), Some("\"C:\\J\\app\\bin\\launcher.exe\""));
		assert_eq!(run_value_data(&out, "Other"), None);
		assert_eq!(run_value_data("    Jocasta    REG_DWORD    0x1\r\n", "Jocasta"), None);
	}
}
