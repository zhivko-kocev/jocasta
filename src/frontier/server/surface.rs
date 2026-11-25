//! The surface the service drives (principle 7; 0392, renamed by 0683): workspaces (Jocasta's projects), tabs and panes,
//! their processes, notifications and the sidebar's metadata. [`Surface`] is implemented in-process by
//! `own_surface::OwnSurface` over the shell's state; the in-memory stand-in the service's tests use is
//! [`fake_surface::FakeSurface`].
//!
//! Every [`Surface`] method returns a boxed future, so a `dyn Surface` can be shared (`Arc<dyn Surface>`).

use std::path::{Path, PathBuf};

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::shared::errors::{Error, Result};

#[cfg(test)]
#[path = "fake_surface.rs"]
pub(crate) mod fake_surface;

/// What a failing surface call says.
pub type SurfaceResult<T> = std::result::Result<T, SurfaceError>;

/// An environment for a process the surface starts, in the order it was written.
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

/// What a pane's agent is doing, from its bridge (0675).
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
	/// The state's name as Jocasta's JSON spells it.
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

/// One pane as the surface lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneInfo {
	pub pane_id: String,
	pub tab_id: String,
	pub workspace_id: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cwd: Option<PathBuf>,
	pub agent_state: AgentState,
	/// The agent in the pane (`omp` once its bridge connected).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub agent: Option<String>,
	/// The omp session file the pane's bridge reported.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub session_path: Option<PathBuf>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub label: Option<String>,
	/// A new id each time the pane's process is started (a respawn).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub terminal_id: Option<String>,
	/// The one pane in front (its workspace, tab and pane) (0484); serialized only when
	/// true.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub focused: bool,
}

/// One workspace, with the folder of its first pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
	pub id: String,
	pub label: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cwd: Option<PathBuf>,
}

/// The workspaces and panes read together, once (the refresh tick's view).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SurfaceSnapshot {
	pub workspaces: Vec<WorkspaceInfo>,
	pub panes: Vec<PaneInfo>,
}

/// What showing a notification came to.
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

/// The program word of a sleeping pane (0689): `[ASLEEP, <text>]` is a pane the shell draws itself, running nothing,
/// that shows `<text>` and asks for its wake (the pane key in its env) on a key.
pub const ASLEEP: &str = "jocasta:asleep";

/// The notification's sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
	Request,
	None,
}

/// A system notification.
#[derive(Debug, Clone)]
pub struct Note {
	pub title: String,
	pub body: Option<String>,
	pub sound: Sound,
}

/// What `report_pane_metadata` reports.
#[derive(Debug, Clone, Default)]
pub struct PaneMetadata {
	pub tokens: Option<Tokens>,
	pub state_labels: Option<Vec<(AgentState, String)>>,
	pub ttl_ms: Option<i64>,
}

/// The surface as the rest of Jocasta uses it.
pub trait Surface: Send + Sync {
	/// Always true: the surface is Jocasta's own (0495).
	fn started_server(&self) -> bool;
	/// The surface's version; fails when it doesn't answer.
	fn ping(&self) -> BoxFuture<'_, SurfaceResult<Option<String>>>;
	/// Workspace id of the workspace labelled `label`; creates it in `cwd` when missing, its one tab running `command`
	/// (with `env`) when given (else the default shell).
	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, SurfaceResult<String>>;
	fn workspaces(&self) -> BoxFuture<'_, SurfaceResult<Vec<WorkspaceInfo>>>;
	/// Workspaces and panes read together, once.
	fn snapshot(&self) -> BoxFuture<'_, SurfaceResult<SurfaceSnapshot>>;
	/// Closes a workspace with its panes and ends every process they ran, with all they started (0486), so nothing
	/// holds its folder.
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>>;
	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, SurfaceResult<TabPane>>;
	/// A git worktree for `branch` at `path` (a new branch starts from `base`, else HEAD) and a pane running `command`
	/// in it. Every worktree is its own workspace, so the result names that workspace too. `prepare` runs once the
	/// checkout exists and before the pane starts; what it returns joins the pane's env (0457, 0473).
	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, SurfaceResult<WorktreeCreated>>;
	/// Moves a workspace to `index` in the list (0 is the first, Alt+1).
	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, SurfaceResult<()>>;
	/// Replaces the pane's tab with one pane running `command` (the tab keeps its label). Returns the new pane.
	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, SurfaceResult<TabPane>>;
	/// Keys by their names (`f9`) to the pane, as typed: the one input that still goes through the terminal,
	/// push-to-talk (F9), which omp reads as a key, not as a message (0675).
	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, SurfaceResult<()>>;
	/// A system notification (0554): title and body as given, with the request sound or none. Resolves `Shown`, or why
	/// not.
	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, SurfaceResult<NotificationResult>>;
	/// Shows the pane's tab.
	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>>;
	/// What the pane shows now, as plain text (its visible grid).
	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<String>>;
	/// Display-only tokens and state labels; `ttl_ms` makes the sidebar drop them when they aren't reported again in
	/// time, so a stopped service leaves nothing stale.
	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata)
	-> BoxFuture<'a, SurfaceResult<()>>;
	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, SurfaceResult<()>>;
	fn panes(&self) -> BoxFuture<'_, SurfaceResult<Vec<PaneInfo>>>;
	/// The process started for the pane (omp in Jocasta's panes), `None` when it has none.
	fn pane_root<'a>(&'a self, pane_id: &'a str, timeout_ms: Option<i64>) -> BoxFuture<'a, SurfaceResult<Option<u32>>>;
	/// Calls `on_change` when a watched pane's agent state changes, starting with each pane's current state. The pane
	/// set is re-read from `pane_ids` every interval. Returns the function that stops watching. Needs a running tokio
	/// runtime.
	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch;
	/// Ends every pane's processes, with all they started (0486), when Jocasta stops.
	fn stop_server(&self) -> BoxFuture<'_, SurfaceResult<()>>;
	/// Stops every watch.
	fn close(&self);
}

/// A failing surface call: a code and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceError {
	pub code: String,
	pub message: String,
}

impl SurfaceError {
	pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
		Self { code: code.into(), message: message.into() }
	}
}

impl std::fmt::Display for SurfaceError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for SurfaceError {}

impl From<SurfaceError> for Error {
	fn from(error: SurfaceError) -> Self {
		Self::Other(error.message)
	}
}

impl From<Error> for SurfaceError {
	/// A failure of a caller's own callback (`prepare`) passing through a surface call.
	fn from(error: Error) -> Self {
		Self::new("error", error.to_string())
	}
}

/// A ping while stopping: how long the stop waits for the surface to answer.
pub const STOP_PING_MS: i64 = 2_000;

/// `{...base, ...extra}`: a name set again keeps its place and takes the new value.
pub fn merge_env(base: Option<&Env>, extra: &Env) -> Env {
	let mut merged = base.cloned().unwrap_or_default();
	for (name, value) in extra {
		match merged.iter_mut().find(|(have, _)| have == name) {
			Some(slot) => slot.1 = value.clone(),
			None => merged.push((name.clone(), value.clone())),
		}
	}
	merged
}

/// `text` for a pane's input with ESC, the other C0 controls (tab and newline kept), DEL and the C1 controls removed:
/// a bracketed paste carries it as it is, so an ESC `[201~` in outside text would end the paste and type the rest as
/// keys (SEC-2).
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

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
