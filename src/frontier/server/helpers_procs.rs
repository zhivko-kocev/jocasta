//! What `e2e_tools_tests.rs` and `e2e_relaunch_tests.rs` need beyond `helpers.rs`: a server on a herdr the test made, a
//! herdr whose `respawn` and `stop_server` a test watches (the TypeScript replaced those methods on its fake), and the
//! shim tree of `test/frontier/server/sleepers.ts`'s `shimTree` with its omp started later.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;

use crate::frontier::server::helpers::{TestServer, start_test_server_with};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{
	AgentView, Env, Herdr, HerdrResult, HerdrSnapshot, Note, NotificationResult, OnChange, PaneIds, PaneInfo,
	PaneMetadata, RespawnOptions, StopWatch, TabPane, TabSpec, Tokens, WorkspaceInfo, WorktreeCreated, WorktreeOptions,
};
use crate::frontier::server::server::ServerOptions;
use crate::shared::proctree::tests::settles;
use crate::shared::proctree::{kill_tree, pid_alive};

/// `with_server` on the herdr the test made, so a test can restart the service on the same panes and watch what the
/// service's herdr is asked. The server is always stopped, and a panic of `run` raised again once it stopped.
pub(crate) async fn with_server_on<T, F, Fut>(herdr: Arc<FakeHerdr>, options: ServerOptions, run: F) -> T
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = T>,
{
	let server = start_test_server_with(herdr, options).await;
	let outcome = std::panic::AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	match outcome {
		Ok(value) => value,
		Err(panic) => std::panic::resume_unwind(panic),
	}
}

type RespawnHook = Arc<dyn Fn(&str) + Send + Sync>;
type StopHook = Arc<dyn Fn() + Send + Sync>;

/// A [`FakeHerdr`] whose `respawn` and `stop_server` first run a hook a test sets, as the TypeScript did by replacing
/// those methods. Hand it to the service as `ServerOptions::herdr`, and the `FakeHerdr` to `start_test_server_with`.
pub(crate) struct Hooked {
	inner: Arc<FakeHerdr>,
	on_respawn: Mutex<Option<RespawnHook>>,
	on_stop_server: Mutex<Option<StopHook>>,
}

impl Hooked {
	pub(crate) fn new(inner: &Arc<FakeHerdr>) -> Arc<Self> {
		Arc::new(Self { inner: Arc::clone(inner), on_respawn: Mutex::new(None), on_stop_server: Mutex::new(None) })
	}

	/// Runs `hook` with the pane's id before each `respawn`.
	pub(crate) fn on_respawn(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
		*self.on_respawn.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
	}

	/// Runs `hook` before `stop_server`.
	pub(crate) fn on_stop_server(&self, hook: impl Fn() + Send + Sync + 'static) {
		*self.on_stop_server.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
	}
}

impl Herdr for Hooked {
	fn started_server(&self) -> bool {
		self.inner.started_server()
	}
	fn ping(&self) -> BoxFuture<'_, HerdrResult<Option<String>>> {
		self.inner.ping()
	}
	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, HerdrResult<String>> {
		self.inner.ensure_workspace(label, cwd, command, env)
	}
	fn workspaces(&self) -> BoxFuture<'_, HerdrResult<Vec<WorkspaceInfo>>> {
		self.inner.workspaces()
	}
	fn snapshot(&self) -> BoxFuture<'_, HerdrResult<HerdrSnapshot>> {
		self.inner.snapshot()
	}
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.close_workspace(workspace_id)
	}
	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, HerdrResult<TabPane>> {
		self.inner.create_tab(workspace_id, tab)
	}
	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, HerdrResult<WorktreeCreated>> {
		self.inner.create_worktree(workspace_id, opts)
	}
	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.move_workspace(workspace_id, index)
	}
	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, HerdrResult<TabPane>> {
		let hook = self.on_respawn.lock().unwrap_or_else(PoisonError::into_inner).clone();
		if let Some(hook) = hook {
			hook(pane_id);
		}
		self.inner.respawn(pane_id, opts)
	}
	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.send_keys(pane_id, keys)
	}
	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, HerdrResult<NotificationResult>> {
		self.inner.notify(note)
	}
	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.focus(pane_id)
	}
	fn set_agent_view<'a>(&'a self, view: &'a AgentView) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.set_agent_view(view)
	}
	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<String>> {
		self.inner.screen(pane_id)
	}
	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.report_pane_metadata(pane_id, meta)
	}
	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.report_workspace_metadata(workspace_id, tokens, ttl_ms)
	}
	fn panes(&self) -> BoxFuture<'_, HerdrResult<Vec<PaneInfo>>> {
		self.inner.panes()
	}
	fn pane_root<'a>(&'a self, pane_id: &'a str, timeout_ms: Option<i64>) -> BoxFuture<'a, HerdrResult<Option<u32>>> {
		self.inner.pane_root(pane_id, timeout_ms)
	}
	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		self.inner.watch(pane_ids, on_change)
	}
	fn stop_server(&self) -> BoxFuture<'_, HerdrResult<()>> {
		let hook = self.on_stop_server.lock().unwrap_or_else(PoisonError::into_inner).clone();
		if let Some(hook) = hook {
			hook();
		}
		self.inner.stop_server()
	}
	fn close(&self) {
		self.inner.close();
	}
}

/// The test function the shim stand-ins run (`proctree_tests.rs`' `shim_main`), and the variable that makes it act.
const SHIM_TEST: &str = "shared::proctree::tests::shim_main";
const SHIM_VAR: &str = "JOCASTA_TEST_SHIM";

fn pid_files(dir: &Path, prefix: &str) -> Vec<u32> {
	let mut found: Vec<u32> = std::fs::read_dir(dir)
		.into_iter()
		.flatten()
		.flatten()
		.filter_map(|entry| entry.file_name().to_str()?.strip_prefix(prefix)?.parse().ok())
		.collect();
	found.sort_unstable();
	found
}

/// Ends each of `pids` with what runs under it, then waits until none runs (so a cleanup can delete the folder they
/// worked in). On its own thread and runtime: it runs from `Drop`, inside a test's runtime.
fn end_all(pids: Vec<u32>) {
	let ended = std::thread::spawn(move || {
		let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("a runtime");
		runtime.block_on(async {
			for pid in &pids {
				kill_tree(*pid).await;
			}
		});
		let until = Instant::now() + Duration::from_secs(10);
		while pids.iter().any(|pid| pid_alive(*pid)) && Instant::now() < until {
			std::thread::sleep(Duration::from_millis(50));
		}
	});
	let _ = ended.join();
}

/// A real tree like a pane's omp (0490): a shim whose child is the real omp, started only by [`ShimTree::start_omp`]
/// (a shim whose omp isn't up yet); omp starts its workers on [`ShimTree::exit`] and then exits. Dropping it ends
/// whatever of it is left.
pub(crate) struct ShimTree {
	/// The process herdr would start for the pane.
	pub shim: u32,
	child: Option<Child>,
	pids: PathBuf,
	go: PathBuf,
	exit: PathBuf,
}

impl ShimTree {
	/// The shim, working in `folder`, once it runs; its omp isn't started yet.
	pub(crate) async fn start(folder: &Path) -> Self {
		let pids = folder.join("pids");
		std::fs::create_dir_all(&pids).unwrap();
		let (go, exit) = (folder.join("go"), folder.join("exit"));
		let spec = format!("shim|{}|{}|{}", pids.display(), go.display(), exit.display());
		let mut command = Command::new(std::env::current_exe().unwrap());
		command
			.args(["--exact", SHIM_TEST, "--nocapture", "--test-threads=1"])
			.env(SHIM_VAR, spec)
			.current_dir(folder)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null());
		#[cfg(unix)]
		{
			use std::os::unix::process::CommandExt;
			command.process_group(0);
		}
		crate::shared::env::apply_to(&mut command);
		let child = command.spawn().expect("starting the shim tree");
		let tree = Self { shim: child.id(), child: Some(child), pids, go, exit };
		assert!(settles(|| pid_files(&tree.pids, "shim-").len() == 1).await, "the shim tree didn't start");
		tree
	}

	/// The real omp, the shim's child, once it runs.
	pub(crate) fn omp(&self) -> Option<u32> {
		pid_files(&self.pids, "omp-").first().copied()
	}

	/// The shim starts omp now; resolves once omp runs.
	pub(crate) async fn start_omp(&self) {
		if !self.go.exists() {
			std::fs::write(&self.go, "").unwrap();
		}
		assert!(settles(|| self.omp().is_some()).await, "the shim's omp didn't start");
	}

	/// The workers omp started on its way out, once it did.
	pub(crate) fn workers(&self) -> Vec<u32> {
		pid_files(&self.pids, "worker-")
	}

	/// omp starts two workers, waits until they run, then exits, as omp does when its pane closes.
	pub(crate) fn exit(&self) {
		if !self.exit.exists() {
			std::fs::write(&self.exit, "").unwrap();
		}
	}

	/// Ends the shim alone (`SIGKILL`, `TerminateProcess` on Windows), as herdr does when its pane closes.
	pub(crate) fn kill_shim(&mut self) {
		if let Some(child) = &mut self.child {
			let _ = child.kill();
		}
	}

	fn all(&self) -> Vec<u32> {
		[pid_files(&self.pids, "shim-"), pid_files(&self.pids, "omp-"), self.workers()].concat()
	}
}

impl Drop for ShimTree {
	fn drop(&mut self) {
		end_all(self.all());
		let _ = self.child.take();
	}
}
