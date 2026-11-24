//! The install layout (appendix I): `<install dir>/jocasta.exe`, `config/`, `conpty/` on Windows and
//! `install.json` written by the installer. One resolver for
//! every caller: bare `jocasta`, the service it starts, setup and a source run.

use std::path::{Path, PathBuf};

/// The repo root in a source run (the package's own folder).
pub const REPO: &str = env!("CARGO_MANIFEST_DIR");

const JOCASTA_EXE: &str = if cfg!(windows) { "jocasta.exe" } else { "jocasta" };

/// The installer's marker in the install dir: the folder it installed into.
pub const INSTALL_MARKER: &str = "install.json";

fn running_exe(exec_path: Option<&Path>) -> PathBuf {
	exec_path.map(Path::to_path_buf).or_else(|| std::env::current_exe().ok()).unwrap_or_default()
}

/// The folder holding `jocasta.exe` and the shipped `config/` beside it: the running executable's own (`exec_path`,
/// else this process's); `None` in a source run. A cargo build's `target/<profile>/` holds the exe but never
/// `config/`, so a source run tells itself from an install as the TypeScript did by Bun's own exec path.
pub fn install_dir(exec_path: Option<&Path>) -> Option<PathBuf> {
	let exe_dir = running_exe(exec_path).parent()?.to_path_buf();
	(exe_dir.join(JOCASTA_EXE).exists() && exe_dir.join("config").is_dir()).then_some(exe_dir)
}

/// The shipped `config/` (boss manual, boss.yml, templates): the install's, or the repo's in a source run.
pub fn shipped_config_dir(exec_path: Option<&Path>) -> PathBuf {
	match install_dir(exec_path) {
		Some(dir) => dir.join("config"),
		None => Path::new(REPO).join("config"),
	}
}

/// How another program starts this Jocasta: the install's `jocasta.exe`; in a source run the running executable
/// itself (the TypeScript's `[bun, <repo>/src/entry.ts]` has no Rust twin).
pub fn self_command(exec_path: Option<&Path>) -> Vec<PathBuf> {
	match install_dir(exec_path) {
		Some(dir) => vec![dir.join(JOCASTA_EXE)],
		None => vec![running_exe(exec_path)],
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_dir;

	#[test]
	fn the_install_dir_is_jocasta_exes_folder_with_its_config_and_a_source_run_has_none() {
		let root = temp_dir("install");
		let dir = root.path().join("Programs").join("jocasta");
		std::fs::create_dir_all(dir.join("config")).unwrap();
		std::fs::write(dir.join(JOCASTA_EXE), "").unwrap();
		let exe = dir.join(JOCASTA_EXE);
		let elsewhere = root.path().join("elsewhere").join("bun.exe");
		assert_eq!(install_dir(Some(&exe)), Some(dir.clone()));
		assert_eq!(shipped_config_dir(Some(&exe)), dir.join("config"));
		assert_eq!(install_dir(Some(&elsewhere)), None);
		assert_eq!(shipped_config_dir(Some(&elsewhere)), Path::new(REPO).join("config"));
		// omp and bare `jocasta` start the install's jocasta.exe (0466, 0552).
		assert_eq!(self_command(Some(&exe)), vec![exe]);
		assert_eq!(self_command(Some(&elsewhere)), vec![elsewhere]);
		// A cargo build: the exe without `config/` beside it is a source run, with the repo's config.
		let build = root.path().join("target").join("debug");
		std::fs::create_dir_all(&build).unwrap();
		std::fs::write(build.join(JOCASTA_EXE), "").unwrap();
		let built = build.join(JOCASTA_EXE);
		assert_eq!(install_dir(Some(&built)), None);
		assert_eq!(shipped_config_dir(Some(&built)), Path::new(REPO).join("config"));
		assert_eq!(self_command(Some(&built)), vec![built]);
	}
}
