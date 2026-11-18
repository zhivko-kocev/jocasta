//! Test stand-ins for herdr (`test/herdr/fake-herdr.ts` and `test/frontier/server/fake-herdr.ts`, rewritten in Rust,
//! 0630): [`FakeHerdrServer`], a socket stand-in that `connect_herdr` really connects to (a named pipe on Windows, a
//! Unix socket elsewhere; in-process on tokio), and [`FakeHerdr`], an in-memory [`Herdr`] for the service's tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::{
	AgentState, AgentView, Direction, Env, Herdr, HerdrError, HerdrResult, HerdrSnapshot, LineReader, Note,
	NotificationResult, OnChange, PaneIds, PaneInfo, PaneMetadata, Placement, PluginPaneOptions, RespawnOptions,
	SplitOptions, StopWatch, TabPane, TabSpec, Tokens, WatchEvent, WorkspaceInfo, WorktreeCreated, WorktreeOptions,
	env_json, locked, path_json, tokens_json,
};
use crate::shared::git::git;

/// What the stand-in server got.
#[derive(Debug, Clone)]
pub(crate) struct FakeRequest {
	pub id: String,
	pub method: String,
	pub params: Value,
}

/// A handler's answer: herdr's `result`, or an `error {code, message}`.
pub(crate) enum Answer {
	Result(Value),
	Error { code: String, message: String },
}

impl Answer {
	pub(crate) fn result(value: Value) -> Self {
		Self::Result(value)
	}

	/// `{"type": "ok"}`.
	pub(crate) fn ok() -> Self {
		Self::Result(json!({"type": "ok"}))
	}

	pub(crate) fn error(code: &str, message: &str) -> Self {
		Self::Error { code: code.to_owned(), message: message.to_owned() }
	}
}

type Handler = Arc<dyn Fn(&Value) -> Answer + Send + Sync>;

enum Push {
	Line(String),
	/// A last line, then the connection closes.
	End(String),
}

struct Subscription {
	request: FakeRequest,
	tx: mpsc::UnboundedSender<Push>,
	closed: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
	requests: Mutex<Vec<FakeRequest>>,
	connections: AtomicUsize,
	/// Events pushed so far.
	pushes: AtomicUsize,
	handlers: Mutex<HashMap<String, Handler>>,
	drops: Mutex<HashMap<String, usize>>,
	subscriptions: Mutex<Vec<Subscription>>,
	tasks: Mutex<Vec<AbortHandle>>,
}

/// A herdr socket stand-in. Like herdr's API server it reads one request line per connection, answers it from its
/// handlers and closes the connection, except `events.subscribe`, which stays open for pushed events. Clones share one
/// server.
#[derive(Clone)]
pub(crate) struct FakeHerdrServer {
	/// What a client connects to (a pipe name on Windows).
	pub path: String,
	inner: Arc<Inner>,
}

impl FakeHerdrServer {
	/// Listens on `path` when given (a server started again on the same socket), else on a fresh one.
	pub(crate) async fn start(path: Option<&str>) -> Self {
		// A Unix socket's path fits in 104 bytes on macOS, whose temp dir alone is about 50: the name stays short.
		let name = format!("jh-{}-{}", std::process::id(), &uuid::Uuid::new_v4().simple().to_string()[..12]);
		let at = path.map_or_else(
			|| {
				if cfg!(windows) {
					format!("\\\\.\\pipe\\jocasta-herdr-test-{name}")
				} else {
					std::env::temp_dir().join(format!("{name}.sock")).display().to_string()
				}
			},
			str::to_owned,
		);
		let inner = Arc::new(Inner::default());
		let fake = Self { path: at, inner };
		fake.set_handler("ping", |_| Answer::result(json!({"type": "pong", "version": "0.9.4", "protocol": 22})));
		fake.set_handler("events.subscribe", |_| Answer::result(json!({"type": "subscription_started"})));
		fake.listen().expect("the fake herdr listens");
		fake
	}

	#[cfg(windows)]
	fn listen(&self) -> std::io::Result<()> {
		use tokio::net::windows::named_pipe::ServerOptions;
		let mut server = ServerOptions::new().create(&self.path)?;
		let (inner, path) = (self.inner.clone(), self.path.clone());
		let accept = tokio::spawn(async move {
			loop {
				if server.connect().await.is_err() {
					return;
				}
				let connected = server;
				server = match ServerOptions::new().create(&path) {
					Ok(next) => next,
					Err(_) => return,
				};
				let connection = tokio::spawn(serve(inner.clone(), connected));
				locked(&inner.tasks).push(connection.abort_handle());
			}
		});
		locked(&self.inner.tasks).push(accept.abort_handle());
		Ok(())
	}

	#[cfg(not(windows))]
	fn listen(&self) -> std::io::Result<()> {
		let _ = std::fs::remove_file(&self.path);
		let listener = tokio::net::UnixListener::bind(&self.path)?;
		let inner = self.inner.clone();
		let accept = tokio::spawn(async move {
			while let Ok((connected, _)) = listener.accept().await {
				let connection = tokio::spawn(serve(inner.clone(), connected));
				locked(&inner.tasks).push(connection.abort_handle());
			}
		});
		locked(&self.inner.tasks).push(accept.abort_handle());
		Ok(())
	}

	/// Answers `method` from now on with `handler` (instead of the default, if any).
	pub(crate) fn set_handler(&self, method: &str, handler: impl Fn(&Value) -> Answer + Send + Sync + 'static) {
		locked(&self.inner.handlers).insert(method.to_owned(), Arc::new(handler));
	}

	/// How many more requests of `method` to take and close without answering, as herdr 0.9.5 sometimes does (0517).
	pub(crate) fn drop_next(&self, method: &str, count: usize) {
		locked(&self.inner.drops).insert(method.to_owned(), count);
	}

	/// Every request taken so far, in order.
	pub(crate) fn requests(&self) -> Vec<FakeRequest> {
		locked(&self.inner.requests).clone()
	}

	pub(crate) fn methods(&self) -> Vec<String> {
		self.requests().into_iter().map(|request| request.method).collect()
	}

	pub(crate) fn last(&self, method: &str) -> Option<FakeRequest> {
		self.requests().into_iter().rev().find(|request| request.method == method)
	}

	/// How many connections were accepted.
	pub(crate) fn connections(&self) -> usize {
		self.inner.connections.load(Ordering::SeqCst)
	}

	/// How many `events.subscribe` requests were acknowledged and kept open.
	pub(crate) fn subscription_count(&self) -> usize {
		locked(&self.inner.subscriptions).len()
	}

	/// Whether every subscription's connection has closed.
	pub(crate) fn subscriptions_closed(&self) -> bool {
		locked(&self.inner.subscriptions).iter().all(|sub| sub.closed.load(Ordering::SeqCst))
	}

	/// Pushes an event to every open subscription that has an entry for its pane.
	pub(crate) fn push(&self, event: &str, data: &Value) {
		let pane_id = data.get("pane_id").and_then(Value::as_str);
		for sub in locked(&self.inner.subscriptions).iter() {
			let entries = sub.request.params.get("subscriptions").and_then(Value::as_array);
			let wanted = entries.is_some_and(|entries| {
				entries.iter().any(|e| {
					e.get("type").and_then(Value::as_str) == Some(event)
						&& e.get("pane_id").and_then(Value::as_str) == pane_id
				})
			});
			if wanted && !sub.closed.load(Ordering::SeqCst) {
				let _ = sub.tx.send(Push::Line(format!("{}\n", json!({"event": event, "data": data}))));
				self.inner.pushes.fetch_add(1, Ordering::SeqCst);
			}
		}
	}

	/// Sends `events_lost` to every open subscription and closes it, as herdr does for a slow reader.
	pub(crate) fn lose_events(&self) {
		for sub in locked(&self.inner.subscriptions).iter() {
			if !sub.closed.load(Ordering::SeqCst) {
				let line = json!({"id": sub.request.id, "error": {"code": "events_lost", "message": "lost"}});
				let _ = sub.tx.send(Push::End(format!("{line}\n")));
			}
		}
	}

	/// Closes every connection and the socket.
	pub(crate) fn stop(&self) {
		for task in locked(&self.inner.tasks).drain(..) {
			task.abort();
		}
		#[cfg(not(windows))]
		let _ = std::fs::remove_file(&self.path);
	}
}

/// Marks a subscription's connection closed when its task ends or is dropped.
struct ClosedOnDrop(Arc<AtomicBool>);

impl Drop for ClosedOnDrop {
	fn drop(&mut self) {
		self.0.store(true, Ordering::SeqCst);
	}
}

async fn serve<S>(inner: Arc<Inner>, stream: S)
where
	S: AsyncRead + AsyncWrite + Unpin + Send,
{
	inner.connections.fetch_add(1, Ordering::SeqCst);
	let (read, mut write) = tokio::io::split(stream);
	let mut lines = LineReader::new(read);
	let Some(line) = lines.next_line().await else { return };
	let Ok(value) = serde_json::from_str::<Value>(&line) else { return };
	let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
	let request = FakeRequest {
		id: text("id"),
		method: text("method"),
		params: value.get("params").cloned().unwrap_or(Value::Null),
	};
	locked(&inner.requests).push(request.clone());
	{
		let mut drops = locked(&inner.drops);
		if let Some(left) = drops.get_mut(&request.method).filter(|left| **left > 0) {
			*left -= 1;
			return;
		}
	}
	let handler = locked(&inner.handlers).get(&request.method).cloned();
	let pushed_before = inner.pushes.load(Ordering::SeqCst);
	let answer = match handler {
		Some(handler) => handler(&request.params),
		None => Answer::error("unknown_method", &format!("no {}", request.method)),
	};
	if inner.pushes.load(Ordering::SeqCst) != pushed_before {
		// An event a handler pushed reaches its client before this answer does, as herdr's does.
		tokio::time::sleep(Duration::from_millis(20)).await;
	}
	let subscribed = request.method == "events.subscribe" && matches!(answer, Answer::Result(_));
	let body = match answer {
		Answer::Result(result) => json!({"id": request.id, "result": result}),
		Answer::Error { code, message } => json!({"id": request.id, "error": {"code": code, "message": message}}),
	};
	// Registered before the answer goes out, so an event pushed once the client has its answer reaches it.
	let (tx, mut rx) = mpsc::unbounded_channel();
	let closed = Arc::new(AtomicBool::new(false));
	let _guard = ClosedOnDrop(closed.clone());
	if subscribed {
		locked(&inner.subscriptions).push(Subscription { request, tx, closed });
	}
	if write.write_all(format!("{body}\n").as_bytes()).await.is_err() {
		return;
	}
	if !subscribed {
		// The client closes once it has read the answer.
		let _ = tokio::time::timeout(Duration::from_secs(2), lines.next_line()).await;
		return;
	}
	loop {
		tokio::select! {
			push = rx.recv() => match push {
				Some(Push::Line(line)) => {
					if write.write_all(line.as_bytes()).await.is_err() {
						return;
					}
				}
				Some(Push::End(line)) => {
					let _ = write.write_all(line.as_bytes()).await;
					let _ = write.shutdown().await;
					return;
				}
				None => return,
			},
			line = lines.next_line() => if line.is_none() {
				return;
			},
		}
	}
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
	pub create_worktree_fails: Option<HerdrError>,
	/// Failed with by `close_workspace` after it recorded the call, as herdr unreachable would.
	pub close_workspace_fails: Option<HerdrError>,
	/// What `notification.show` answers; `NoForegroundClient` while no client is attached.
	pub notify_result: NotificationResult,
	/// The open popup's env, as `open_plugin_pane` got it; `None` while none is open.
	pub popup: Option<Env>,
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
pub(crate) struct FakeHerdr {
	state: Arc<Mutex<FakeState>>,
}

impl Default for FakeHerdr {
	fn default() -> Self {
		Self::new()
	}
}

impl FakeHerdr {
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
				popup: None,
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

	async fn create_worktree_now(&self, workspace_id: &str, opts: WorktreeOptions) -> HerdrResult<WorktreeCreated> {
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

fn done() -> BoxFuture<'static, HerdrResult<()>> {
	Box::pin(std::future::ready(Ok(())))
}

impl Herdr for FakeHerdr {
	fn started_server(&self) -> bool {
		self.state().started_server
	}

	fn ping(&self) -> BoxFuture<'_, HerdrResult<Option<String>>> {
		Box::pin(async { Ok(Some("0.9.4".to_owned())) })
	}

	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a std::path::Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, HerdrResult<String>> {
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

	fn workspaces(&self) -> BoxFuture<'_, HerdrResult<Vec<WorkspaceInfo>>> {
		Box::pin(async { Ok(self.state().workspace_list.clone()) })
	}

	fn snapshot(&self) -> BoxFuture<'_, HerdrResult<HerdrSnapshot>> {
		Box::pin(async {
			let gate = self.state().snapshot_gate.clone();
			if let Some(mut gate) = gate {
				let _ = gate.wait_for(|open| *open).await;
			}
			let inner = self.state();
			Ok(HerdrSnapshot { workspaces: inner.workspace_list.clone(), panes: inner.pane_list.clone() })
		})
	}

	/// He closes a workspace in herdr (or the service does): its panes go; a worktree checkout stays on disk.
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
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

	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, HerdrResult<TabPane>> {
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
	) -> BoxFuture<'a, HerdrResult<WorktreeCreated>> {
		Box::pin(self.create_worktree_now(workspace_id, opts))
	}

	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, HerdrResult<()>> {
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

	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, HerdrResult<TabPane>> {
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

	fn split_beside<'a>(&'a self, pane_id: &'a str, opts: &'a SplitOptions) -> BoxFuture<'a, HerdrResult<String>> {
		Box::pin(async move {
			let mut recorded = json!({"command": opts.command, "cwd": path_json(&opts.cwd)});
			if let Value::Object(map) = &mut recorded {
				if let Some(label) = &opts.label {
					map.insert("label".to_owned(), json!(label));
				}
				if let Some(env) = &opts.env {
					map.insert("env".to_owned(), env_json(env));
				}
				if let Some(direction) = opts.direction {
					map.insert(
						"direction".to_owned(),
						json!(if direction == Direction::Right { "right" } else { "down" }),
					);
				}
				if let Some(ratio) = opts.ratio {
					map.insert("ratio".to_owned(), json!(ratio));
				}
			}
			self.record("splitBeside", vec![json!(pane_id), recorded]);
			let target = self.state().pane_list.iter().find(|p| p.pane_id == pane_id).cloned();
			let workspace = target.as_ref().map_or("", |t| t.workspace_id.as_str());
			let tab = target.as_ref().map(|t| t.tab_id.as_str());
			let pane = self.add_pane((workspace, &opts.cwd.display().to_string()), Some(&opts.command), tab, None);
			if let Some(label) = opts.label.as_ref().filter(|label| !label.is_empty())
				&& let Some(listed) = self.state().pane_list.iter_mut().find(|p| p.pane_id == pane.pane_id)
			{
				listed.label = Some(label.clone());
			}
			Ok(pane.pane_id)
		})
	}

	fn prompt<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		if self.state().prompt_refused.as_deref() == Some(pane_id) {
			let blocked =
				HerdrError::new("agent_blocked", format!("agent {pane_id} is blocked and requires interactive input"));
			return Box::pin(std::future::ready(Err(blocked)));
		}
		self.record("prompt", vec![json!(pane_id), json!(text)]);
		done()
	}

	fn send_text<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("sendText", vec![json!(pane_id), json!(text)]);
		done()
	}

	fn stop<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("stop", vec![json!(pane_id)]);
		done()
	}

	fn submit<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("submit", vec![json!(pane_id)]);
		done()
	}

	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("sendKeys", vec![json!(pane_id), json!(keys)]);
		done()
	}

	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, HerdrResult<NotificationResult>> {
		let sound = if note.sound == super::Sound::Request { "request" } else { "none" };
		let mut recorded = json!({"title": note.title, "sound": sound});
		if let (Some(body), Value::Object(map)) = (&note.body, &mut recorded) {
			map.insert("body".to_owned(), json!(body));
		}
		self.record("notify", vec![recorded]);
		let result = self.state().notify_result;
		Box::pin(std::future::ready(Ok(result)))
	}

	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("focus", vec![json!(pane_id)]);
		done()
	}

	fn open_plugin_pane<'a>(&'a self, opts: &'a PluginPaneOptions) -> BoxFuture<'a, HerdrResult<Option<String>>> {
		Box::pin(async move {
			let mut recorded = json!({
				"pluginId": opts.plugin_id,
				"entrypoint": opts.entrypoint,
				"placement": if opts.placement == Placement::Split { "split" } else { "popup" },
			});
			if let Value::Object(map) = &mut recorded {
				if let Some(target) = &opts.target_pane_id {
					map.insert("targetPaneId".to_owned(), json!(target));
				}
				if let Some(cwd) = &opts.cwd {
					map.insert("cwd".to_owned(), path_json(cwd));
				}
				if let Some(env) = &opts.env {
					map.insert("env".to_owned(), env_json(env));
				}
			}
			self.record("openPluginPane", vec![recorded]);
			if opts.placement == Placement::Popup {
				self.state().popup = Some(opts.env.clone().unwrap_or_default());
				return Ok(None);
			}
			let target =
				self.state().pane_list.iter().find(|p| Some(&p.pane_id) == opts.target_pane_id.as_ref()).cloned();
			let Some(target) = target else { panic!("no pane {:?}", opts.target_pane_id) };
			let cwd = opts.cwd.as_ref().map(|c| c.display().to_string()).unwrap_or_default();
			let pane = self.add_pane((&target.workspace_id, &cwd), None, Some(&target.tab_id), None);
			if let Some(listed) = self.state().pane_list.iter_mut().find(|p| p.pane_id == pane.pane_id) {
				listed.label = Some(opts.entrypoint.clone());
			}
			Ok(Some(pane.pane_id))
		})
	}

	fn close_popup(&self) -> BoxFuture<'_, HerdrResult<bool>> {
		self.record("closePopup", Vec::new());
		let open = self.state().popup.take().is_some();
		Box::pin(std::future::ready(Ok(open)))
	}

	fn set_split_ratio<'a>(&'a self, tab_id: &'a str, ratio: f64) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("setSplitRatio", vec![json!(tab_id), json!(ratio)]);
		done()
	}

	fn set_agent_view<'a>(&'a self, view: &'a AgentView) -> BoxFuture<'a, HerdrResult<()>> {
		let mut recorded = json!({"source": view.source});
		if let Value::Object(map) = &mut recorded {
			if let Some(label) = &view.label {
				map.insert("label".to_owned(), json!(label));
			}
			if let Some(filter) = &view.filter {
				map.insert("filter".to_owned(), filter.clone());
			}
			if let Some(sort) = &view.sort {
				map.insert("sort".to_owned(), Value::Array(sort.clone()));
			}
		}
		self.record("setAgentView", vec![recorded]);
		done()
	}

	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<String>> {
		self.record("screen", vec![json!(pane_id)]);
		let screen = self.state().screens.get(pane_id).cloned().unwrap_or_default();
		Box::pin(std::future::ready(Ok(screen)))
	}

	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata) -> BoxFuture<'a, HerdrResult<()>> {
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
	) -> BoxFuture<'a, HerdrResult<()>> {
		self.record("reportWorkspaceMetadata", vec![json!(workspace_id), tokens_json(tokens)]);
		done()
	}

	fn panes(&self) -> BoxFuture<'_, HerdrResult<Vec<PaneInfo>>> {
		Box::pin(async { Ok(self.state().pane_list.clone()) })
	}

	fn pane_root<'a>(&'a self, pane_id: &'a str, _timeout_ms: Option<i64>) -> BoxFuture<'a, HerdrResult<Option<u32>>> {
		let root = self.state().roots.get(pane_id).copied();
		Box::pin(std::future::ready(Ok(root)))
	}

	fn watch(&self, _pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		self.state().watcher = Some(Arc::from(on_change));
		let state = self.state.clone();
		Box::new(move || locked(&state).watcher = None)
	}

	fn keep_down(&self) {
		self.record("keepDown", Vec::new());
	}

	/// Its server stops with every pane (0434).
	fn stop_server(&self) -> BoxFuture<'_, HerdrResult<()>> {
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
