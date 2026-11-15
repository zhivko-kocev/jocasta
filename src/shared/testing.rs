//! Test support (Rust only; the TypeScript has `test/preload.ts` and `useTempJocasta`). Every unit test process
//! starts with the Jocasta and omp roots in a sandbox under the OS temp folder, so nothing a test runs reaches his
//! real folders; a test that reads or writes those roots holds [`temp_jocasta`], which gives it fresh roots of its
//! own and keeps other such tests waiting until it ends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

use crate::shared::env::overlay;

/// The variables that name Jocasta's and omp's roots, as `test/preload.ts` sets them.
const ROOT_VARS: [&str; 4] = ["JOCASTA_CODING_AGENT_DIR", "JOCASTA_DATA_DIR", "PI_CODING_AGENT_DIR", "PI_CONFIG_DIR"];

static SANDBOX: LazyLock<PathBuf> = LazyLock::new(|| {
	let root = std::env::temp_dir().join(format!("jocasta-sandbox-{}", std::process::id()));
	std::fs::create_dir_all(&root).expect("making the test sandbox");
	root
});

/// This test process's sandbox folder.
pub fn sandbox() -> &'static Path {
	&SANDBOX
}

/// The overlay's first values: the four roots under `root` (`PI_CONFIG_DIR` relative to the home folder, as omp
/// reads it).
pub(crate) fn sandbox_values() -> HashMap<String, Option<String>> {
	roots_under(sandbox()).into_iter().map(|(name, value)| (name.to_owned(), Some(value))).collect()
}

fn roots_under(root: &Path) -> [(&'static str, String); 4] {
	let omp = root.join("omp");
	let home = std::env::home_dir().unwrap_or_default();
	let config_rel = pathdiff(&omp, &home);
	[
		(ROOT_VARS[0], root.join("config").display().to_string()),
		(ROOT_VARS[1], root.join("data").display().to_string()),
		(ROOT_VARS[2], omp.join("agent").display().to_string()),
		(ROOT_VARS[3], config_rel),
	]
}

/// `to` relative to `from`, with `..` steps (omp joins `PI_CONFIG_DIR` to the home folder).
fn pathdiff(to: &Path, from: &Path) -> String {
	let to: Vec<_> = to.components().collect();
	let from: Vec<_> = from.components().collect();
	let common = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
	let mut rel = PathBuf::new();
	for _ in common..from.len() {
		rel.push("..");
	}
	for part in &to[common..] {
		rel.push(part.as_os_str());
	}
	rel.display().to_string()
}

fn env_lock() -> MutexGuard<'static, ()> {
	static LOCK: Mutex<()> = Mutex::new(());
	LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A folder under the sandbox, removed when dropped.
pub struct TempDir {
	path: PathBuf,
}

impl TempDir {
	pub fn path(&self) -> &Path {
		&self.path
	}
}

impl Drop for TempDir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.path);
	}
}

/// A fresh, empty folder for one test.
pub fn temp_dir(prefix: &str) -> TempDir {
	let path = sandbox().join(format!("{prefix}-{}", uuid::Uuid::new_v4().simple()));
	std::fs::create_dir_all(&path).expect("making a test folder");
	TempDir { path }
}

/// Fresh Jocasta and omp roots for one test (the TypeScript's `useTempJocasta`), held until dropped; other tests that
/// take one wait. [`TempJocasta::set_var`] sets any other variable for the test's length.
pub struct TempJocasta {
	dir: TempDir,
	before: Vec<(String, Option<Option<String>>)>,
	_lock: MutexGuard<'static, ()>,
}

pub fn temp_jocasta() -> TempJocasta {
	let lock = env_lock();
	let dir = temp_dir("jocasta");
	let mut temp = TempJocasta { dir, before: Vec::new(), _lock: lock };
	for (name, value) in roots_under(temp.dir.path()) {
		temp.set_var(name, Some(&value));
	}
	temp
}

impl TempJocasta {
	/// The test's root: `config/`, `data/` and `omp/` are under it.
	pub fn root(&self) -> &Path {
		self.dir.path()
	}

	/// Sets (or with `None` unsets) a variable until the test ends.
	pub fn set_var(&mut self, name: &str, value: Option<&str>) {
		let before = overlay::set(name, value.map(str::to_owned));
		self.before.push((name.to_owned(), before));
	}
}

impl Drop for TempJocasta {
	fn drop(&mut self) {
		for (name, before) in self.before.drain(..).rev() {
			overlay::restore(&name, before);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::env;

	#[test]
	fn the_roots_point_into_a_fresh_folder_under_the_sandbox() {
		// The value outside a `temp_jocasta` can't be compared here: another test may hold its own at that moment.
		let base = sandbox_values().remove("JOCASTA_DATA_DIR").flatten().unwrap();
		assert!(Path::new(&base).starts_with(sandbox()));
		{
			let mut temp = temp_jocasta();
			let data = env::var("JOCASTA_DATA_DIR").unwrap();
			assert!(Path::new(&data).starts_with(temp.root()));
			assert_ne!(data, base);
			// A name no other test sets, so its restore is visible here without racing them.
			temp.set_var("JOCASTA_TESTING_RESTORE_PROBE", Some("k"));
			assert_eq!(env::var("JOCASTA_TESTING_RESTORE_PROBE").as_deref(), Some("k"));
		}
		assert_eq!(env::var("JOCASTA_TESTING_RESTORE_PROBE"), None);
	}

	#[test]
	fn pi_config_dir_is_relative_to_home() {
		let home = std::env::home_dir().unwrap();
		let rel = env::var("PI_CONFIG_DIR").unwrap();
		assert!(home.join(rel).ends_with("omp"));
	}
}
