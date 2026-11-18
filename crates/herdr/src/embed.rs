//! The fork's hooks for Jocasta, which runs herdr inside its own `jocasta` binary. A new file of the fork's (FORK.md):
//! herdr's own code reads these only where FORK.md lists a seam.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

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

/// What a server extension is told when the server listens.
#[derive(Clone)]
pub struct ServerHost {
    /// The server's API socket (on Windows the path its named pipe is named after).
    pub api_socket: PathBuf,
    /// Stops the server as `server stop` does, from inside its process; callable from any thread.
    pub stop: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for ServerHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHost")
            .field("api_socket", &self.api_socket)
            .finish_non_exhaustive()
    }
}

/// Something that runs beside herdr's server, in its process: Jocasta's service. Its calls come on herdr's server
/// thread, so each must return quickly; the extension does its work on threads of its own.
pub trait ServerExtension: Send {
    /// The server listens and is about to run its loop; its plugins' startup hooks have run.
    fn started(&mut self, host: ServerHost);
    /// The last client of his terminal left (a quit or a detach); the server keeps running.
    fn last_client_left(&mut self);
    /// The server's loop has ended; the process ends once this returns.
    fn stopped(&mut self);
}

static EXTENSION: Mutex<Option<Box<dyn ServerExtension>>> = Mutex::new(None);

/// Sets the extension a server started by this process runs; call it before [`crate::main`].
pub fn set_server_extension(extension: Box<dyn ServerExtension>) {
    if let Ok(mut slot) = EXTENSION.lock() {
        *slot = Some(extension);
    }
}

fn with_extension(call: impl FnOnce(&mut dyn ServerExtension)) {
    if let Ok(mut slot) = EXTENSION.lock() {
        if let Some(extension) = slot.as_mut() {
            call(extension.as_mut());
        }
    }
}

pub(crate) fn server_started(api_socket: PathBuf, stop: Arc<dyn Fn() + Send + Sync>) {
    with_extension(|extension| extension.started(ServerHost { api_socket, stop }));
}

pub(crate) fn last_client_left() {
    with_extension(|extension| extension.last_client_left());
}

pub(crate) fn server_stopped() {
    let taken = EXTENSION.lock().ok().and_then(|mut slot| slot.take());
    if let Some(mut extension) = taken {
        extension.stopped();
    }
}

type ExitHook = Box<dyn Fn(i32) -> i32 + Send + Sync>;

static EXIT_HOOK: OnceLock<ExitHook> = OnceLock::new();

/// Sets what runs before herdr's client ends the process with a failure (its server stopped, or none could start):
/// the hook gets herdr's exit code and returns the process's. Only the first call counts.
pub fn set_exit_hook(hook: ExitHook) {
    let _ = EXIT_HOOK.set(hook);
}

/// Ends the process with `code`, after the embedder's exit hook (fork seam, FORK.md: bare `jocasta` says why Jocasta
/// stopped once herdr's client ended).
pub(crate) fn exit(code: i32) -> ! {
    let code = EXIT_HOOK.get().map_or(code, |hook| hook(code));
    std::process::exit(code)
}
