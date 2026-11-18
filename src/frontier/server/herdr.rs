//! The one module that talks to herdr (principle 7; 0392). herdr 0.9.x's socket API: newline-delimited JSON over a named
//! pipe on Windows (a Unix socket elsewhere); a `{id, method, params}` request line is answered by a line echoing `id`
//! with `result` or `error {code, message}`; `events.subscribe` keeps its connection open and pushes bare
//! `{event, data}` lines (herdr `docs/next/website/src/content/docs/socket-api.mdx` "Socket transport", "Event
//! subscriptions"; schema `docs/next/api/herdr-api.schema.json`).
//!
//! Every [`Herdr`] method returns a boxed future, so a `dyn Herdr` can be shared (`Arc<dyn Herdr>`); the in-memory
//! stand-in the service's tests use is [`fake_herdr::FakeHerdr`], and the socket stand-in is
//! [`fake_herdr::FakeHerdrServer`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::FuturesOrdered;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

use crate::shared::errors::{Error, Result};
use crate::shared::line_rpc::LineSplitter;
use crate::shared::log::{LogLevel, log};
use crate::shared::poll::wait_for;
use crate::shared::proctree::{ProcessId, end_processes, process_id_of};

#[cfg(test)]
#[path = "fake_herdr.rs"]
pub(crate) mod fake_herdr;
#[cfg(test)]
#[path = "herdr_tests.rs"]
mod tests;

/// What a failing herdr call says; the TypeScript's `HerdrError`.
pub type HerdrResult<T> = std::result::Result<T, HerdrError>;

/// An environment for a process herdr starts, in the order it was written (it reaches herdr's wire as a JSON object).
pub type Env = Vec<(String, String)>;

/// Display tokens by name, in order; `None` clears one (JSON `null`).
pub type Tokens = Vec<(String, Option<String>)>;

/// The pane ids a watch follows, asked again every interval.
pub type PaneIds = Box<dyn Fn() -> Vec<String> + Send + Sync>;

/// Told each change of a watched pane's agent state.
pub type OnChange = Box<dyn Fn(WatchEvent) + Send + Sync>;

/// Stops a watch; calling it again does nothing.
pub type StopWatch = Box<dyn Fn() + Send + Sync>;

/// Runs once a worktree's checkout exists and before its pane starts; resolves what joins the pane's env.
pub type Prepare = Box<dyn FnOnce(PathBuf) -> BoxFuture<'static, Result<Env>> + Send>;

/// Where warnings go.
pub type OnWarning = Arc<dyn Fn(&str) + Send + Sync>;

/// What [`connect_herdr`] and the session helpers are given.
#[derive(Clone, Default)]
pub struct HerdrOptions {
	/// herdr session name; Jocasta's is `jocasta`.
	pub session: String,

	/// Socket (a pipe name on Windows) override (tests); [`herdr_socket_path`] when omitted.
	pub socket_path: Option<String>,
	/// Where warnings go; Jocasta's log (`log.rs`) when omitted.
	pub on_warning: Option<OnWarning>,
	/// Per-request timeout; 30 s when omitted.
	pub timeout_ms: Option<i64>,
	/// How often `watch` re-reads its pane set; 1 s when omitted.
	pub watch_interval_ms: Option<i64>,
}

/// What herdr's agent detection says about a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
	Working,
	Blocked,
	Idle,
	Done,
	#[default]
	Unknown,
}

impl AgentState {
	/// The state herdr's `agent_status` names; `Unknown` for anything else.
	fn of(status: Option<&str>) -> Self {
		match status {
			Some("working") => Self::Working,
			Some("blocked") => Self::Blocked,
			Some("idle") => Self::Idle,
			Some("done") => Self::Done,
			_ => Self::Unknown,
		}
	}

	/// The state's name as herdr and Jocasta's JSON spell it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Working => "working",
			Self::Blocked => "blocked",
			Self::Idle => "idle",
			Self::Done => "done",
			Self::Unknown => "unknown",
		}
	}
}

/// One pane as herdr lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneInfo {
	pub pane_id: String,
	pub tab_id: String,
	pub workspace_id: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cwd: Option<PathBuf>,
	pub agent_state: AgentState,
	/// The agent herdr detected or an integration reported in the pane (`omp` once omp is up).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub agent: Option<String>,
	/// The omp session file herdr's omp integration reported (`agent_session` of kind `path`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub session_path: Option<PathBuf>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub label: Option<String>,
	/// herdr's terminal behind the pane: a new one each time herdr starts the pane's process (a restore, a respawn).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub terminal_id: Option<String>,
	/// The one pane herdr has focused (its workspace, tab and pane), as `pane.list` says (0484); serialized only when
	/// true.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub focused: bool,
}

/// One workspace as herdr lists it, with the folder of its first pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
	pub id: String,
	pub label: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cwd: Option<PathBuf>,
}

/// herdr's workspaces and panes read together, once (the refresh tick's view).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HerdrSnapshot {
	pub workspaces: Vec<WorkspaceInfo>,
	pub panes: Vec<PaneInfo>,
}

/// herdr's answers to `notification.show` (`NotificationShowReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationResult {
	#[default]
	Shown,
	Disabled,
	RateLimited,
	NoForegroundClient,
	Busy,
}

impl NotificationResult {
	fn of(reason: Option<&str>) -> Self {
		match reason {
			Some("disabled") => Self::Disabled,
			Some("rate_limited") => Self::RateLimited,
			Some("no_foreground_client") => Self::NoForegroundClient,
			Some("busy") => Self::Busy,
			_ => Self::Shown,
		}
	}
}

/// A pane's agent state changed (or was first read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
	pub pane_id: String,
	pub workspace_id: String,
	pub state: AgentState,
}

/// A tab to create: one pane running `command` in `cwd`.
#[derive(Debug, Clone, Default)]
pub struct TabSpec {
	pub title: String,
	pub cwd: PathBuf,
	pub command: Vec<String>,
	pub env: Option<Env>,
}

/// A tab and its pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabPane {
	pub tab_id: String,
	pub pane_id: String,
}

/// A git worktree for `branch` at `path` and the pane to run in it.
#[derive(Default)]
pub struct WorktreeOptions {
	pub repo: PathBuf,
	pub branch: String,
	pub base: Option<String>,
	pub path: PathBuf,
	pub title: String,
	pub command: Vec<String>,
	pub env: Option<Env>,
	pub prepare: Option<Prepare>,
}

/// What `create_worktree` made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeCreated {
	pub workspace_id: String,
	pub tab_id: String,
	pub pane_id: String,
	pub path: PathBuf,
}

/// What a pane is started again with.
#[derive(Debug, Clone, Default)]
pub struct RespawnOptions {
	pub cwd: PathBuf,
	pub command: Vec<String>,
	pub env: Option<Env>,
}

/// Which way a split opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
	Right,
	Down,
}

impl Direction {
	fn as_str(self) -> &'static str {
		match self {
			Self::Right => "right",
			Self::Down => "down",
		}
	}
}

/// A pane to start beside another.
#[derive(Debug, Clone, Default)]
pub struct SplitOptions {
	pub command: Vec<String>,
	pub cwd: PathBuf,
	pub env: Option<Env>,
	pub label: Option<String>,
	pub ratio: Option<f64>,
	pub direction: Option<Direction>,
}

/// herdr's notification sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
	Request,
	None,
}

/// A Windows notification through herdr's client.
#[derive(Debug, Clone)]
pub struct Note {
	pub title: String,
	pub body: Option<String>,
	pub sound: Sound,
}

/// Where a plugin's pane opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
	Split,
	Popup,
}

/// One of a herdr plugin's panes to open.
#[derive(Debug, Clone)]
pub struct PluginPaneOptions {
	pub plugin_id: String,
	pub entrypoint: String,
	pub placement: Placement,
	pub target_pane_id: Option<String>,
	pub direction: Option<Direction>,
	pub cwd: Option<PathBuf>,
	pub env: Option<Env>,
	/// `false` when omitted.
	pub focus: Option<bool>,
}

/// The Agents view's filter and sort for `source`.
#[derive(Debug, Clone, Default)]
pub struct AgentView {
	pub source: String,
	pub label: Option<String>,
	pub filter: Option<Value>,
	pub sort: Option<Vec<Value>>,
}

/// What `report_pane_metadata` reports.
#[derive(Debug, Clone, Default)]
pub struct PaneMetadata {
	pub tokens: Option<Tokens>,
	pub state_labels: Option<Vec<(AgentState, String)>>,
	pub ttl_ms: Option<i64>,
}

/// herdr as the rest of Jocasta uses it.
pub trait Herdr: Send + Sync {
	/// Always true: the server is Jocasta's own process (0495).
	fn started_server(&self) -> bool;
	/// herdr's server version (`ping`); fails when the server doesn't answer.
	fn ping(&self) -> BoxFuture<'_, HerdrResult<Option<String>>>;
	/// Workspace id of the workspace labelled `label`; creates it in `cwd` when missing, its one tab running `command`
	/// (with `env`) when given (else the default shell).
	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, HerdrResult<String>>;
	fn workspaces(&self) -> BoxFuture<'_, HerdrResult<Vec<WorkspaceInfo>>>;
	/// Workspaces and panes from one `workspace.list` and one `pane.list`.
	fn snapshot(&self) -> BoxFuture<'_, HerdrResult<HerdrSnapshot>>;
	/// Closes a workspace with its panes (`workspace.close`) and ends every process they ran, with all they started
	/// (0486), so nothing holds its folder.
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, HerdrResult<TabPane>>;
	/// A git worktree for `branch` at `path` (a new branch starts from `base`, else HEAD) and a pane running `command`
	/// in it. herdr opens every worktree as its own workspace, grouped under the repo's workspace, so the result names
	/// that workspace too. `prepare` runs once the checkout exists and before the pane starts; what it returns joins
	/// the pane's env (0457, 0473).
	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, HerdrResult<WorktreeCreated>>;
	/// Moves a workspace to `index` in herdr's list (0 is the first, Alt+1).
	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, HerdrResult<()>>;
	/// Replaces the pane's tab with one pane running `command` (the tab keeps its label): a pane herdr restored as a
	/// shell runs omp again. Returns the new pane.
	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, HerdrResult<TabPane>>;
	/// A new pane running `command` beside `pane_id` in its tab, named `label` when given; returns the new pane id.
	fn split_beside<'a>(&'a self, pane_id: &'a str, opts: &'a SplitOptions) -> BoxFuture<'a, HerdrResult<String>>;
	/// Submits `text` to the pane's agent: a bracketed paste, then Enter. ESC and other control characters are removed
	/// first, so the text can't end the paste early and type keys of its own.
	fn prompt<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	/// Types `text` into the pane without submitting it (control characters removed, as for `prompt`).
	fn send_text<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	/// One Escape.
	fn stop<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	/// One Enter: submits what sits in the pane's input (a prompt whose paste landed but whose Enter didn't).
	fn submit<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	/// Keys by herdr's key names (`f9`, `enter`) to the pane, as typed (`pane.send_keys`).
	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, HerdrResult<()>>;
	/// A Windows notification through herdr's client (`notification.show`, 0554): title and body as given, with
	/// herdr's `request` sound or none. Resolves herdr's reason: `Shown`, or why not.
	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, HerdrResult<NotificationResult>>;
	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>>;
	/// Opens one of a herdr plugin's panes (`plugin.pane.open`, 0553): `Split` beside `target_pane_id` resolves the new
	/// pane's id; `Popup` (no pane id) resolves `None`. `env` reaches the launched process.
	fn open_plugin_pane<'a>(&'a self, opts: &'a PluginPaneOptions) -> BoxFuture<'a, HerdrResult<Option<String>>>;
	/// Closes the session's open popup (`popup.close`); false when none is open.
	fn close_popup(&self) -> BoxFuture<'_, HerdrResult<bool>>;
	/// Sets a tab's top split: `ratio` of it for the first side (`layout.set_split_ratio`, path `[]`).
	fn set_split_ratio<'a>(&'a self, tab_id: &'a str, ratio: f64) -> BoxFuture<'a, HerdrResult<()>>;
	/// Installs the Agents view's filter and sort for `source` (`agent.view.set`, 0553).
	fn set_agent_view<'a>(&'a self, view: &'a AgentView) -> BoxFuture<'a, HerdrResult<()>>;
	/// What the pane shows now, as plain text (`pane.read`, its visible viewport).
	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<String>>;
	/// Display-only tokens and state labels; `ttl_ms` makes herdr drop them when they aren't reported again in time, so
	/// a stopped service leaves nothing stale.
	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata) -> BoxFuture<'a, HerdrResult<()>>;
	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, HerdrResult<()>>;
	fn panes(&self) -> BoxFuture<'_, HerdrResult<Vec<PaneInfo>>>;
	/// The process herdr started for the pane (`pane.process_info`'s `shell_pid`; omp in Jocasta's panes), `None` when
	/// herdr names none. Never starts the server; fails when it doesn't answer within `timeout_ms` (the client's own
	/// timeout when omitted).
	fn pane_root<'a>(&'a self, pane_id: &'a str, timeout_ms: Option<i64>) -> BoxFuture<'a, HerdrResult<Option<u32>>>;
	/// Calls `on_change` when a watched pane's agent state changes, starting with each pane's current state. The pane
	/// set is re-read from `pane_ids` every interval; the subscription is reopened when it changes or drops
	/// (`events_lost`, herdr restarted). Returns the function that stops watching. Needs a running tokio runtime.
	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch;
	/// Stops this session's server with its panes (`server.stop`; only this session, 0434) and waits for it to go;
	/// then ends every process the panes ran, with all they started (0486). Does nothing when it isn't running.
	fn stop_server(&self) -> BoxFuture<'_, HerdrResult<()>>;
	/// Stops every watch. Requests hold no connection between calls.
	fn close(&self);
}

/// A herdr error response (`{id, error: {code, message}}`), or a local failure: `unreachable` (no server on the
/// socket), `transport` (closed before answering), `timeout`, `not_installed`, `invalid_response`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrError {
	pub code: String,
	pub message: String,
}

impl HerdrError {
	pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
		Self { code: code.into(), message: message.into() }
	}
}

impl std::fmt::Display for HerdrError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for HerdrError {}

impl From<HerdrError> for Error {
	fn from(error: HerdrError) -> Self {
		Self::Other(error.message)
	}
}

impl From<Error> for HerdrError {
	/// A failure of a caller's own callback (`prepare`) passing through herdr's call.
	fn from(error: Error) -> Self {
		Self::new("error", error.to_string())
	}
}

fn is_unreachable(error: &HerdrError) -> bool {
	error.code == "unreachable"
}

fn is_transport(error: &HerdrError) -> bool {
	error.code == "transport"
}

/// herdr's answer while its server is on its way out (`server_unavailable`): its panes are going, and the server with
/// them, so a stop has nothing left to do. The embedded server answers so between its loop's end and the process's.
fn is_shutting_down(error: &HerdrError) -> bool {
	error.code == "server_unavailable"
}

/// Shown in herdr's metadata as the reporter (`source` must match `[A-Za-z0-9:._-]{1,80}`).
const SOURCE: &str = "jocasta";
/// How long a stopped server may take to close its socket (0434).
const SERVER_STOP_MS: i64 = 10_000;
/// A ping while stopping: a server on its way out answers at once or not at all.
pub const STOP_PING_MS: i64 = 2_000;
/// How many panes' processes `pane_processes` asks herdr for at once (SRV-A-8).
const PANE_READS_AT_ONCE: usize = 4;
const DEFAULT_TIMEOUT_MS: i64 = 30_000;
/// `worktree.create` runs `git worktree add` before it answers.
const WORKTREE_TIMEOUT_MS: i64 = 120_000;
/// How long a connect waits for a busy pipe to take it, as libuv's waits (Windows).
#[cfg(windows)]
const PIPE_BUSY_MS: u64 = 30_000;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn millis(ms: i64) -> Duration {
	Duration::from_millis(ms.max(0).unsigned_abs())
}

/// herdr's socket for `session`. The server runs inside Jocasta's process, whose herdr config dir is
/// `<data dir>/herdr` (`embed::set_dirs`): Jocasta's default session `jocasta` is herdr's default session, socket
/// `<data dir>/herdr/herdr.sock`; any other name is `<data dir>/herdr/sessions/<name>/herdr.sock`.
pub fn herdr_socket_path(session: &str) -> String {
	let config_dir = crate::shared::paths::data_dir().join("herdr");
	let file = if session == "jocasta" {
		config_dir.join("herdr.sock")
	} else {
		config_dir.join("sessions").join(session).join("herdr.sock")
	};
	let mut file = file.display().to_string();
	if cfg!(windows) {
		file = file.replace('/', "\\");
	}
	herdr_pipe(&file)
}

/// What a client connects to for herdr's socket file `file` ([`herdr_socket_path`]'s, or the `HERDR_SOCKET_PATH` herdr
/// gives its plugin commands): on Windows herdr binds that path string as a namespaced pipe name, so clients connect to
/// `\\.\pipe\<path>` (`src/ipc.rs:44-79`); the file itself elsewhere.
pub fn herdr_pipe(file: &str) -> String {
	if cfg!(windows) { format!("\\\\.\\pipe\\{file}") } else { file.to_owned() }
}

/// Connects to herdr's session `options.session`: one `ping`. The server is Jocasta's own process, so none is started
/// or revived here; one that doesn't answer fails with `unreachable`.
pub async fn connect_herdr(options: HerdrOptions) -> HerdrResult<Arc<dyn Herdr>> {
	let path = options.socket_path.clone().unwrap_or_else(|| herdr_socket_path(&options.session));
	let client = Arc::new(Client::new(path, options.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)));
	client.ping().await?;
	Ok(Arc::new(HerdrClient::new(client, options.watch_interval_ms.unwrap_or(1000))))
}

/// One request to session `session`'s server, as the service's client sends it (a read herdr dropped is asked again
/// once, 0517); never starts a server. Fails with a [`HerdrError`].
pub async fn herdr_request_once<T: DeserializeOwned>(
	session: &str,
	method: &str,
	params: &Value,
	timeout_ms: Option<i64>,
) -> HerdrResult<T> {
	let ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
	Client::new(herdr_socket_path(session), ms).request(method, params, Some(ms)).await
}

/// A connect error as herdr's state: no socket or pipe (ENOENT) or nothing listening (ECONNREFUSED) is no server
/// (`unreachable`); anything else (a busy pipe, EMFILE, EACCES) is a `transport` failure.
fn connect_error(path: &str, error: &std::io::Error) -> HerdrError {
	let code = match error.kind() {
		std::io::ErrorKind::NotFound => Some("ENOENT"),
		std::io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
		_ => None,
	};
	match code {
		Some(code) => {
			HerdrError::new("unreachable", format!("herdr isn't reachable at {path}: Error: connect {code} {path}"))
		}
		None => HerdrError::new("transport", format!("herdr's socket at {path} didn't connect: Error: {error}")),
	}
}

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(not(windows))]
type Stream = tokio::net::UnixStream;

#[cfg(windows)]
async fn open_stream(path: &str) -> std::io::Result<Stream> {
	use tokio::net::windows::named_pipe::ClientOptions;
	const ERROR_PIPE_BUSY: i32 = 231;
	let end = tokio::time::Instant::now() + Duration::from_millis(PIPE_BUSY_MS);
	loop {
		match ClientOptions::new().open(path) {
			Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && tokio::time::Instant::now() < end => {
				tokio::time::sleep(Duration::from_millis(50)).await;
			}
			other => return other,
		}
	}
}

#[cfg(not(windows))]
async fn open_stream(path: &str) -> std::io::Result<Stream> {
	tokio::net::UnixStream::connect(path).await
}

/// Opens one NDJSON connection (rejected with [`connect_error`]'s `HerdrError`).
async fn connect_stream(path: &str) -> HerdrResult<Stream> {
	open_stream(path).await.map_err(|error| connect_error(path, &error))
}

/// Text decoded from a byte stream a chunk at a time, a character cut by a chunk's end kept for the next.
#[derive(Default)]
struct Utf8Stream {
	partial: Vec<u8>,
}

impl Utf8Stream {
	fn decode(&mut self, chunk: &[u8]) -> String {
		self.partial.extend_from_slice(chunk);
		let valid = match std::str::from_utf8(&self.partial) {
			Ok(_) => self.partial.len(),
			Err(error) if error.error_len().is_none() => error.valid_up_to(),
			Err(_) => self.partial.len(),
		};
		let text = String::from_utf8_lossy(&self.partial[..valid]).into_owned();
		self.partial.drain(..valid);
		text
	}
}

/// The non-empty lines a stream sends, one at a time.
struct LineReader<R> {
	reader: R,
	utf8: Utf8Stream,
	splitter: LineSplitter,
	ready: VecDeque<String>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
	fn new(reader: R) -> Self {
		Self { reader, utf8: Utf8Stream::default(), splitter: LineSplitter::new(), ready: VecDeque::new() }
	}

	/// The next line, or `None` once the stream ended or failed.
	async fn next_line(&mut self) -> Option<String> {
		loop {
			if let Some(line) = self.ready.pop_front() {
				return Some(line);
			}
			let mut buffer = [0u8; 8192];
			match self.reader.read(&mut buffer).await {
				Ok(0) | Err(_) => return None,
				Ok(count) => {
					let text = self.utf8.decode(&buffer[..count]);
					self.ready.extend(self.splitter.push(&text).into_iter().filter(|line| !line.is_empty()));
				}
			}
		}
	}
}

/// A line herdr sent, only the fields used here.
struct Response {
	id: Option<String>,
	result: Value,
	error: Option<(Option<String>, Option<String>)>,
	event: Option<String>,
	/// Read only for `pane.agent_status_changed` (schema subscription_event `PaneAgentStatusChangedEvent`).
	data: Value,
}

fn parse(line: &str) -> Option<Response> {
	let value: Value = serde_json::from_str(line).ok()?;
	let text = |v: &Value| v.as_str().map(str::to_owned);
	let error = match value.get("error") {
		Some(Value::Object(error)) => Some((error.get("code").and_then(text), error.get("message").and_then(text))),
		Some(Value::Null) | None => None,
		Some(_) => Some((None, None)),
	};
	Some(Response {
		id: value.get("id").and_then(text),
		result: value.get("result").cloned().unwrap_or(Value::Null),
		error,
		event: value.get("event").and_then(text),
		data: value.get("data").cloned().unwrap_or(Value::Null),
	})
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> String {
	format!("jocasta_{}", NEXT_ID.fetch_add(1, Ordering::SeqCst) + 1)
}

/// The requests that only read herdr's state. herdr 0.9.5 sometimes closes an accepted connection without answering
/// (seen in the VM under bursts of thread starts and closes, 0517); one of these is then asked again once, while a
/// change never is: herdr may have made it.
const READS: [&str; 6] = ["ping", "workspace.list", "pane.list", "pane.get", "pane.read", "pane.process_info"];

/// Requests. herdr's API server reads one request line per connection, answers it and closes the connection (herdr
/// `src/api/server.rs:328-376`, `read_initial_request_line`); only `events.subscribe` keeps it open. So every request
/// opens its own connection.
struct Client {
	path: String,
	timeout_ms: i64,
}

impl Client {
	fn new(path: String, timeout_ms: i64) -> Self {
		Self { path, timeout_ms }
	}

	/// ping → `{type: "pong", version, protocol}` (socket-api.mdx "Socket transport").
	async fn ping(&self) -> HerdrResult<Option<String>> {
		let answer = self.attempt("ping", &json!({}), self.timeout_ms).await?;
		Ok(answer.get("version").and_then(Value::as_str).map(str::to_owned))
	}

	async fn request<T: DeserializeOwned>(
		&self,
		method: &str,
		params: &Value,
		timeout_ms: Option<i64>,
	) -> HerdrResult<T> {
		let answer = self.request_value(method, params, timeout_ms).await?;
		serde_json::from_value(answer).map_err(|error| {
			HerdrError::new("invalid_response", format!("herdr's {method} answer was unusable: {error}"))
		})
	}

	async fn request_value(&self, method: &str, params: &Value, timeout_ms: Option<i64>) -> HerdrResult<Value> {
		self.attempt(method, params, timeout_ms.unwrap_or(self.timeout_ms)).await
	}

	/// `server.stop` (schema `EmptyParams`; what `herdr server stop` sends), then waits for the socket to go. Every
	/// process its panes ran, with all they started, is listed before and ended after (0486): omp's workers outlived
	/// the server and held a worktree. False when no server was running.
	async fn stop_server(&self) -> HerdrResult<bool> {
		match self.attempt("ping", &json!({}), STOP_PING_MS).await {
			Err(error) if is_unreachable(&error) || is_shutting_down(&error) => return Ok(false),
			Err(error) => return Err(error),
			Ok(_) => {}
		}
		let processes = self.pane_processes(None, None).await;
		let stopped = self.stop_and_wait().await;
		end_processes(&processes, &[]).await;
		stopped.map(|()| true)
	}

	async fn stop_and_wait(&self) -> HerdrResult<()> {
		if let Err(error) = self.attempt("server.stop", &json!({}), self.timeout_ms).await {
			// A server on its way out may close the connection before it answers.
			if !is_unreachable(&error) && !is_transport(&error) && !is_shutting_down(&error) {
				return Err(error);
			}
		}
		// `ping` answers until the socket goes, which in Jocasta's own process is only at its end; any other request
		// answers `server_unavailable` from the stop on, and that is enough: the server is going.
		let stopped = wait_for(
			|| async {
				Ok(match self.attempt("pane.list", &json!({}), STOP_PING_MS).await {
					Ok(_) => false,
					Err(error) => is_unreachable(&error) || is_shutting_down(&error),
				})
			},
			SERVER_STOP_MS,
			100,
		)
		.await;
		if matches!(stopped, Ok(true)) {
			return Ok(());
		}
		Err(HerdrError::new(
			"timeout",
			format!("herdr's server at {} didn't stop in {} s", self.path, SERVER_STOP_MS / 1000),
		))
	}

	/// The processes herdr started for the panes (those of workspace `workspace_id` or tab `tab_id`, else every pane),
	/// each by pid and start, read while herdr still has the panes: each pane's root (`pane.process_info`'s
	/// `shell_pid`) and its foreground processes. `end_processes` later ends them with everything they started, by the
	/// start-checked walk (SRV-A-1). At most 4 panes are asked at once (0517: herdr drops requests under bursts), and
	/// the panes whose processes couldn't be read are named in one warning (SRV-A-8). Never starts the server; empty
	/// when herdr can't say.
	async fn pane_processes(&self, workspace_id: Option<&str>, tab_id: Option<&str>) -> Vec<ProcessId> {
		let panes = match self.request_listed_panes().await {
			Ok(panes) => panes,
			Err(error) => {
				log(LogLevel::Warn, "herdr", &format!("the panes' processes weren't listed: {}", error.message), None);
				return Vec::new();
			}
		};
		let asked: Vec<&str> = panes
			.iter()
			.filter(|pane| workspace_id.is_none_or(|id| pane.workspace_id == id))
			.filter(|pane| tab_id.is_none_or(|id| pane.tab_id == id))
			.map(|pane| pane.pane_id.as_str())
			.collect();
		// A sliding window of reads, answers in the order asked.
		let mut reads: Vec<(&str, HerdrResult<WireProcessInfo>)> = Vec::new();
		let mut running = FuturesOrdered::new();
		let mut waiting = asked.iter().copied();
		for id in waiting.by_ref().take(PANE_READS_AT_ONCE) {
			running.push_back(self.read_pane(id));
		}
		while let Some(read) = running.next().await {
			reads.push(read);
			if let Some(id) = waiting.next() {
				running.push_back(self.read_pane(id));
			}
		}
		let mut pids: Vec<i64> = Vec::new();
		let mut unread: Vec<&str> = Vec::new();
		for (pane_id, info) in reads {
			match info {
				Ok(info) => {
					pids.push(info.shell_pid.unwrap_or(0));
					pids.extend(info.foreground_processes.unwrap_or_default().iter().map(|p| p.pid));
				}
				Err(_) => unread.push(pane_id),
			}
		}
		if !unread.is_empty() {
			let plural = if unread.len() == 1 { "" } else { "s" };
			log(
				LogLevel::Warn,
				"herdr",
				&format!("the processes of pane{plural} {} weren't read", unread.join(", ")),
				None,
			);
		}
		let mut seen = HashSet::new();
		pids.into_iter()
			.filter(|pid| *pid > 0 && seen.insert(*pid))
			.filter_map(|pid| u32::try_from(pid).ok().and_then(process_id_of))
			.collect()
	}

	/// One pane's processes, named by the pane (`pane_processes`' reads).
	async fn read_pane<'a>(&self, pane_id: &'a str) -> (&'a str, HerdrResult<WireProcessInfo>) {
		(pane_id, self.process_info(pane_id, None).await)
	}

	/// `pane.list {}` → `{type: "pane_list", panes}` across all workspaces.
	async fn request_listed_panes(&self) -> HerdrResult<Vec<WirePane>> {
		let answer = self.attempt("pane.list", &json!({}), self.timeout_ms).await?;
		decode::<PaneList>("pane.list", answer).map(|list| list.panes)
	}

	/// One pane's processes, within `timeout_ms` (the client's own by default); never starts the server.
	async fn process_info(&self, pane_id: &str, timeout_ms: Option<i64>) -> HerdrResult<WireProcessInfo> {
		// pane.process_info {pane_id} → `{type: "pane_process_info", process_info}` (schema `PaneProcessInfo`).
		let answer = self
			.attempt("pane.process_info", &json!({"pane_id": pane_id}), timeout_ms.unwrap_or(self.timeout_ms))
			.await?;
		decode::<ProcessInfoAnswer>("pane.process_info", answer).map(|answer| answer.process_info)
	}

	/// A read herdr dropped before answering is asked again once ([`READS`]); anything else fails as it came.
	async fn attempt(&self, method: &str, params: &Value, timeout_ms: i64) -> HerdrResult<Value> {
		match self.send(method, params, timeout_ms).await {
			Err(error) if is_transport(&error) && READS.contains(&method) => {
				tokio::time::sleep(Duration::from_millis(100)).await;
				self.send(method, params, timeout_ms).await
			}
			other => other,
		}
	}

	/// One request on its own connection: herdr answers it and closes. `timeout_ms` covers connecting too.
	async fn send(&self, method: &str, params: &Value, timeout_ms: i64) -> HerdrResult<Value> {
		let id = next_id();
		let line = format!("{}\n", json!({"id": id, "method": method, "params": params}));
		let closed = || HerdrError::new("transport", format!("herdr closed the connection before answering {method}"));
		let exchange = async {
			let mut stream = connect_stream(&self.path).await?;
			stream.write_all(line.as_bytes()).await.map_err(|_| closed())?;
			let mut lines = LineReader::new(&mut stream);
			while let Some(line) = lines.next_line().await {
				// An undecodable request is answered with an empty `id` (socket-api.mdx "Socket transport").
				let Some(response) = parse(&line) else { continue };
				if response.id.as_deref() != Some(id.as_str()) && response.id.as_deref() != Some("") {
					continue;
				}
				return match response.error {
					Some((code, message)) => Err(HerdrError::new(
						code.unwrap_or_else(|| "error".to_owned()),
						message.unwrap_or_else(|| "herdr error".to_owned()),
					)),
					None => Ok(response.result),
				};
			}
			Err(closed())
		};
		match tokio::time::timeout(millis(timeout_ms), exchange).await {
			Ok(result) => result,
			Err(_) => Err(HerdrError::new("timeout", format!("herdr didn't answer {method} in {timeout_ms} ms"))),
		}
	}
}

fn decode<T: DeserializeOwned>(method: &str, value: Value) -> HerdrResult<T> {
	serde_json::from_value(value)
		.map_err(|error| HerdrError::new("invalid_response", format!("herdr's {method} answer was unusable: {error}")))
}

// Wire shapes (schema success_response $defs), only the fields used here.
#[derive(Deserialize)]
struct WirePane {
	pane_id: String,
	tab_id: String,
	workspace_id: String,
	cwd: Option<String>,
	agent_status: Option<String>,
	agent: Option<String>,
	agent_session: Option<WireSession>,
	label: Option<String>,
	terminal_id: Option<String>,
	focused: Option<bool>,
}

#[derive(Deserialize)]
struct WireSession {
	kind: Option<String>,
	value: Option<String>,
}

/// `pane.process_info`'s `process_info` (schema `PaneProcessInfo`; `shell_pid` is the process herdr started).
#[derive(Deserialize)]
struct WireProcessInfo {
	shell_pid: Option<i64>,
	foreground_processes: Option<Vec<WireForeground>>,
}

#[derive(Deserialize)]
struct WireForeground {
	pid: i64,
}

#[derive(Deserialize)]
struct ProcessInfoAnswer {
	process_info: WireProcessInfo,
}

#[derive(Deserialize)]
struct WireWorkspace {
	workspace_id: String,
	label: String,
}

#[derive(Deserialize)]
struct WorkspaceList {
	workspaces: Vec<WireWorkspace>,
}

#[derive(Deserialize)]
struct PaneList {
	panes: Vec<WirePane>,
}

#[derive(Deserialize, Default)]
struct WireTab {
	tab_id: Option<String>,
}

#[derive(Deserialize)]
struct WorkspaceCreated {
	workspace: WireWorkspace,
	#[serde(default)]
	tab: WireTab,
}

#[derive(Deserialize)]
struct WorktreeCreatedAnswer {
	workspace: WireWorkspace,
	#[serde(default)]
	tab: WireTab,
	worktree: WireWorktree,
}

#[derive(Deserialize)]
struct WireWorktree {
	path: String,
}

#[derive(Deserialize)]
struct PaneGot {
	pane: WirePane,
}

#[derive(Deserialize)]
struct WireLayoutNode {
	#[serde(rename = "type")]
	kind: String,
	pane_id: Option<String>,
	first: Option<Box<WireLayoutNode>>,
	second: Option<Box<WireLayoutNode>>,
}

#[derive(Deserialize)]
struct WireLayout {
	tab_id: String,
	root: WireLayoutNode,
}

#[derive(Deserialize)]
struct LayoutApplied {
	layout: WireLayout,
}

#[derive(Deserialize)]
struct WireMove {
	changed: bool,
	reason: Option<String>,
	pane: WirePane,
}

#[derive(Deserialize)]
struct PaneMoved {
	move_result: WireMove,
}

#[derive(Deserialize)]
struct PaneRead {
	read: WireRead,
}

#[derive(Deserialize)]
struct WireRead {
	text: String,
}

#[derive(Deserialize)]
struct PluginOpened {
	pane: Option<PluginPane>,
}

#[derive(Deserialize)]
struct PluginPane {
	pane: Option<PluginPaneInfo>,
}

#[derive(Deserialize)]
struct PluginPaneInfo {
	pane_id: Option<String>,
}

fn pane_info(pane: WirePane) -> PaneInfo {
	let session = pane.agent_session.as_ref().filter(|s| s.kind.as_deref() == Some("path"));
	PaneInfo {
		agent_state: AgentState::of(pane.agent_status.as_deref()),
		cwd: pane.cwd.filter(|v| !v.is_empty()).map(PathBuf::from),
		agent: pane.agent.filter(|v| !v.is_empty()),
		session_path: session.and_then(|s| s.value.clone()).filter(|v| !v.is_empty()).map(PathBuf::from),
		label: pane.label.filter(|v| !v.is_empty()),
		terminal_id: pane.terminal_id.filter(|v| !v.is_empty()),
		focused: pane.focused == Some(true),
		pane_id: pane.pane_id,
		tab_id: pane.tab_id,
		workspace_id: pane.workspace_id,
	}
}

fn first_pane_id(node: &WireLayoutNode) -> Option<String> {
	if node.kind == "pane" {
		return node.pane_id.clone();
	}
	node.first.as_deref().and_then(first_pane_id).or_else(|| node.second.as_deref().and_then(first_pane_id))
}

/// `value` as a JSON path string.
fn path_json(path: &Path) -> Value {
	Value::String(path.display().to_string())
}

/// `env` as the JSON object herdr takes, in its order.
fn env_json(env: &Env) -> Value {
	Value::Object(env.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect())
}

/// `{...base, ...extra}`: a name set again keeps its place and takes the new value.
fn merge_env(base: Option<&Env>, extra: &Env) -> Env {
	let mut merged = base.cloned().unwrap_or_default();
	for (name, value) in extra {
		match merged.iter_mut().find(|(have, _)| have == name) {
			Some(slot) => slot.1 = value.clone(),
			None => merged.push((name.clone(), value.clone())),
		}
	}
	merged
}

/// A layout tree of one pane running `command` (`layout.apply` `LayoutNode` of type `pane`: argv, cwd, env).
fn command_pane(cwd: &Path, command: &[String], env: Option<&Env>) -> Value {
	let mut pane = json!({"type": "pane", "cwd": path_json(cwd), "command": command});
	if let (Some(env), Value::Object(map)) = (env, &mut pane) {
		map.insert("env".to_owned(), env_json(env));
	}
	pane
}

/// `{tab_id, ...layout}`: the params `layout.apply` takes to replace a tab.
fn tab_layout(tab_id: &str, layout: Value) -> Value {
	let mut params = Map::new();
	params.insert("tab_id".to_owned(), Value::String(tab_id.to_owned()));
	if let Value::Object(layout) = layout {
		params.extend(layout);
	}
	Value::Object(params)
}

/// `text` for a pane's input with ESC, the other C0 controls (tab and newline kept), DEL and the C1 controls removed:
/// herdr writes it inside a bracketed paste as it is (herdr `src/pane.rs:3708-3715`), so an ESC `[201~` in outside text
/// would end the paste and type the rest as keys (SEC-2).
pub fn pane_text(text: &str) -> String {
	text.chars()
		.filter(|char| {
			let code = u32::from(*char);
			code == 0x09 || code == 0x0a || (0x20..0x7f).contains(&code) || code > 0x9f
		})
		.collect()
}

/// `value` with [`pane_text`]'s control characters removed from every string in it (N-3): what a popup, `today` and a
/// notification get can't carry terminal escapes from a title, a brief or a question (SEC-NET-3).
pub fn clean_text(value: &Value) -> Value {
	match value {
		Value::String(text) => Value::String(pane_text(text)),
		Value::Array(items) => Value::Array(items.iter().map(clean_text).collect()),
		Value::Object(map) => Value::Object(map.iter().map(|(key, inner)| (key.clone(), clean_text(inner))).collect()),
		other => other.clone(),
	}
}

fn workspace_list(workspaces: Vec<WireWorkspace>, panes: &[WirePane]) -> Vec<WorkspaceInfo> {
	// WorkspaceInfo has no cwd; each workspace's first pane from pane.list supplies it.
	workspaces
		.into_iter()
		.map(|ws| {
			let cwd = panes
				.iter()
				.find(|pane| pane.workspace_id == ws.workspace_id)
				.and_then(|pane| pane.cwd.as_deref())
				.filter(|cwd| !cwd.is_empty())
				.map(PathBuf::from);
			WorkspaceInfo { id: ws.workspace_id, label: ws.label, cwd }
		})
		.collect()
}

type Watches = Mutex<HashMap<u64, Arc<WatchCore>>>;

struct HerdrClient {
	client: Arc<Client>,
	watch_interval_ms: i64,
	watches: Arc<Watches>,
	next_watch: AtomicU64,
}

impl HerdrClient {
	fn new(client: Arc<Client>, watch_interval_ms: i64) -> Self {
		Self { client, watch_interval_ms, watches: Arc::new(Mutex::new(HashMap::new())), next_watch: AtomicU64::new(0) }
	}

	async fn wire_panes(&self) -> HerdrResult<Vec<WirePane>> {
		wire_panes(&self.client).await
	}

	async fn snapshot(&self) -> HerdrResult<HerdrSnapshot> {
		// workspace.list → `{type: "workspace_list", workspaces}`; pane.list across all workspaces.
		let params = json!({});
		let (list, panes) =
			tokio::try_join!(self.client.request::<WorkspaceList>("workspace.list", &params, None), self.wire_panes())?;
		let workspaces = workspace_list(list.workspaces, &panes);
		Ok(HerdrSnapshot { workspaces, panes: panes.into_iter().map(pane_info).collect() })
	}

	async fn close_workspace(&self, workspace_id: &str) -> HerdrResult<()> {
		// Listed while herdr still has the panes; ended once it closed them (0486).
		let processes = self.client.pane_processes(Some(workspace_id), None).await;
		// workspace.close {workspace_id} → `{type: "ok"}` (schema `WorkspaceCloseParams`).
		self.client.request_value("workspace.close", &json!({"workspace_id": workspace_id}), None).await?;
		end_processes(&processes, &[]).await;
		Ok(())
	}

	async fn ensure_workspace(
		&self,
		label: &str,
		cwd: &Path,
		command: Option<&[String]>,
		env: Option<&Env>,
	) -> HerdrResult<String> {
		let list: WorkspaceList = self.client.request("workspace.list", &json!({}), None).await?;
		if let Some(existing) = list.workspaces.iter().find(|ws| ws.label == label) {
			return Ok(existing.workspace_id.clone());
		}
		// workspace.create {cwd, label, focus} → `{type: "workspace_created", workspace, tab, root_pane}`.
		let created: WorkspaceCreated = self
			.client
			.request("workspace.create", &json!({"cwd": path_json(cwd), "label": label, "focus": false}), None)
			.await?;
		// Its root tab runs the default shell; layout.apply replaces it with one running `command`.
		if let Some(command) = command {
			let tab_id =
				created.tab.tab_id.as_deref().ok_or_else(|| {
					HerdrError::new("invalid_response", "herdr's workspace.create returned no tab id")
				})?;
			let layout = json!({"tab_label": label, "focus": false, "root": command_pane(cwd, command, env)});
			self.replace_tab(tab_id, layout).await?;
		}
		Ok(created.workspace.workspace_id)
	}

	async fn create_tab(&self, workspace_id: &str, tab: &TabSpec) -> HerdrResult<TabPane> {
		// tab.create starts the default shell only; layout.apply {workspace_id, tab_label, root} creates a tab whose
		// pane runs an argv `command` → `{type: "layout_apply", layout: {tab_id, root}}` (socket-api.mdx
		// "layout.apply").
		self.apply_layout(&json!({
			"workspace_id": workspace_id,
			"tab_label": tab.title,
			"focus": false,
			"root": command_pane(&tab.cwd, &tab.command, tab.env.as_ref()),
		}))
		.await
	}

	async fn create_worktree(&self, workspace_id: &str, opts: WorktreeOptions) -> HerdrResult<WorktreeCreated> {
		// worktree.create {workspace_id | cwd, branch, base, path, label, focus} → `{type: "worktree_created",
		// workspace, tab, root_pane, worktree: {path}}`; it opens the checkout at `path` (schema
		// `WorktreeCreateParams`, else herdr's worktrees folder) as a new workspace grouped with the source workspace
		// and checks out `branch`, creating it from `base` or HEAD when missing (socket-api.mdx "Worktree methods";
		// `src/api/schema/worktrees.rs:22`).
		let mut params = Map::new();
		if workspace_id.is_empty() {
			params.insert("cwd".to_owned(), path_json(&opts.repo));
		} else {
			params.insert("workspace_id".to_owned(), json!(workspace_id));
		}
		params.insert("branch".to_owned(), json!(opts.branch));
		if let Some(base) = opts.base.as_ref().filter(|base| !base.is_empty()) {
			params.insert("base".to_owned(), json!(base));
		}
		params.insert("path".to_owned(), path_json(&opts.path));
		params.insert("label".to_owned(), json!(opts.title));
		params.insert("focus".to_owned(), json!(false));
		let created: WorktreeCreatedAnswer =
			self.client.request("worktree.create", &Value::Object(params), Some(WORKTREE_TIMEOUT_MS)).await?;
		let path = PathBuf::from(&created.worktree.path);
		let extra = match opts.prepare {
			Some(prepare) => prepare(path.clone()).await?,
			None => Env::new(),
		};
		let env = (opts.env.is_some() || !extra.is_empty()).then(|| merge_env(opts.env.as_ref(), &extra));
		let tab_id = created
			.tab
			.tab_id
			.as_deref()
			.ok_or_else(|| HerdrError::new("invalid_response", "herdr's worktree.create returned no tab id"))?;
		// Its root tab runs the default shell; layout.apply replaces it with a tab running `command`.
		let layout = json!({
			"tab_label": opts.title,
			"focus": false,
			"root": command_pane(&path, &opts.command, env.as_ref()),
		});
		let applied = self.replace_tab(tab_id, layout).await?;
		Ok(WorktreeCreated {
			workspace_id: created.workspace.workspace_id,
			tab_id: applied.tab_id,
			pane_id: applied.pane_id,
			path,
		})
	}

	async fn respawn(&self, pane_id: &str, opts: &RespawnOptions) -> HerdrResult<TabPane> {
		// layout.apply {tab_id, root} replaces the tab's layout; without `tab_label` the tab keeps its own name (herdr
		// `src/app/api/layouts.rs:72-80`).
		let target: PaneGot = self.client.request("pane.get", &json!({"pane_id": pane_id}), None).await?;
		let layout = json!({"focus": false, "root": command_pane(&opts.cwd, &opts.command, opts.env.as_ref())});
		self.replace_tab(&target.pane.tab_id, layout).await
	}

	async fn split_beside(&self, pane_id: &str, opts: &SplitOptions) -> HerdrResult<String> {
		// pane.split starts the default shell only and takes no argv. So: a temporary tab running `command`
		// (layout.apply), then pane.move {pane_id, destination: {type: "tab", tab_id, target_pane_id, split, ratio}}
		// into the target's tab; moving a tab's only pane closes that tab → `{type: "pane_move", move_result:
		// {changed, reason, pane}}` (socket-api.mdx "pane.move").
		let target: PaneGot = self.client.request("pane.get", &json!({"pane_id": pane_id}), None).await?;
		let temp = self
			.apply_layout(&json!({
				"workspace_id": target.pane.workspace_id,
				"focus": false,
				"root": command_pane(&opts.cwd, &opts.command, opts.env.as_ref()),
			}))
			.await?;
		let mut destination = json!({
			"type": "tab",
			"tab_id": target.pane.tab_id,
			"target_pane_id": pane_id,
			"split": opts.direction.unwrap_or(Direction::Right).as_str(),
		});
		if let (Some(ratio), Value::Object(map)) = (opts.ratio, &mut destination) {
			map.insert("ratio".to_owned(), json!(ratio));
		}
		let params = json!({"pane_id": temp.pane_id, "destination": destination, "focus": false});
		let moved: PaneMoved = match self.client.request("pane.move", &params, None).await {
			Ok(moved) => moved,
			Err(error) => {
				self.close_tab(&temp.tab_id).await;
				return Err(error);
			}
		};
		if !moved.move_result.changed {
			self.close_tab(&temp.tab_id).await;
			let reason = moved.move_result.reason.unwrap_or_else(|| "unchanged".to_owned());
			return Err(HerdrError::new(reason.clone(), format!("herdr didn't split {pane_id}: {reason}")));
		}
		let split = moved.move_result.pane.pane_id;
		// pane.rename {pane_id, label} (schema `PaneRenameParams`).
		if let Some(label) = opts.label.as_ref().filter(|label| !label.is_empty()) {
			self.client.request_value("pane.rename", &json!({"pane_id": split, "label": label}), None).await?;
		}
		Ok(split)
	}

	/// Closes a temporary tab; a tab that can't be closed is left.
	async fn close_tab(&self, tab_id: &str) {
		let _ = self.client.request_value("tab.close", &json!({"tab_id": tab_id}), None).await;
	}

	async fn apply_layout(&self, params: &Value) -> HerdrResult<TabPane> {
		let applied: LayoutApplied = self.client.request("layout.apply", params, None).await?;
		let Some(pane_id) = first_pane_id(&applied.layout.root) else {
			return Err(HerdrError::new("invalid_response", "herdr's layout.apply returned no pane id"));
		};
		Ok(TabPane { tab_id: applied.layout.tab_id, pane_id })
	}

	/// `layout.apply` over tab `tab_id`, replacing its panes. herdr (0.9.5 on Windows) drops a replaced pane but can
	/// leave the process it ran running, such as the default shell a new workspace or worktree starts with; nothing
	/// lists it after that. So the tab's processes are listed while it still has them and ended once the new layout is
	/// in (0486).
	async fn replace_tab(&self, tab_id: &str, layout: Value) -> HerdrResult<TabPane> {
		let replaced = self.client.pane_processes(None, Some(tab_id)).await;
		let applied = self.apply_layout(&tab_layout(tab_id, layout)).await?;
		end_processes(&replaced, &[]).await;
		Ok(applied)
	}

	async fn send_keys(&self, pane_id: &str, keys: &[&str]) -> HerdrResult<()> {
		// pane.send_keys {pane_id, keys}: herdr key-combo strings; `esc` is Escape, `enter` is Enter (socket-api.mdx
		// "pane.send_keys"; herdr `src/app/api/agents.rs:835-846`).
		self.client.request_value("pane.send_keys", &json!({"pane_id": pane_id, "keys": keys}), None).await.map(drop)
	}

	async fn notify(&self, note: &Note) -> HerdrResult<NotificationResult> {
		// notification.show {title, body?, sound?} → `{type: "notification_show", shown, reason}` (schema
		// `NotificationShowParams`, `NotificationShowReason`; herdr 0.9.5).
		let mut params = json!({"title": note.title});
		if let (Some(body), Value::Object(map)) = (note.body.as_ref().filter(|b| !b.is_empty()), &mut params) {
			map.insert("body".to_owned(), json!(body));
		}
		if let Value::Object(map) = &mut params {
			map.insert("sound".to_owned(), json!(if note.sound == Sound::Request { "request" } else { "none" }));
		}
		let answer = self.client.request_value("notification.show", &params, None).await?;
		Ok(NotificationResult::of(answer.get("reason").and_then(Value::as_str)))
	}

	async fn open_plugin_pane(&self, opts: &PluginPaneOptions) -> HerdrResult<Option<String>> {
		// plugin.pane.open {plugin_id, entrypoint, placement, target_pane_id, direction, cwd, env, focus} →
		// `{type: "plugin_pane_opened", pane: {plugin_id, entrypoint, pane: PaneInfo}}` for a split, `{type: "ok"}` for
		// a popup (socket-api.mdx "plugin.pane.open"; schema `PluginPaneOpenParams`, `PluginPaneInfo`).
		let mut params = Map::new();
		params.insert("plugin_id".to_owned(), json!(opts.plugin_id));
		params.insert("entrypoint".to_owned(), json!(opts.entrypoint));
		params
			.insert("placement".to_owned(), json!(if opts.placement == Placement::Split { "split" } else { "popup" }));
		if let Some(target) = opts.target_pane_id.as_ref().filter(|t| !t.is_empty()) {
			params.insert("target_pane_id".to_owned(), json!(target));
		}
		if let Some(direction) = opts.direction {
			params.insert("direction".to_owned(), json!(direction.as_str()));
		}
		if let Some(cwd) = opts.cwd.as_ref().filter(|cwd| !cwd.as_os_str().is_empty()) {
			params.insert("cwd".to_owned(), path_json(cwd));
		}
		if let Some(env) = &opts.env {
			params.insert("env".to_owned(), env_json(env));
		}
		params.insert("focus".to_owned(), json!(opts.focus.unwrap_or(false)));
		let opened: PluginOpened = self.client.request("plugin.pane.open", &Value::Object(params), None).await?;
		Ok(opened.pane.and_then(|p| p.pane).and_then(|p| p.pane_id))
	}

	async fn close_popup(&self) -> HerdrResult<bool> {
		// popup.close {} → `{type: "ok"}`, or error `popup_not_open`.
		match self.client.request_value("popup.close", &json!({}), None).await {
			Ok(_) => Ok(true),
			Err(error) if error.code == "popup_not_open" => Ok(false),
			Err(error) => Err(error),
		}
	}

	async fn report_pane_metadata(&self, pane_id: &str, meta: &PaneMetadata) -> HerdrResult<()> {
		// pane.report_metadata {pane_id, source, tokens, state_labels, ttl_ms}: display-only; tokens render as `$name`
		// in sidebar rows; `null` clears a token; `ttl_ms` (1 ms to 24 h) expires the report (socket-api.mdx "Agent
		// state reporting"; schema `PaneReportMetadataParams`).
		let mut params = json!({"pane_id": pane_id, "source": SOURCE});
		if let Value::Object(map) = &mut params {
			if let Some(tokens) = &meta.tokens {
				map.insert("tokens".to_owned(), tokens_json(tokens));
			}
			if let Some(labels) = &meta.state_labels {
				let labels = labels.iter().map(|(state, label)| (state.as_str().to_owned(), json!(label))).collect();
				map.insert("state_labels".to_owned(), Value::Object(labels));
			}
			if let Some(ttl) = meta.ttl_ms.filter(|ttl| *ttl != 0) {
				map.insert("ttl_ms".to_owned(), json!(ttl));
			}
		}
		self.client.request_value("pane.report_metadata", &params, None).await.map(drop)
	}

	async fn pane_root(&self, pane_id: &str, timeout_ms: Option<i64>) -> HerdrResult<Option<u32>> {
		let pid = self.client.process_info(pane_id, timeout_ms).await?.shell_pid;
		Ok(pid.filter(|pid| *pid > 0).and_then(|pid| u32::try_from(pid).ok()))
	}

	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		let id = self.next_watch.fetch_add(1, Ordering::SeqCst);
		let core = Arc::new(WatchCore {
			client: self.client.clone(),
			pane_ids,
			emitter: Arc::new(Emitter { last: Mutex::new(HashMap::new()), stopped: AtomicBool::new(false), on_change }),
			state: Mutex::new(WatchState::default()),
			id,
			registry: Arc::downgrade(&self.watches),
		});
		locked(&self.watches).insert(id, core.clone());
		core.start(self.watch_interval_ms);
		Box::new(move || core.stop())
	}
}

fn tokens_json(tokens: &Tokens) -> Value {
	Value::Object(tokens.iter().map(|(name, value)| (name.clone(), json!(value))).collect())
}

/// pane.list {} → `{type: "pane_list", panes}` across all workspaces, each with `agent_status` and, once herdr's omp
/// integration reported it, `agent_session {kind: "path", value: <session file>}`.
async fn wire_panes(client: &Client) -> HerdrResult<Vec<WirePane>> {
	client.request::<PaneList>("pane.list", &json!({}), None).await.map(|list| list.panes)
}

impl Herdr for HerdrClient {
	fn started_server(&self) -> bool {
		true
	}

	fn ping(&self) -> BoxFuture<'_, HerdrResult<Option<String>>> {
		Box::pin(self.client.ping())
	}

	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, HerdrResult<String>> {
		Box::pin(HerdrClient::ensure_workspace(self, label, cwd, command, env))
	}

	fn workspaces(&self) -> BoxFuture<'_, HerdrResult<Vec<WorkspaceInfo>>> {
		Box::pin(async move { Ok(self.snapshot().await?.workspaces) })
	}

	fn snapshot(&self) -> BoxFuture<'_, HerdrResult<HerdrSnapshot>> {
		Box::pin(HerdrClient::snapshot(self))
	}

	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(HerdrClient::close_workspace(self, workspace_id))
	}

	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, HerdrResult<TabPane>> {
		Box::pin(HerdrClient::create_tab(self, workspace_id, tab))
	}

	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, HerdrResult<WorktreeCreated>> {
		Box::pin(HerdrClient::create_worktree(self, workspace_id, opts))
	}

	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// workspace.move {workspace_id, insert_index} (schema `WorkspaceMoveParams`).
			let params = json!({"workspace_id": workspace_id, "insert_index": index});
			self.client.request_value("workspace.move", &params, None).await.map(drop)
		})
	}

	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, HerdrResult<TabPane>> {
		Box::pin(HerdrClient::respawn(self, pane_id, opts))
	}

	fn split_beside<'a>(&'a self, pane_id: &'a str, opts: &'a SplitOptions) -> BoxFuture<'a, HerdrResult<String>> {
		Box::pin(HerdrClient::split_beside(self, pane_id, opts))
	}

	fn prompt<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// agent.prompt {target, text}: a bracketed paste then Enter into the pane's agent; refuses with
			// `agent_blocked` or `agent_not_ready` (socket-api.mdx "Raw methods"; herdr
			// `src/app/api/agents.rs:140-207`).
			let params = json!({"target": pane_id, "text": pane_text(text)});
			self.client.request_value("agent.prompt", &params, None).await.map(drop)
		})
	}

	fn send_text<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			let params = json!({"pane_id": pane_id, "text": pane_text(text)});
			self.client.request_value("pane.send_text", &params, None).await.map(drop)
		})
	}

	fn stop<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move { HerdrClient::send_keys(self, pane_id, &["esc"]).await })
	}

	fn submit<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move { HerdrClient::send_keys(self, pane_id, &["enter"]).await })
	}

	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
			HerdrClient::send_keys(self, pane_id, &keys).await
		})
	}

	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, HerdrResult<NotificationResult>> {
		Box::pin(HerdrClient::notify(self, note))
	}

	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// pane.focus {pane_id} switches to the pane's workspace and tab and marks it seen.
			self.client.request_value("pane.focus", &json!({"pane_id": pane_id}), None).await.map(drop)
		})
	}

	fn open_plugin_pane<'a>(&'a self, opts: &'a PluginPaneOptions) -> BoxFuture<'a, HerdrResult<Option<String>>> {
		Box::pin(HerdrClient::open_plugin_pane(self, opts))
	}

	fn close_popup(&self) -> BoxFuture<'_, HerdrResult<bool>> {
		Box::pin(HerdrClient::close_popup(self))
	}

	fn set_split_ratio<'a>(&'a self, tab_id: &'a str, ratio: f64) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// layout.set_split_ratio {tab_id, path, ratio}: `path` [] is the tab's top split; `ratio` is the first
			// side's share.
			let params = json!({"tab_id": tab_id, "path": [], "ratio": ratio});
			self.client.request_value("layout.set_split_ratio", &params, None).await.map(drop)
		})
	}

	fn set_agent_view<'a>(&'a self, view: &'a AgentView) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// agent.view.set {source, label, filter, sort} → `{type: "agent_view", active, source}` (schema
			// `AgentViewSetParams`); a plugin's source is `plugin:<id>`, refused while the plugin is missing or
			// disabled.
			let mut params = Map::new();
			params.insert("source".to_owned(), json!(view.source));
			if let Some(label) = &view.label {
				params.insert("label".to_owned(), json!(label));
			}
			if let Some(filter) = &view.filter {
				params.insert("filter".to_owned(), filter.clone());
			}
			if let Some(sort) = &view.sort {
				params.insert("sort".to_owned(), Value::Array(sort.clone()));
			}
			self.client.request_value("agent.view.set", &Value::Object(params), None).await.map(drop)
		})
	}

	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<String>> {
		Box::pin(async move {
			// pane.read {pane_id, source} → `{type: "pane_read", read: {text, …}}` (schema `PaneReadParams`,
			// `PaneReadResult`); `visible` is the rendered viewport, ANSI stripped by default.
			let params = json!({"pane_id": pane_id, "source": "visible"});
			let answer: PaneRead = self.client.request("pane.read", &params, None).await?;
			Ok(answer.read.text)
		})
	}

	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(HerdrClient::report_pane_metadata(self, pane_id, meta))
	}

	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, HerdrResult<()>> {
		Box::pin(async move {
			// workspace.report_metadata {workspace_id, source, tokens, ttl_ms}.
			let mut params = json!({"workspace_id": workspace_id, "source": SOURCE, "tokens": tokens_json(tokens)});
			if let (Some(ttl), Value::Object(map)) = (ttl_ms.filter(|ttl| *ttl != 0), &mut params) {
				map.insert("ttl_ms".to_owned(), json!(ttl));
			}
			self.client.request_value("workspace.report_metadata", &params, None).await.map(drop)
		})
	}

	fn panes(&self) -> BoxFuture<'_, HerdrResult<Vec<PaneInfo>>> {
		Box::pin(async move { Ok(self.wire_panes().await?.into_iter().map(pane_info).collect()) })
	}

	fn pane_root<'a>(&'a self, pane_id: &'a str, timeout_ms: Option<i64>) -> BoxFuture<'a, HerdrResult<Option<u32>>> {
		Box::pin(HerdrClient::pane_root(self, pane_id, timeout_ms))
	}

	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		HerdrClient::watch(self, pane_ids, on_change)
	}

	fn stop_server(&self) -> BoxFuture<'_, HerdrResult<()>> {
		Box::pin(async move { self.client.stop_server().await.map(drop) })
	}

	fn close(&self) {
		let all: Vec<Arc<WatchCore>> = locked(&self.watches).values().cloned().collect();
		for core in all {
			core.stop();
		}
	}
}

/// Tells a watch's `on_change` each pane's state once per change.
struct Emitter {
	last: Mutex<HashMap<String, AgentState>>,
	stopped: AtomicBool,
	on_change: OnChange,
}

impl Emitter {
	fn emit(&self, pane_id: &str, workspace_id: &str, state: AgentState) {
		if self.stopped.load(Ordering::SeqCst) {
			return;
		}
		{
			let mut last = locked(&self.last);
			if last.get(pane_id) == Some(&state) {
				return;
			}
			last.insert(pane_id.to_owned(), state);
		}
		(self.on_change)(WatchEvent { pane_id: pane_id.to_owned(), workspace_id: workspace_id.to_owned(), state });
	}
}

/// The subscription a watch holds: the pane ids it is for and the task that reads it (its end is the connection's).
struct Subscription {
	key: String,
	reader: AbortHandle,
}

#[derive(Default)]
struct WatchState {
	subscription: Option<Subscription>,
	/// The connection of a subscribe still waiting for herdr's answer.
	subscribing: Option<AbortHandle>,
	missing: bool,
	wanted_key: String,
	timer: Option<AbortHandle>,
}

/// One `watch`: its pane set, its subscription and the tick that keeps them in step.
struct WatchCore {
	client: Arc<Client>,
	pane_ids: PaneIds,
	emitter: Arc<Emitter>,
	state: Mutex<WatchState>,
	id: u64,
	registry: Weak<Watches>,
}

type Pushed = Arc<Mutex<HashSet<String>>>;

impl WatchCore {
	fn start(self: &Arc<Self>, interval_ms: i64) {
		let core = self.clone();
		let tick = tokio::spawn(async move {
			loop {
				core.sync().await;
				tokio::time::sleep(millis(interval_ms)).await;
			}
		});
		locked(&self.state).timer = Some(tick.abort_handle());
	}

	fn stop(&self) {
		self.emitter.stopped.store(true, Ordering::SeqCst);
		{
			let mut state = locked(&self.state);
			for handle in [state.timer.take(), state.subscribing.take(), state.subscription.take().map(|s| s.reader)]
				.into_iter()
				.flatten()
			{
				handle.abort();
			}
		}
		if let Some(registry) = self.registry.upgrade() {
			locked(&registry).remove(&self.id);
		}
	}

	fn stopped(&self) -> bool {
		self.emitter.stopped.load(Ordering::SeqCst)
	}

	/// Subscribes; returns the panes whose state a pushed event has carried since `subscription_started`.
	async fn subscribe(&self, ids: &[String]) -> HerdrResult<Pushed> {
		// events.subscribe {subscriptions: [{type: "pane.agent_status_changed", pane_id}]}: one entry per pane (the
		// type requires `pane_id`); acknowledged with `{type: "subscription_started"}`, then pushes `{event:
		// "pane.agent_status_changed", data: {pane_id, workspace_id, agent_status}}`. An unknown pane rejects the
		// whole request; falling behind sends error `events_lost`; both close the connection (socket-api.mdx "Event
		// subscriptions").
		let pushed: Pushed = Arc::new(Mutex::new(HashSet::new()));
		let opened = tokio::time::timeout(millis(self.client.timeout_ms), self.open_subscription(ids, &pushed)).await;
		let reader = locked(&self.state).subscribing.take();
		let abort = |reader: Option<AbortHandle>| {
			if let Some(reader) = reader {
				reader.abort();
			}
		};
		match opened {
			Ok(Ok(())) => {}
			Ok(Err(error)) => {
				abort(reader);
				return Err(error);
			}
			Err(_) => {
				abort(reader);
				return Err(HerdrError::new("timeout", "herdr didn't answer events.subscribe"));
			}
		}
		let Some(reader) = reader else { return Ok(pushed) };
		if self.stopped() {
			reader.abort();
			return Ok(pushed);
		}
		let key = ids.join(",");
		if let Some(old) = locked(&self.state).subscription.replace(Subscription { key, reader }) {
			old.reader.abort();
		}
		Ok(pushed)
	}

	/// Connects, sends the subscribe and waits for its answer; the reader task it starts is left in `subscribing`.
	async fn open_subscription(&self, ids: &[String], pushed: &Pushed) -> HerdrResult<()> {
		let id = next_id();
		let stream = connect_stream(&self.client.path).await?;
		let (read, mut write) = tokio::io::split(stream);
		let subscriptions: Vec<Value> =
			ids.iter().map(|pane_id| json!({"type": "pane.agent_status_changed", "pane_id": pane_id})).collect();
		let request = json!({"id": id, "method": "events.subscribe", "params": {"subscriptions": subscriptions}});
		write
			.write_all(format!("{request}\n").as_bytes())
			.await
			.map_err(|_| HerdrError::new("transport", "subscription closed"))?;
		let (started, answered) = oneshot::channel();
		let reader = tokio::spawn(read_subscription((read, write), id, started, pushed.clone(), self.emitter.clone()));
		locked(&self.state).subscribing = Some(reader.abort_handle());
		answered.await.unwrap_or_else(|_| Err(HerdrError::new("transport", "subscription closed")))
	}

	async fn sync(&self) {
		if self.stopped() {
			return;
		}
		if self.sync_once().await.is_err() {
			// herdr down, or a pane closed while subscribing (herdr rejects the whole request): the next tick retries.
			locked(&self.state).wanted_key = String::new();
		}
	}

	async fn sync_once(&self) -> HerdrResult<()> {
		let mut wanted = (self.pane_ids)();
		wanted.sort();
		wanted.dedup();
		let key = wanted.join(",");
		let live = locked(&self.state).subscription.as_ref().is_some_and(|s| !s.reader.is_finished());
		{
			let state = locked(&self.state);
			if key == state.wanted_key && !state.missing && (live || wanted.is_empty()) {
				return Ok(());
			}
		}
		let present: HashSet<String> = wire_panes(&self.client).await?.into_iter().map(|pane| pane.pane_id).collect();
		let ids: Vec<String> = wanted.iter().filter(|id| present.contains(*id)).cloned().collect();
		{
			let mut state = locked(&self.state);
			state.missing = ids.len() < wanted.len();
			state.wanted_key = key;
		}
		locked(&self.emitter.last).retain(|id, _| ids.contains(id));
		if ids.is_empty() {
			if let Some(old) = locked(&self.state).subscription.take() {
				old.reader.abort();
			}
			return Ok(());
		}
		if live && locked(&self.state).subscription.as_ref().is_some_and(|s| s.key == ids.join(",")) {
			return Ok(());
		}
		let pushed = self.subscribe(&ids).await?;
		// Reconcile after `subscription_started` with an authoritative read: the state now, for each pane. A pane whose
		// event arrived meanwhile already has a newer state than this read.
		let fresh = wire_panes(&self.client).await?;
		for id in &ids {
			let Some(pane) = fresh.iter().find(|pane| &pane.pane_id == id) else { continue };
			if !locked(&pushed).contains(id) {
				self.emitter.emit(id, &pane.workspace_id, AgentState::of(pane.agent_status.as_deref()));
			}
		}
		Ok(())
	}
}

/// Reads a subscription's connection: its answer to `events.subscribe`, then the events it pushes. Holds both halves,
/// so the connection closes when this task ends or is aborted.
async fn read_subscription<R, W>(
	(read, _write): (R, W),
	id: String,
	started: oneshot::Sender<HerdrResult<()>>,
	pushed: Pushed,
	emitter: Arc<Emitter>,
) where
	R: AsyncRead + Unpin,
	W: Send,
{
	let mut started = Some(started);
	let mut lines = LineReader::new(read);
	while let Some(line) = lines.next_line().await {
		let Some(message) = parse(&line) else { continue };
		if message.id.as_deref() == Some(id.as_str()) {
			let answer = match message.error {
				Some((code, message)) => {
					Err(HerdrError::new(code.unwrap_or_else(|| "error".to_owned()), message.unwrap_or_default()))
				}
				None => Ok(()),
			};
			if let Some(started) = started.take() {
				let _ = started.send(answer);
			}
		} else if message.event.as_deref() == Some("pane.agent_status_changed")
			&& let Some(pane_id) = message.data.get("pane_id").and_then(Value::as_str).filter(|p| !p.is_empty())
		{
			locked(&pushed).insert(pane_id.to_owned());
			let workspace_id = message.data.get("workspace_id").and_then(Value::as_str).unwrap_or("");
			let status = message.data.get("agent_status").and_then(Value::as_str);
			emitter.emit(pane_id, workspace_id, AgentState::of(status));
		}
	}
	if let Some(started) = started.take() {
		let _ = started.send(Err(HerdrError::new("transport", "subscription closed")));
	}
}
