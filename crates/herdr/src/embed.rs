//! The fork's hooks for Jocasta, which runs herdr inside its own `jocasta` binary. A new file of the fork's (FORK.md):
//! herdr's own code reads these only where FORK.md lists a seam.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Where herdr keeps its files inside Jocasta, in place of `~/.config/herdr` and its kin.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// herdr's `config_dir`: sessions, sockets, the session file, plugins and their registry.
    pub config: PathBuf,
    /// herdr's `state_dir`: plugin state and the client's preferences.
    pub state: PathBuf,
    /// herdr's `config_path`, the config file; `HERDR_CONFIG_PATH` still wins over it.
    pub config_file: PathBuf,
}

static DIRS: OnceLock<Dirs> = OnceLock::new();

/// Sets herdr's dirs for this process; call it before [`crate::main`]. Only the first call counts.
pub fn set_dirs(dirs: Dirs) {
    let _ = DIRS.set(dirs);
}

/// The dirs [`set_dirs`] gave, if any.
pub(crate) fn dirs() -> Option<&'static Dirs> {
    DIRS.get()
}

/// herdr's version, the one plugins' `min_herdr_version` is checked against.
pub fn version() -> String {
    crate::build_info::version()
}
