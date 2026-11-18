//! The fork's hooks for Jocasta, which runs herdr inside its own `jocasta` binary. A new file of the fork's (FORK.md):
//! herdr's own code reads these only where FORK.md lists a seam.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::api::schema::EventKind;

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

/// The prefix of every message kind an extension and its shell extension trade over herdr's client connection, as
/// `EndpointControl` messages (herdr's own way to add messages without changing its protocol).
pub const KIND_PREFIX: &str = "jocasta.";

/// Where an extension's message goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// One client, by the id [`ServerExtension::client_joined`] gave.
    Client(u64),
    /// Every client-rendered shell.
    Shells,
}

/// A message for herdr's clients: `kind` starts with [`KIND_PREFIX`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    pub target: Target,
    pub kind: String,
    pub data: String,
}

type SendFn = Arc<dyn Fn(Outbound) + Send + Sync>;

/// What a server extension is told when the server listens.
#[derive(Clone)]
pub struct ServerHost {
    /// The server's API socket (on Windows the path its named pipe is named after).
    pub api_socket: PathBuf,
    /// Stops the server as `server stop` does, from inside its process; callable from any thread.
    pub stop: Arc<dyn Fn() + Send + Sync>,
    send: SendFn,
}

impl ServerHost {
    /// A host whose messages go to `send` (herdr's server loop; a test's own).
    pub fn new(api_socket: PathBuf, stop: Arc<dyn Fn() + Send + Sync>, send: SendFn) -> Self {
        Self {
            api_socket,
            stop,
            send,
        }
    }

    /// Sends `data` as a `kind` message to `target`, from any thread; dropped once the server stopped.
    pub fn send(&self, target: Target, kind: &str, data: String) {
        (self.send)(Outbound {
            target,
            kind: kind.to_owned(),
            data,
        });
    }

    /// Resolves at herdr's next change Jocasta reads (see [`herdr_event`]); changes while nothing waits are one wake.
    pub async fn herdr_changed(&self) {
        CHANGED.notified().await;
    }
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
    /// A client-rendered shell attached; `pid` is the process at the other end of its connection, as the OS reports it
    /// (`None` where it can't be read).
    fn client_joined(&mut self, _client: u64, _pid: Option<u32>) {}
    /// A client-rendered shell left.
    fn client_left(&mut self, _client: u64) {}
    /// A client sent a `kind` message (one starting with [`KIND_PREFIX`]).
    fn client_message(&mut self, _client: u64, _kind: &str, _data: &str) {}
    /// His key bound to plugin action `jocasta.<action>`, with the focused pane's id; true when the extension took it
    /// (then no installed plugin is looked for).
    fn action(&mut self, _action: &str, _pane_id: Option<&str>) -> bool {
        false
    }
}

static EXTENSION: Mutex<Option<Box<dyn ServerExtension>>> = Mutex::new(None);

/// Sets the extension a server started by this process runs; call it before [`crate::main`].
pub fn set_server_extension(extension: Box<dyn ServerExtension>) {
    if let Ok(mut slot) = EXTENSION.lock() {
        *slot = Some(extension);
    }
}

fn with_extension<T>(call: impl FnOnce(&mut dyn ServerExtension) -> T) -> Option<T> {
    let mut slot = EXTENSION.lock().ok()?;
    slot.as_mut().map(|extension| call(extension.as_mut()))
}

pub(crate) fn server_started(host: ServerHost) {
    with_extension(|extension| extension.started(host));
}

pub(crate) fn last_client_left() {
    with_extension(|extension| extension.last_client_left());
}

/// Woken by herdr's events that change what Jocasta reads of it.
static CHANGED: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);

/// One of herdr's events, as its event hub records it: a workspace, tab or pane created, closed or focused, a pane's
/// process exiting, or its agent detected or changing state wakes [`ServerHost::herdr_changed`]. Output, layout, moves,
/// renames and other updates don't.
pub(crate) fn herdr_event(kind: EventKind) {
    if matches!(
        kind,
        EventKind::WorkspaceCreated
            | EventKind::WorkspaceClosed
            | EventKind::WorkspaceFocused
            | EventKind::TabFocused
            | EventKind::PaneCreated
            | EventKind::PaneClosed
            | EventKind::PaneExited
            | EventKind::PaneFocused
            | EventKind::PaneAgentDetected
            | EventKind::PaneAgentStatusChanged
    ) {
        CHANGED.notify_one();
    }
}

/// Each shell's peer process, from its handshake until it attaches.
static PEERS: Mutex<Vec<(u64, Option<u32>)>> = Mutex::new(Vec::new());

/// The process at the other end of a client's connection, as the OS reports it: Windows' named pipe client, Linux's
/// `SO_PEERCRED`, macOS's `LOCAL_PEERPID`; `None` where it can't be read.
#[allow(
    clippy::useless_conversion,
    reason = "the pid is a u32 on Windows and a pid_t elsewhere"
)]
pub(crate) fn peer_pid(stream: &crate::ipc::LocalStream) -> Option<u32> {
    use interprocess::local_socket::traits::StreamCommon as _;
    if let Some(pid) = stream.peer_creds().ok().and_then(|creds| creds.pid()) {
        return u32::try_from(pid).ok();
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::{AsFd as _, AsRawFd as _};
        let crate::ipc::LocalStream::UdSocket(socket) = stream;
        let mut pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        // SAFETY: getsockopt writes at most `len` bytes into `pid`, a live `pid_t`, for an open socket.
        let read = unsafe {
            libc::getsockopt(
                socket.as_fd().as_raw_fd(),
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&mut pid as *mut libc::pid_t).cast(),
                &mut len,
            )
        };
        if read == 0 {
            return u32::try_from(pid).ok();
        }
    }
    None
}

/// Keeps a shell's peer process until it attaches.
pub(crate) fn note_peer(client: u64, pid: Option<u32>) {
    if let Ok(mut peers) = PEERS.lock() {
        peers.retain(|(known, _)| *known != client);
        peers.push((client, pid));
    }
}

fn take_peer(client: u64) -> Option<u32> {
    let mut peers = PEERS.lock().ok()?;
    let at = peers.iter().position(|(known, _)| *known == client)?;
    peers.remove(at).1
}

pub(crate) fn client_joined(client: u64) {
    let pid = take_peer(client);
    with_extension(|extension| extension.client_joined(client, pid));
}

pub(crate) fn client_left(client: u64) {
    take_peer(client);
    with_extension(|extension| extension.client_left(client));
}

pub(crate) fn client_message(client: u64, kind: &str, data: &str) {
    with_extension(|extension| extension.client_message(client, kind, data));
}

/// Whether the extension took plugin action `action_id` (`jocasta.<action>`).
pub(crate) fn plugin_action(action_id: &str, pane_id: Option<&str>) -> bool {
    let Some(action) = action_id.strip_prefix(KIND_PREFIX) else {
        return false;
    };
    with_extension(|extension| extension.action(action, pane_id)).unwrap_or(false)
}

pub(crate) fn server_stopped() {
    let taken = EXTENSION.lock().ok().and_then(|mut slot| slot.take());
    if let Some(mut extension) = taken {
        extension.stopped();
    }
}

/// The command a key binds (`type = "plugin_action"`) to give the dock his keys; herdr's client takes it itself.
pub const DOCK_COMMAND: &str = "jocasta.today";

/// What a shell extension's key or paste did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// Messages for the server's extension, as `(kind, data)`.
    pub send: Vec<(String, String)>,
    /// What it draws changed.
    pub redraw: bool,
    /// The dock lets go of his keys.
    pub release: bool,
    /// The key isn't the dock's: it lets go of his keys, and the key goes on as if the dock had none.
    pub pass: bool,
}

/// What Jocasta draws in herdr's client shell and takes keys for: the dock beside the panes of one workspace, and a
/// question over everything. Its calls come on the client's thread, so each must return quickly.
pub trait ShellExtension: Send {
    /// A message from the server's extension: whether what it draws changed, and what goes back.
    fn message(&mut self, kind: &str, data: &str) -> Reply;
    /// Whether the dock shows beside the panes of `workspace_id`, the focused workspace.
    fn docks(&self, workspace_id: &str) -> bool;
    /// Draws the dock in `area`; `focused` while it has his keys.
    fn draw_dock(&mut self, buffer: &mut Buffer, area: Rect, focused: bool);
    /// A key while the dock has his keys.
    fn dock_key(&mut self, key: KeyEvent) -> Reply;
    /// Whether a question is up: it takes every key.
    fn asking(&self) -> bool;
    /// Draws the question over `area` (the whole terminal); returns the part it covers.
    fn draw_question(&mut self, buffer: &mut Buffer, area: Rect) -> Rect;
    /// A key while a question is up.
    fn question_key(&mut self, key: KeyEvent) -> Reply;
    /// A paste while a question is up.
    fn question_paste(&mut self, text: &str) -> Reply;
    /// Whether what it draws changed with time (its clock, a note that aged out).
    fn tick(&mut self) -> bool;
    /// Its segment at the left of the tab bar's right side, beside herdr's own; `None` for none.
    fn status(&self) -> Option<String>;
}

static SHELL: Mutex<Option<Box<dyn ShellExtension>>> = Mutex::new(None);

/// Sets the extension herdr's client shell runs in this process; call it before [`crate::main`].
pub fn set_shell_extension(extension: Box<dyn ShellExtension>) {
    if let Ok(mut slot) = SHELL.lock() {
        *slot = Some(extension);
    }
}

fn with_shell<T>(call: impl FnOnce(&mut dyn ShellExtension) -> T) -> Option<T> {
    let mut slot = SHELL.lock().ok()?;
    slot.as_mut().map(|extension| call(extension.as_mut()))
}

pub(crate) fn shell_message(kind: &str, data: &str) -> Reply {
    with_shell(|shell| shell.message(kind, data)).unwrap_or_default()
}

pub(crate) fn shell_docks(workspace_id: &str) -> bool {
    with_shell(|shell| shell.docks(workspace_id)).unwrap_or(false)
}

pub(crate) fn shell_draw_dock(buffer: &mut Buffer, area: Rect, focused: bool) {
    with_shell(|shell| shell.draw_dock(buffer, area, focused));
}

pub(crate) fn shell_dock_key(key: KeyEvent) -> Reply {
    with_shell(|shell| shell.dock_key(key)).unwrap_or(Reply {
        release: true,
        pass: true,
        ..Reply::default()
    })
}

pub(crate) fn shell_asking() -> bool {
    with_shell(|shell| shell.asking()).unwrap_or(false)
}

pub(crate) fn shell_draw_question(buffer: &mut Buffer, area: Rect) -> Rect {
    with_shell(|shell| shell.draw_question(buffer, area)).unwrap_or_default()
}

pub(crate) fn shell_question_key(key: KeyEvent) -> Reply {
    with_shell(|shell| shell.question_key(key)).unwrap_or_default()
}

pub(crate) fn shell_question_paste(text: &str) -> Reply {
    with_shell(|shell| shell.question_paste(text)).unwrap_or_default()
}

pub(crate) fn shell_tick() -> bool {
    with_shell(|shell| shell.tick()).unwrap_or(false)
}

pub(crate) fn shell_status() -> Option<String> {
    with_shell(|shell| shell.status()).flatten()
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
