//! Where Jocasta's and omp's folders and files are.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::shared::env;
use crate::shared::fsx::{normalize, private_dir};

fn homedir() -> PathBuf {
	std::env::home_dir().unwrap_or_default()
}

/// A leading `~` as the home folder, for the `JOCASTA_*_DIR` and `PI_CODING_AGENT_DIR` overrides (0098).
fn expand_tilde(path: &str) -> PathBuf {
	if path == "~" {
		return homedir();
	}
	if let Some(rest) = path.strip_prefix("~/").or_else(|| if cfg!(windows) { path.strip_prefix("~\\") } else { None })
	{
		return normalize(&homedir().join(rest));
	}
	PathBuf::from(path)
}

/// Folders made private in this process: each is made (or tightened) once.
static MADE: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// `dir`, made 0700 on Linux and macOS the first time this process names it (0484); on Windows it is only named. A
/// folder that can't be made is left to the caller that writes in it, which says why.
fn private_on_posix(dir: PathBuf) -> PathBuf {
	if cfg!(windows) {
		return dir;
	}
	let mut made = MADE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	if !made.contains(&dir) && private_dir(&dir, None).is_ok() {
		made.insert(dir.clone());
	}
	dir
}

/// Jocasta's config dir (0054, 0387); `JOCASTA_CODING_AGENT_DIR` overrides it (0098). 0700 on Linux and macOS (0484).
pub fn config_dir() -> PathBuf {
	private_on_posix(match env::var_nonempty("JOCASTA_CODING_AGENT_DIR") {
		Some(value) => expand_tilde(&value),
		None => homedir().join(".jocasta").join("agent"),
	})
}

/// The OS data dir (0095); `JOCASTA_DATA_DIR` overrides it (0098). 0700 on Linux and macOS (0484).
pub fn data_dir() -> PathBuf {
	private_on_posix(match env::var_nonempty("JOCASTA_DATA_DIR") {
		Some(value) => expand_tilde(&value),
		None => default_data_dir(),
	})
}

/// The data dir without `JOCASTA_DATA_DIR`: `%LOCALAPPDATA%\jocasta\data` on Windows, `~/Library/Application Support`
/// on macOS, `$XDG_DATA_HOME` (or `~/.local/share`) elsewhere (0095). An empty variable counts as unset.
pub fn default_data_dir() -> PathBuf {
	let home = homedir();
	if cfg!(windows) {
		let base = env::var_nonempty("LOCALAPPDATA").map_or_else(|| home.join("AppData").join("Local"), PathBuf::from);
		return base.join("jocasta").join("data");
	}
	if cfg!(target_os = "macos") {
		return home.join("Library").join("Application Support").join("jocasta").join("data");
	}
	let base = env::var_nonempty("XDG_DATA_HOME").map_or_else(|| home.join(".local").join("share"), PathBuf::from);
	base.join("jocasta").join("data")
}

/// omp's agent dir (0387), as omp resolves it (`@oh-my-pi/pi-utils` `dirs.ts` `getAgentDir`): a named `profile`'s is
/// `<root>/profiles/<name>/agent` whatever `PI_CODING_AGENT_DIR` says; else `PI_CODING_AGENT_DIR` (tilde-expanded),
/// else `<root>/agent`; `<root>` is `~/.omp`, or `~/<PI_CONFIG_DIR>`. Setup writes there; the service reads its
/// `sessions/`.
pub fn omp_agent_dir(profile: Option<&str>) -> PathBuf {
	let config = env::var_nonempty("PI_CONFIG_DIR").unwrap_or_else(|| ".omp".to_owned());
	let root = normalize(&homedir().join(config));
	if let Some(profile) = profile {
		return root.join("profiles").join(profile).join("agent");
	}
	match env::var_nonempty("PI_CODING_AGENT_DIR") {
		Some(value) => expand_tilde(&value),
		None => root.join("agent"),
	}
}

/// `~/.jocasta/agent/projects.yaml` (0109).
pub fn projects_file() -> PathBuf {
	config_dir().join("projects.yaml")
}

/// `<data>/catalog.db` (0096).
pub fn catalog_path() -> PathBuf {
	data_dir().join("catalog.db")
}

/// `<data>/projects/<pid>` (0096).
pub fn project_dir(project_id: &str) -> PathBuf {
	data_dir().join("projects").join(project_id)
}

/// `<data>/projects/<pid>/lessons` (0459): the project's lessons, outside client repos. Its `SKILL.md` is the index
/// omp lists as the skill `lessons`; each lesson is its own file beside it.
pub fn lessons_dir(project_id: &str) -> PathBuf {
	project_dir(project_id).join("lessons")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::{temp_dir, temp_jocasta};

	#[cfg(unix)]
	#[test]
	fn data_and_config_folders_are_0700_on_linux_and_macos_made_or_tightened() {
		use std::os::unix::fs::PermissionsExt;
		let dir = temp_dir("paths");
		let data = dir.path().join("data");
		let config = dir.path().join("config");
		std::fs::create_dir(&config).unwrap();
		std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o755)).unwrap();
		let mut temp = temp_jocasta();
		temp.set_var("JOCASTA_DATA_DIR", Some(&data.display().to_string()));
		temp.set_var("JOCASTA_CODING_AGENT_DIR", Some(&config.display().to_string()));
		assert_eq!(data_dir(), data);
		assert_eq!(config_dir(), config);
		assert_eq!(std::fs::metadata(&data).unwrap().permissions().mode() & 0o777, 0o700);
		assert_eq!(std::fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o700);
	}

	#[cfg(windows)]
	#[test]
	fn on_windows_data_dir_only_names_the_folder_nothing_is_made() {
		let dir = temp_dir("paths");
		let unmade = dir.path().join("unmade");
		let mut temp = temp_jocasta();
		temp.set_var("JOCASTA_DATA_DIR", Some(&unmade.display().to_string()));
		assert_eq!(data_dir(), unmade);
		assert!(!unmade.exists());
	}

	#[test]
	fn names_hang_off_the_data_and_config_folders() {
		let mut temp = temp_jocasta();
		let root = temp.root().to_path_buf();
		temp.set_var("JOCASTA_DATA_DIR", Some(&root.join("d").display().to_string()));
		temp.set_var("JOCASTA_CODING_AGENT_DIR", Some(&root.join("c").display().to_string()));
		assert_eq!(catalog_path(), root.join("d").join("catalog.db"));
		assert_eq!(projects_file(), root.join("c").join("projects.yaml"));
		assert_eq!(lessons_dir("p-1"), root.join("d").join("projects").join("p-1").join("lessons"));
	}

	#[test]
	fn omp_agent_dir_follows_the_profile_then_the_override_then_the_root() {
		let mut temp = temp_jocasta();
		let home = homedir();
		temp.set_var("PI_CODING_AGENT_DIR", Some("~/elsewhere"));
		temp.set_var("PI_CONFIG_DIR", Some(".omp-x"));
		assert_eq!(omp_agent_dir(None), home.join("elsewhere"));
		assert_eq!(omp_agent_dir(Some("work")), home.join(".omp-x").join("profiles").join("work").join("agent"));
		temp.set_var("PI_CODING_AGENT_DIR", None);
		assert_eq!(omp_agent_dir(None), home.join(".omp-x").join("agent"));
	}
}
