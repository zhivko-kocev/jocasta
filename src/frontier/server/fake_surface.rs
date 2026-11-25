//! The in-memory [`Surface`] the service's tests use (`FakeSurface`): workspaces, panes and agent states, every call
//! recorded.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use super::{
	AgentState, Env, Note, NotificationResult, OnChange, PaneIds, PaneInfo, PaneMetadata, RespawnOptions, StopWatch,
	Surface, SurfaceError, SurfaceResult, SurfaceSnapshot, TabPane, TabSpec, Tokens, WatchEvent, WorkspaceInfo,
	WorktreeCreated, WorktreeOptions,
};
use crate::shared::git::git;

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path_json(path: &std::path::Path) -> Value {
	Value::String(path.display().to_string())
}

fn env_json(env: &Env) -> Value {
	Value::Object(env.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect())
}

fn tokens_json(tokens: &Tokens) -> Value {
	Value::Object(tokens.iter().map(|(name, value)| (name.clone(), json!(value))).collect())
}

/// One recorded [`FakeHerdr`] call; the arguments are recorded as JSON.
#[derive(Debug, Clone)]
pub(crate) struct Call {
	pub method: String,
	pub args: Vec<Value>,
}

/// What a [`FakeHerdr`] holds; a test reads and sets these through [`FakeHerdr::state`].
pub(crate) struct FakeState {
	pub calls: Vec<Call>,
	pub workspace_list: Vec<WorkspaceInfo>,
	pub pane_list: Vec<PaneInfo>,
	/// Each pane's root process (`pane_root`), as a test sets it; none by default.
	pub roots: HashMap<String, u32>,
	/// What each pane shows (`screen`), as a test sets it; empty by default.
	pub screens: HashMap<String, String>,
	/// Whether Jocasta started this session's server (`connect_herdr`, 0495); a test of an existing session clears it.
	pub started_server: bool,
	/// The state a worktree's new pane reports at once, with herdr's event, as an omp that comes up before the
	/// service first looks (TST-2); `None`: none until a test sets one, so `agentUp`'s polling stays covered.
	pub omp_up_on_create: Option<AgentState>,
	/// Failed with by `create_worktree` once it made the worktree, workspace and pane, as herdr's layout failing would.
	pub create_worktree_fails: Option<SurfaceError>,
	/// Failed with by `close_workspace` after it recorded the call, as herdr unreachable would.
	pub close_workspace_fails: Option<SurfaceError>,
	/// What `notification.show` answers; `NoForegroundClient` while no client is attached.
	pub notify_result: NotificationResult,
	/// While set and not yet `true`, `snapshot` waits (a test's gate on the service's discovery).
	pub snapshot_gate: Option<tokio::sync::watch::Receiver<bool>>,
	/// Run by `close_workspace` after it recorded the call and before it fails or removes anything.
	pub on_close_workspace: Option<Arc<dyn Fn(String) -> BoxFuture<'static, ()> + Send + Sync>>,
	/// While set, `prompt` into this pane is refused as herdr refuses a pane waiting on an answer (`agent_blocked`) and
	/// is not recorded (the TypeScript test replaced the method).
	pub prompt_refused: Option<String>,
	envs: HashMap<String, Env>,
	next: u64,
	terminals: u64,
	watcher: Option<Arc<dyn Fn(WatchEvent) + Send + Sync>>,
}

/// An in-memory herdr for the service's tests: workspaces, panes and agent states, every call recorded. `create_worktree`
/// runs a real `git worktree add` so the service sees a linked worktree. Like herdr's, `ensure_workspace` lists, then
/// creates: two calls at once both create (CR-10). Share it as `Arc<FakeHerdr>`.
pub(crate) struct FakeSurface {
	state: Arc<Mutex<FakeState>>,
}

impl Default for FakeSurface {
	fn default() -> Self {
		Self::new()
	}
}

impl FakeSurface {
	pub(crate) fn new() -> Self {
		Self {
			state: Arc::new(Mutex::new(FakeState {
				calls: Vec::new(),
				workspace_list: Vec::new(),
				pane_list: Vec::new(),
				roots: HashMap::new(),
				screens: HashMap::new(),
				started_server: true,
				omp_up_on_create: None,
				create_worktree_fails: None,
				close_workspace_fails: None,
				notify_result: NotificationResult::Shown,
				snapshot_gate: None,
				on_close_workspace: None,
				prompt_refused: None,
				envs: HashMap::new(),
				next: 0,
				terminals: 0,
				watcher: None,
			})),
		}
	}

	/// The fake's state, for a test to read and set.
	pub(crate) fn state(&self) -> MutexGuard<'_, FakeState> {
		locked(&self.state)
	}

	/// The env `pane_id` was started with (its pane key among it, 0578); `None` for a pane started without one.
	pub(crate) fn env_of(&self, pane_id: &str) -> Option<Env> {
		self.state().envs.get(pane_id).cloned()
	}

	/// The calls of one method, in order: each one's arguments.
	pub(crate) fn of(&self, method: &str) -> Vec<Vec<Value>> {
		self.state().calls.iter().filter(|call| call.method == method).map(|call| call.args.clone()).collect()
	}

	/// Every call's method, in order.
	pub(crate) fn methods(&self) -> Vec<String> {
		self.state().calls.iter().map(|call| call.method.clone()).collect()
	}
	/// A bridge for tests that records the service's prompts and control and reports them here (`bridge_call`).
	pub(crate) fn recording_bridge(self: &Arc<Self>) -> crate::frontier::server::bridge::Bridge {
		let bridge = crate::frontier::server::bridge::Bridge::recording();
		let herdr = Arc::clone(self);
		bridge.record(|recording| recording.sink = Some(Arc::new(move |method, args| herdr.bridge_call(method, args))));
		bridge
	}

	/// What a recording bridge reports here (`Bridge::recording`'s sink): the call is recorded as the old typed one was,
	/// and a pane marked `prompt_refused` refuses a prompt as herdr's `agent_blocked` did.
	pub(crate) fn bridge_call(&self, method: &str, args: Vec<Value>) -> std::result::Result<(), String> {
		let pane = args.first().and_then(Value::as_str).unwrap_or_default().to_owned();
		if method == "prompt" && self.state().prompt_refused.as_deref() == Some(pane.as_str()) {
			return Err(format!("agent {pane} is blocked and requires interactive input"));
		}
		self.record(method, args);
		Ok(())
	}
	fn record(&self, method: &str, args: Vec<Value>) {
		self.state().calls.push(Call { method: method.to_owned(), args });
	}

	/// Changes a pane's agent state, as omp's integration would report it (omp is the pane's agent once it reports).
	/// `push: false` changes it without the event, as when herdr's event was lost (CR-09).
	pub(crate) fn set_state(&self, pane_id: &str, state: AgentState, session_path: Option<&str>, push: bool) {
		let (workspace_id, watcher) = {
			let mut inner = self.state();
			let Some(pane) = inner.pane_list.iter_mut().find(|p| p.pane_id == pane_id) else {
				panic!("no pane {pane_id}");
			};
			pane.agent_state = state;
			pane.agent = (state != AgentState::Unknown).then(|| "omp".to_owned());
			if let Some(path) = session_path {
				pane.session_path = Some(PathBuf::from(path));
			}
			let workspace_id = pane.workspace_id.clone();
			(workspace_id, inner.watcher.clone())
		};
		if let (true, Some(watcher)) = (push, watcher) {
			watcher(WatchEvent { pane_id: pane_id.to_owned(), workspace_id, state });
		}
	}

	/// herdr restarted and restored every pane as a shell: same pane ids, new terminals, no agent (0404).
	pub(crate) fn restore(&self) {
		let mut inner = self.state();
		let mut next = inner.next;
		for pane in &mut inner.pane_list {
			next += 1;
			pane.agent_state = AgentState::Unknown;
			pane.agent = None;
			pane.session_path = None;
			pane.terminal_id = Some(format!("term_{next}"));
			pane.label = Some("shell".to_owned());
		}
		inner.next = next;
	}

	fn id(inner: &mut FakeState, prefix: &str) -> String {
		inner.next += 1;
		format!("{prefix}_{}", inner.next)
	}

	fn add_pane(
		&self,
		(workspace_id, cwd): (&str, &str),
		command: Option<&[String]>,
		tab_id: Option<&str>,
		env: Option<&Env>,
	) -> PaneInfo {
		let mut inner = self.state();
		let pane_id = Self::id(&mut inner, "p");
		let tab_id = tab_id.map_or_else(|| Self::id(&mut inner, "t"), str::to_owned);
		inner.terminals += 1;
		let pane = PaneInfo {
			pane_id: pane_id.clone(),
			tab_id,
			workspace_id: workspace_id.to_owned(),
			cwd: Some(PathBuf::from(cwd)),
			agent_state: AgentState::Unknown,
			terminal_id: Some(format!("term_{}", inner.terminals)),
			label: command.map(|command| command.join(" ")),
			..PaneInfo::default()
		};
		if let Some(env) = env {
			inner.envs.insert(pane_id, env.clone());
		}
		inner.pane_list.push(pane.clone());
		pane
	}

	fn new_workspace(&self, label: &str, cwd: &str) -> String {
		let mut inner = self.state();
		let id = Self::id(&mut inner, "w");
		inner.workspace_list.push(WorkspaceInfo {
			id: id.clone(),
			label: label.to_owned(),
			cwd: Some(PathBuf::from(cwd)),
		});
		id
	}

	async fn create_worktree_now(&self, workspace_id: &str, opts: WorktreeOptions) -> SurfaceResult<WorktreeCreated> {
		let path = opts.path.clone();
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).expect("making the worktree's parent");
		}
		// Like herdr: checks out `branch` at `path`, creating it from `base` (else HEAD) when missing.
		let exists = git(&opts.repo, &["rev-parse", "--verify", "--quiet", &opts.branch]).ok().flatten().is_some();
		let path_arg = path.display().to_string();
		let added = if exists {
			git(&opts.repo, &["worktree", "add", &path_arg, &opts.branch])
		} else {
			let mut args = vec!["worktree", "add", "-b", &opts.branch, &path_arg];
			args.extend(opts.base.as_deref());
			git(&opts.repo, &args)
		};
		assert!(added.expect("git ran").is_some(), "git worktree add failed");
		let extra = match opts.prepare {
			Some(prepare) => prepare(path.clone()).await?,
			None => Env::new(),
		};
		let env = super::merge_env(opts.env.as_ref(), &extra);
		let mut recorded = json!({
			"repo": path_json(&opts.repo), "branch": opts.branch, "path": path_json(&path),
			"title": opts.title, "command": opts.command, "env": env_json(&env),
		});
		if let (Some(base), Value::Object(map)) = (&opts.base, &mut recorded) {
			map.insert("base".to_owned(), json!(base));
		}
		self.record("createWorktree", vec![json!(workspace_id), recorded]);
		let id = self.new_workspace(&opts.title, &path.display().to_string());
		let pane = self.add_pane((&id, &path.display().to_string()), Some(&opts.command), None, Some(&env));
		let (fails, up) = {
			let inner = self.state();
			(inner.create_worktree_fails.clone(), inner.omp_up_on_create)
		};
		if let Some(error) = fails {
			return Err(error);
		}
		if let Some(state) = up {
			self.set_state(&pane.pane_id, state, None, true);
		}
		Ok(WorktreeCreated { workspace_id: id, tab_id: pane.tab_id, pane_id: pane.pane_id, path })
	}
}

fn done() -> BoxFuture<'static, SurfaceResult<()>> {
	Box::pin(std::future::ready(Ok(())))
}

impl Surface for FakeSurface {
	fn started_server(&self) -> bool {
		self.state().started_server
	}

	fn ping(&self) -> BoxFuture<'_, SurfaceResult<Option<String>>> {
		Box::pin(async { Ok(Some("0.9.4".to_owned())) })
	}

	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a std::path::Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, SurfaceResult<String>> {
		Box::pin(async move {
			let mut args = vec![json!(label), path_json(cwd), command.map_or(Value::Null, |c| json!(c))];
			args.extend(env.map(env_json));
			self.record("ensureWorkspace", args);
			let existing = self.state().workspace_list.iter().find(|w| w.label == label).map(|w| w.id.clone());
			// herdr answers workspace.list, then workspace.create: another caller can run in between.
			tokio::task::yield_now().await;
			if let Some(existing) = existing {
				return Ok(existing);
			}
			let id = self.new_workspace(label, &cwd.display().to_string());
			self.add_pane((&id, &cwd.display().to_string()), command, None, env);
			Ok(id)
		})
	}

	fn workspaces(&self) -> BoxFuture<'_, SurfaceResult<Vec<WorkspaceInfo>>> {
		Box::pin(async { Ok(self.state().workspace_list.clone()) })
	}

	fn snapshot(&self) -> BoxFuture<'_, SurfaceResult<SurfaceSnapshot>> {
		Box::pin(async {
			let gate = self.state().snapshot_gate.clone();
			if let Some(mut gate) = gate {
				let _ = gate.wait_for(|open| *open).await;
			}
			let inner = self.state();
			Ok(SurfaceSnapshot { workspaces: inner.workspace_list.clone(), panes: inner.pane_list.clone() })
		})
	}

	/// He closes a workspace in herdr (or the service does): its panes go; a worktree checkout stays on disk.
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			self.record("closeWorkspace", vec![json!(workspace_id)]);
			let hook = self.state().on_close_workspace.clone();
			if let Some(hook) = hook {
				hook(workspace_id.to_owned()).await;
			}
			let fails = self.state().close_workspace_fails.clone();
			if let Some(error) = fails {
				return Err(error);
			}
			let mut inner = self.state();
			if let Some(at) = inner.workspace_list.iter().position(|w| w.id == workspace_id) {
				inner.workspace_list.remove(at);
			}
			inner.pane_list.retain(|pane| pane.workspace_id != workspace_id);
			Ok(())
		})
	}

	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, SurfaceResult<TabPane>> {
		Box::pin(async move {
			let mut recorded = json!({"title": tab.title, "cwd": path_json(&tab.cwd), "command": tab.command});
			if let (Some(env), Value::Object(map)) = (&tab.env, &mut recorded) {
				map.insert("env".to_owned(), env_json(env));
			}
			self.record("createTab", vec![json!(workspace_id), recorded]);
			let pane = self.add_pane(
				(workspace_id, &tab.cwd.display().to_string()),
				Some(&tab.command),
				None,
				tab.env.as_ref(),
			);
			Ok(TabPane { tab_id: pane.tab_id, pane_id: pane.pane_id })
		})
	}

	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, SurfaceResult<WorktreeCreated>> {
		Box::pin(self.create_worktree_now(workspace_id, opts))
	}

	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			self.record("moveWorkspace", vec![json!(workspace_id), json!(index)]);
			let mut inner = self.state();
			if let Some(at) = inner.workspace_list.iter().position(|w| w.id == workspace_id) {
				let moved = inner.workspace_list.remove(at);
				let index = index.min(inner.workspace_list.len());
				inner.workspace_list.insert(index, moved);
			}
			Ok(())
		})
	}

	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, SurfaceResult<TabPane>> {
		Box::pin(async move {
			let mut recorded = json!({"cwd": path_json(&opts.cwd), "command": opts.command});
			if let (Some(env), Value::Object(map)) = (&opts.env, &mut recorded) {
				map.insert("env".to_owned(), env_json(env));
			}
			self.record("respawn", vec![json!(pane_id), recorded]);
			let target = self.state().pane_list.iter().find(|p| p.pane_id == pane_id).cloned();
			let Some(target) = target else { panic!("no pane {pane_id}") };
			self.state().pane_list.retain(|pane| pane.tab_id != target.tab_id);
			let pane = self.add_pane(
				(&target.workspace_id, &opts.cwd.display().to_string()),
				Some(&opts.command),
				Some(&target.tab_id),
				opts.env.as_ref(),
			);
			Ok(TabPane { tab_id: pane.tab_id, pane_id: pane.pane_id })
		})
	}

	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, SurfaceResult<()>> {
		self.record("sendKeys", vec![json!(pane_id), json!(keys)]);
		done()
	}

	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, SurfaceResult<NotificationResult>> {
		let sound = if note.sound == super::Sound::Request { "request" } else { "none" };
		let mut recorded = json!({"title": note.title, "sound": sound});
		if let (Some(body), Value::Object(map)) = (&note.body, &mut recorded) {
			map.insert("body".to_owned(), json!(body));
		}
		self.record("notify", vec![recorded]);
		let result = self.state().notify_result;
		Box::pin(std::future::ready(Ok(result)))
	}

	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>> {
		self.record("focus", vec![json!(pane_id)]);
		done()
	}

	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<String>> {
		self.record("screen", vec![json!(pane_id)]);
		let screen = self.state().screens.get(pane_id).cloned().unwrap_or_default();
		Box::pin(std::future::ready(Ok(screen)))
	}

	fn report_pane_metadata<'a>(
		&'a self,
		pane_id: &'a str,
		meta: &'a PaneMetadata,
	) -> BoxFuture<'a, SurfaceResult<()>> {
		let mut recorded = json!({});
		if let Value::Object(map) = &mut recorded {
			if let Some(tokens) = &meta.tokens {
				map.insert("tokens".to_owned(), tokens_json(tokens));
			}
			if let Some(labels) = &meta.state_labels {
				let labels = labels.iter().map(|(state, label)| (state.as_str().to_owned(), json!(label))).collect();
				map.insert("stateLabels".to_owned(), Value::Object(labels));
			}
			if let Some(ttl) = meta.ttl_ms {
				map.insert("ttlMs".to_owned(), json!(ttl));
			}
		}
		self.record("reportPaneMetadata", vec![json!(pane_id), recorded]);
		done()
	}

	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		_ttl_ms: Option<i64>,
	) -> BoxFuture<'a, SurfaceResult<()>> {
		self.record("reportWorkspaceMetadata", vec![json!(workspace_id), tokens_json(tokens)]);
		done()
	}

	fn panes(&self) -> BoxFuture<'_, SurfaceResult<Vec<PaneInfo>>> {
		Box::pin(async { Ok(self.state().pane_list.clone()) })
	}

	fn pane_root<'a>(
		&'a self,
		pane_id: &'a str,
		_timeout_ms: Option<i64>,
	) -> BoxFuture<'a, SurfaceResult<Option<u32>>> {
		let root = self.state().roots.get(pane_id).copied();
		Box::pin(std::future::ready(Ok(root)))
	}

	fn watch(&self, _pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		self.state().watcher = Some(Arc::from(on_change));
		let state = self.state.clone();
		Box::new(move || locked(&state).watcher = None)
	}

	/// Its server stops with every pane (0434).
	fn stop_server(&self) -> BoxFuture<'_, SurfaceResult<()>> {
		self.record("stopServer", Vec::new());
		let mut inner = self.state();
		inner.workspace_list.clear();
		inner.pane_list.clear();
		Box::pin(std::future::ready(Ok(())))
	}

	fn close(&self) {
		self.state().watcher = None;
	}
}
