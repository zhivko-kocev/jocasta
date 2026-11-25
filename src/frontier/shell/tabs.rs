//! The shell's model (0683): workspaces (Jocasta's projects and threads' worktrees), their tabs, one pane each, and which
//! tab is in front. The UI thread reads it to draw; the service changes it through `own_surface::OwnSurface`. Agent
//! state is not kept here: it comes from the bridge (0675) through the lookup the service sets.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use super::pty::{Launch, Pane, Ui, Wake};
use crate::frontier::server::surface::{AgentState, PaneInfo, PaneMetadata, SurfaceSnapshot, Tokens, WorkspaceInfo};

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a pane's bridge says (`bridge.rs`'s `PaneAgent` and hello): the sidebar's state and the session file.
#[derive(Debug, Clone, Default)]
pub struct AgentInfo {
	/// The pane's omp connected to the bridge.
	pub connected: bool,
	pub working: bool,
	/// A dialog of omp's waits on him (an approval, a plan review, a question).
	pub input_open: bool,
	/// The omp session file the pane reported.
	pub session: Option<PathBuf>,
}

impl AgentInfo {
	/// The state the sidebar and the service's `watch` show.
	pub fn state(&self) -> AgentState {
		if !self.connected {
			AgentState::Unknown
		} else if self.input_open {
			AgentState::Blocked
		} else if self.working {
			AgentState::Working
		} else {
			AgentState::Idle
		}
	}
}

/// Looks a pane's bridge state up by pane id; set once the service has its bridge.
pub type AgentLookup = Arc<dyn Fn(&str) -> AgentInfo + Send + Sync>;

/// What `report_pane_metadata` last said of a pane, until it expires.
#[derive(Default)]
struct Meta {
	tokens: Vec<(String, String)>,
	labels: Vec<(AgentState, String)>,
	until: Option<i64>,
}

struct Tab {
	id: String,
	pane_id: String,
	label: String,
	cwd: PathBuf,
	pane: Arc<Pane>,
	terminal_id: String,
	meta: Meta,
}

struct Workspace {
	id: String,
	label: String,
	cwd: PathBuf,
	tabs: Vec<Tab>,
	tokens: Vec<(String, String)>,
	tokens_until: Option<i64>,
}

#[derive(Default)]
struct Model {
	workspaces: Vec<Workspace>,
	/// The pane in front.
	active: Option<String>,
	/// The size a pane starts with: the main area of the frame.
	area: (u16, u16),
}

/// What starts a pane.
pub struct Start {
	/// The program and its arguments; empty runs the user's shell.
	pub argv: Vec<String>,
	pub cwd: PathBuf,
	pub env: Vec<(String, String)>,
}

/// The tab in front.
pub struct Front {
	pub pane: Arc<Pane>,
	pub pane_id: String,
	pub workspace_id: String,
	/// The workspace's label, for the window title.
	pub workspace_label: String,
}

/// One row of the sidebar's agents list.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
	pub pane_id: String,
	pub label: String,
	pub state: AgentState,
	/// The second line: what the pane is doing, as the service words it (`review ready`, a question) or the state's name.
	pub text: String,
	pub model: Option<String>,
	pub cost: Option<String>,
	pub running: Option<String>,
	pub pinned: bool,
	pub front: bool,
}

/// The shell's panes: see the module's text.
pub struct Hub {
	model: Mutex<Model>,
	ui: Sender<Ui>,
	/// One signal per waiter (the service's refresh, each surface watch): `notify_one` keeps a wake for each, where one
	/// shared signal would wake only one of them.
	changed: Mutex<Vec<std::sync::Weak<tokio::sync::Notify>>>,
	lookup: OnceLock<AgentLookup>,
	next: AtomicU64,
}

fn default_shell() -> String {
	if cfg!(windows) {
		std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned())
	} else {
		std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
	}
}

/// `name`'s value in the tokens, if set and not expired.
fn token<'a>(tokens: &'a [(String, String)], name: &str) -> Option<&'a String> {
	tokens.iter().find(|(known, _)| known == name).map(|(_, value)| value)
}

fn merge_tokens(into: &mut Vec<(String, String)>, tokens: &Tokens) {
	for (name, value) in tokens {
		into.retain(|(known, _)| known != name);
		if let Some(value) = value {
			into.push((name.clone(), value.clone()));
		}
	}
}

impl Hub {
	pub fn new(ui: Sender<Ui>) -> Arc<Self> {
		Arc::new(Self {
			model: Mutex::new(Model { area: (80, 24), ..Model::default() }),
			ui,
			changed: Mutex::new(Vec::new()),
			lookup: OnceLock::new(),
			next: AtomicU64::new(1),
		})
	}

	/// A signal of its own that resolves at the next change of the panes the service reads.
	pub fn changed(&self) -> Arc<tokio::sync::Notify> {
		let notify = Arc::new(tokio::sync::Notify::new());
		locked(&self.changed).push(Arc::downgrade(&notify));
		notify
	}

	/// Where the bridge's state of a pane comes from; set once.
	pub fn set_lookup(&self, lookup: AgentLookup) {
		let _ = self.lookup.set(lookup);
	}

	fn info(&self, pane_id: &str) -> AgentInfo {
		self.lookup.get().map(|lookup| lookup(pane_id)).unwrap_or_default()
	}

	fn model(&self) -> MutexGuard<'_, Model> {
		locked(&self.model)
	}

	/// Everything changed that the frame or the service reads.
	fn touched(&self) {
		locked(&self.changed).retain(|waiter| waiter.upgrade().inspect(|notify| notify.notify_one()).is_some());
		let _ = self.ui.send(Ui::Changed);
	}

	/// The size a new pane starts with, and what the UI last drew its main area as.
	pub fn set_area(&self, cols: u16, rows: u16) {
		self.model().area = (cols.max(1), rows.max(1));
	}

	/// Starts a pane's process; not yet in any tab.
	pub fn start(&self, start: &Start) -> std::io::Result<Arc<Pane>> {
		let (cols, rows) = self.model().area;
		let uid = self.next.fetch_add(1, Ordering::SeqCst);
		let argv = if start.argv.is_empty() { vec![default_shell()] } else { start.argv.clone() };
		let launch = Launch { argv: &argv, cwd: &start.cwd, env: &start.env, cols, rows };
		Pane::spawn(uid, self.ui.clone(), &launch).map(Arc::new)
	}

	/// A sleeping pane (0689), running nothing; not yet in any tab.
	pub fn start_still(&self, text: &str, wake: Wake) -> Arc<Pane> {
		let area = self.model().area;
		let uid = self.next.fetch_add(1, Ordering::SeqCst);
		Arc::new(Pane::still(uid, self.ui.clone(), area, text, wake))
	}

	fn new_tab(&self, label: &str, cwd: &Path, pane: Arc<Pane>) -> Tab {
		let n = self.next.fetch_add(1, Ordering::SeqCst);
		Tab {
			id: format!("t{n}"),
			pane_id: format!("p{}", pane.uid),
			label: label.to_owned(),
			cwd: cwd.to_path_buf(),
			terminal_id: format!("term-{}", pane.uid),
			pane,
			meta: Meta::default(),
		}
	}

	/// A workspace labelled `label` with one tab, in front when it is the first. Returns its ids.
	pub fn add_workspace(&self, label: &str, cwd: &Path, pane: Arc<Pane>) -> (String, String, String) {
		let tab = self.new_tab(label, cwd, pane);
		let ids = (format!("w{}", self.next.fetch_add(1, Ordering::SeqCst)), tab.id.clone(), tab.pane_id.clone());
		{
			let mut model = self.model();
			if model.active.is_none() {
				model.active = Some(tab.pane_id.clone());
			}
			model.workspaces.push(Workspace {
				id: ids.0.clone(),
				label: label.to_owned(),
				cwd: cwd.to_path_buf(),
				tabs: vec![tab],
				tokens: Vec::new(),
				tokens_until: None,
			});
		}
		self.touched();
		ids
	}

	/// A tab in workspace `workspace_id`; `None` when there is none.
	pub fn add_tab(&self, workspace_id: &str, label: &str, cwd: &Path, pane: Arc<Pane>) -> Option<(String, String)> {
		let tab = self.new_tab(label, cwd, pane);
		let ids = (tab.id.clone(), tab.pane_id.clone());
		{
			let mut model = self.model();
			model.workspaces.iter_mut().find(|ws| ws.id == workspace_id)?.tabs.push(tab);
			if model.active.is_none() {
				model.active = Some(ids.1.clone());
			}
		}
		self.touched();
		Some(ids)
	}

	/// Puts `pane` in the place of the tab holding `pane_id`, which keeps its id and label. Returns the tab's id, the
	/// new pane's id and the pane that was there.
	pub fn replace(&self, pane_id: &str, cwd: &Path, pane: Arc<Pane>) -> Option<(String, String, Arc<Pane>)> {
		let new_id = format!("p{}", pane.uid);
		let terminal_id = format!("term-{}", pane.uid);
		let (tab_id, old) = {
			let mut model = self.model();
			let tab =
				model.workspaces.iter_mut().flat_map(|ws| ws.tabs.iter_mut()).find(|tab| tab.pane_id == pane_id)?;
			let old = std::mem::replace(&mut tab.pane, pane);
			tab.pane_id = new_id.clone();
			tab.terminal_id = terminal_id;
			tab.cwd = cwd.to_path_buf();
			tab.meta = Meta::default();
			let tab_id = tab.id.clone();
			if model.active.as_deref() == Some(pane_id) {
				model.active = Some(new_id.clone());
			}
			(tab_id, old)
		};
		self.touched();
		Some((tab_id, new_id, old))
	}

	/// Removes workspace `workspace_id` with its tabs; the panes it had, for the caller to end.
	pub fn remove_workspace(&self, workspace_id: &str) -> Vec<Arc<Pane>> {
		let panes = {
			let mut model = self.model();
			let Some(at) = model.workspaces.iter().position(|ws| ws.id == workspace_id) else { return Vec::new() };
			let ws = model.workspaces.remove(at);
			let panes: Vec<Arc<Pane>> = ws.tabs.into_iter().map(|tab| tab.pane).collect();
			Self::keep_front(&mut model);
			panes
		};
		self.touched();
		panes
	}

	/// A pane's process ended: its tab goes, and its workspace with its last tab (as herdr closed them); a pane a
	/// respawn already replaced is not in any tab.
	pub fn exited(&self, uid: u64) {
		let gone = {
			let mut model = self.model();
			let mut gone = false;
			for ws in &mut model.workspaces {
				let before = ws.tabs.len();
				ws.tabs.retain(|tab| tab.pane.uid != uid);
				gone |= ws.tabs.len() != before;
			}
			model.workspaces.retain(|ws| !ws.tabs.is_empty());
			if gone {
				Self::keep_front(&mut model);
			}
			gone
		};
		if gone {
			self.touched();
		}
	}

	/// The tab in front is one that exists: when it went, the first workspace's first tab.
	fn keep_front(model: &mut Model) {
		let exists = |model: &Model, id: &str| {
			model.workspaces.iter().flat_map(|ws| ws.tabs.iter()).any(|tab| tab.pane_id == id)
		};
		if model.active.as_deref().is_some_and(|id| exists(model, id)) {
			return;
		}
		model.active = model.workspaces.first().and_then(|ws| ws.tabs.first()).map(|tab| tab.pane_id.clone());
	}

	/// Every pane, for ending them all when Jocasta stops.
	pub fn all_panes(&self) -> Vec<Arc<Pane>> {
		let model = self.model();
		model.workspaces.iter().flat_map(|ws| ws.tabs.iter()).map(|tab| Arc::clone(&tab.pane)).collect()
	}

	pub fn find_workspace(&self, label: &str) -> Option<String> {
		self.model().workspaces.iter().find(|ws| ws.label == label).map(|ws| ws.id.clone())
	}

	pub fn workspaces(&self) -> Vec<WorkspaceInfo> {
		let model = self.model();
		model
			.workspaces
			.iter()
			.map(|ws| WorkspaceInfo {
				id: ws.id.clone(),
				label: ws.label.clone(),
				cwd: ws.tabs.first().map(|tab| tab.cwd.clone()).or_else(|| Some(ws.cwd.clone())),
			})
			.collect()
	}

	pub fn snapshot(&self) -> SurfaceSnapshot {
		let model = self.model();
		let front = model.active.clone();
		let mut panes = Vec::new();
		for ws in &model.workspaces {
			for tab in &ws.tabs {
				let info = self.info(&tab.pane_id);
				panes.push(PaneInfo {
					pane_id: tab.pane_id.clone(),
					tab_id: tab.id.clone(),
					workspace_id: ws.id.clone(),
					cwd: Some(tab.cwd.clone()),
					agent_state: info.state(),
					agent: info.connected.then(|| "omp".to_owned()),
					session_path: info.session.clone(),
					label: Some(tab.label.clone()),
					terminal_id: Some(tab.terminal_id.clone()),
					focused: front.as_deref() == Some(tab.pane_id.as_str()),
				});
			}
		}
		let workspaces = model
			.workspaces
			.iter()
			.map(|ws| WorkspaceInfo {
				id: ws.id.clone(),
				label: ws.label.clone(),
				cwd: ws.tabs.first().map(|tab| tab.cwd.clone()).or_else(|| Some(ws.cwd.clone())),
			})
			.collect();
		SurfaceSnapshot { workspaces, panes }
	}

	/// The pane `pane_id`, if it exists.
	pub fn pane(&self, pane_id: &str) -> Option<Arc<Pane>> {
		let model = self.model();
		model
			.workspaces
			.iter()
			.flat_map(|ws| ws.tabs.iter())
			.find(|tab| tab.pane_id == pane_id)
			.map(|tab| Arc::clone(&tab.pane))
	}

	/// Brings the tab holding `pane_id` to the front; false when there is none.
	pub fn focus(&self, pane_id: &str) -> bool {
		{
			let mut model = self.model();
			if !model.workspaces.iter().flat_map(|ws| ws.tabs.iter()).any(|tab| tab.pane_id == pane_id) {
				return false;
			}
			model.active = Some(pane_id.to_owned());
		}
		self.touched();
		true
	}

	/// Moves workspace `workspace_id` to `index` in the list.
	pub fn move_workspace(&self, workspace_id: &str, index: usize) -> bool {
		{
			let mut model = self.model();
			let Some(at) = model.workspaces.iter().position(|ws| ws.id == workspace_id) else { return false };
			let ws = model.workspaces.remove(at);
			let index = index.min(model.workspaces.len());
			model.workspaces.insert(index, ws);
		}
		self.touched();
		true
	}

	/// A pane's display tokens and state labels, as `report_pane_metadata` takes them; they expire `ttl_ms` after.
	pub fn pane_metadata(&self, pane_id: &str, meta: &PaneMetadata, now: i64) -> bool {
		{
			let mut model = self.model();
			let Some(tab) =
				model.workspaces.iter_mut().flat_map(|ws| ws.tabs.iter_mut()).find(|tab| tab.pane_id == pane_id)
			else {
				return false;
			};
			if tab.meta.until.is_some_and(|until| until <= now) {
				tab.meta = Meta::default();
			}
			if let Some(tokens) = &meta.tokens {
				merge_tokens(&mut tab.meta.tokens, tokens);
			}
			if let Some(labels) = &meta.state_labels {
				tab.meta.labels.clone_from(labels);
			}
			if let Some(ttl) = meta.ttl_ms {
				tab.meta.until = Some(now + ttl);
			}
		}
		let _ = self.ui.send(Ui::Changed);
		true
	}

	/// A workspace's display tokens.
	pub fn workspace_metadata(&self, workspace_id: &str, tokens: &Tokens, ttl_ms: Option<i64>, now: i64) -> bool {
		{
			let mut model = self.model();
			let Some(ws) = model.workspaces.iter_mut().find(|ws| ws.id == workspace_id) else { return false };
			if ws.tokens_until.is_some_and(|until| until <= now) {
				ws.tokens.clear();
			}
			merge_tokens(&mut ws.tokens, tokens);
			if let Some(ttl) = ttl_ms {
				ws.tokens_until = Some(now + ttl);
			}
		}
		let _ = self.ui.send(Ui::Changed);
		true
	}

	/// The tab in front.
	pub fn front(&self) -> Option<Front> {
		let model = self.model();
		let id = model.active.as_deref()?;
		model.workspaces.iter().find_map(|ws| {
			ws.tabs.iter().find(|tab| tab.pane_id == id).map(|tab| Front {
				pane: Arc::clone(&tab.pane),
				pane_id: tab.pane_id.clone(),
				workspace_id: ws.id.clone(),
				workspace_label: ws.label.clone(),
			})
		})
	}

	/// The sidebar's rows, the boss's first (the `pin` token the service reports on its pane, 0667), then the panes
	/// awaiting him, those with a review or plan ready, working, idle and the rest, each group in the order made.
	pub fn rows(&self, now: i64) -> Vec<Row> {
		let model = self.model();
		let mut rows: Vec<(u8, usize, Row)> = Vec::new();
		for (order, (ws, tab)) in
			model.workspaces.iter().flat_map(|ws| ws.tabs.iter().map(move |tab| (ws, tab))).enumerate()
		{
			let live = tab.meta.until.is_none_or(|until| until > now);
			let (tokens, labels) = if live { (&tab.meta.tokens[..], &tab.meta.labels[..]) } else { (&[][..], &[][..]) };
			let state = self.info(&tab.pane_id).state();
			let label_of = |state| labels.iter().find(|(known, _)| *known == state).map(|(_, text)| text.clone());
			let ready = label_of(AgentState::Done);
			let (rank, text) = match state {
				AgentState::Blocked => (1, label_of(AgentState::Blocked).unwrap_or_else(|| "awaiting you".to_owned())),
				AgentState::Working => (3, "working".to_owned()),
				AgentState::Idle if ready.is_some() => (2, ready.unwrap_or_default()),
				AgentState::Idle => (4, label_of(AgentState::Idle).unwrap_or_else(|| "idle".to_owned())),
				AgentState::Done => (2, label_of(AgentState::Done).unwrap_or_else(|| "done".to_owned())),
				AgentState::Unknown if tab.pane.asleep() => (5, "asleep".to_owned()),
				AgentState::Unknown => (5, "starting".to_owned()),
			};
			let pinned = token(tokens, "pin").is_some();
			let running = if ws.tokens_until.is_none_or(|until| until > now) {
				token(&ws.tokens, "running").cloned()
			} else {
				None
			};
			let row = Row {
				pane_id: tab.pane_id.clone(),
				label: ws.label.clone(),
				state,
				text,
				model: token(tokens, "model").cloned(),
				cost: token(tokens, "cost").cloned(),
				running,
				pinned,
				front: model.active.as_deref() == Some(tab.pane_id.as_str()),
			};
			rows.push((if pinned { 0 } else { rank }, order, row));
		}
		rows.sort_by_key(|(rank, order, _)| (*rank, *order));
		rows.into_iter().map(|(_, _, row)| row).collect()
	}

	/// Brings the pane `delta` rows after the one in front (before when negative) forward, wrapping.
	pub fn cycle(&self, delta: isize, now: i64) {
		let rows = self.rows(now);
		let Some(at) = rows.iter().position(|row| row.front) else { return };
		let count = isize::try_from(rows.len()).unwrap_or(1);
		let next = (isize::try_from(at).unwrap_or(0) + delta).rem_euclid(count);
		if let Some(row) = usize::try_from(next).ok().and_then(|next| rows.get(next)) {
			self.focus(&row.pane_id);
		}
	}

	/// Brings row `index` (0 is the first) forward.
	pub fn switch_row(&self, index: usize, now: i64) {
		if let Some(row) = self.rows(now).get(index) {
			self.focus(&row.pane_id);
		}
	}

	/// The agent state of `pane_id` now, and its workspace's id, for the service's `watch`.
	pub fn state_of(&self, pane_id: &str) -> Option<(String, AgentState)> {
		let workspace = {
			let model = self.model();
			model.workspaces.iter().find(|ws| ws.tabs.iter().any(|tab| tab.pane_id == pane_id))?.id.clone()
		};
		Some((workspace, self.info(pane_id).state()))
	}
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
