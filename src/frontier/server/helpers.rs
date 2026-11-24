//! What the service's test files share: the Rust port of `test/frontier/server/helpers.ts` (test only; later waves'
//! `service-*`, `day`, `pane-check` and `push-gate` tests build on it). A test world is a temp Jocasta
//! ([`crate::shared::testing::temp_jocasta`], which the test holds for its whole length), a [`FakeHerdr`] and a
//! [`TestServer`] over them; nothing here starts his real herdr or omp.
//!
//! Where the TypeScript's function has default parameters that depend on others, Rust has the plain call and a
//! `_as`/`_with`/`_within` variant; closures that borrowed the server take it by value (a [`TestServer`] is cheap to
//! clone).

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::{FutureExt, SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::frontier::protocol::{ClientMessage, ServerMessage};
use crate::frontier::server::allow::Answer;
use crate::frontier::server::bridge::{PaneAgent, PaneEvent};
use crate::frontier::server::day::DayOptions;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Env};
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::review_prompt::{ReviewContext, review_prompt};
use crate::frontier::server::server::{JocastaServer, ServerOptions, start_server};
use crate::frontier::server::tools::{Caller, ToolReply};
use crate::frontier::server::wording::{HANDOFF_PROMPT, REVIEW_LEAD};
use crate::shared::bridge_wire::{BRIDGE_ENV, BRIDGE_EXTENSION};
use crate::shared::install::{self_command, shipped_config_dir};
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::{data_dir, project_dir};
use crate::shared::poll::wait_for;
use crate::shared::process::{RunOptions, run_program_sync};
use crate::shared::projects::{Project, add_project};
use crate::shared::push_hook::push_hook_env;
use crate::shared::schema::open_catalog;
use crate::shared::testing::sandbox;

/// The boss key test servers run with (0484); `call` sends it from the boss pane.
pub(crate) const TEST_BOSS_KEY: &str = "test-boss-key";

/// The brief `start_thread` gives.
pub(crate) const BRIEF: &str = "Add a README line.";

/// The thread variables of the project's first thread, `Readme line` (0457).
pub(crate) const README_THREAD_ENV: [(&str, &str); 3] =
	[("JOCASTA_THREAD", "readme-line"), ("COMPOSE_PROJECT_NAME", "demo-readme-line"), ("JOCASTA_PORT_OFFSET", "10")];

/// How long `until` and `Page::wait_for` wait by default.
pub(crate) const WAIT_MS: i64 = 15_000;

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn strings(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

// ---- repositories ----

/// Runs git in `dir` with a throwaway identity; panics on failure with git's exit code and stderr (the TypeScript's
/// `sh`). Git's automatic maintenance is off: since 2.47 a commit starts it detached, and its lock files coming and
/// going under `.git` broke a template's copy on CI's macOS.
pub(crate) fn sh(dir: &Path, args: &[&str]) -> String {
	let mut argv = vec!["git", "-C"];
	let shown = dir.to_string_lossy();
	argv.push(&shown);
	argv.extend_from_slice(&["-c", "user.name=Test", "-c", "user.email=test@example.com"]);
	argv.extend_from_slice(&["-c", "maintenance.auto=false", "-c", "gc.auto=0"]);
	argv.extend_from_slice(args);
	let out = run_program_sync(&argv, RunOptions { timeout_ms: Some(30_000), ..RunOptions::default() })
		.unwrap_or_else(|error| panic!("git {args:?} didn't run in {shown}: {error}"));
	assert_eq!(out.code, Some(0), "git {args:?} failed in {shown}: {}", out.stderr.trim());
	out.stdout.trim().to_owned()
}

fn make_repo(dir: &Path) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("README.md"), "demo\n").unwrap();
	sh(dir, &["add", "."]);
	// The folder name keeps two fixture repos made apart from sharing a root commit (0103's `root:<sha>` identity).
	let name = dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
	sh(dir, &["commit", "-q", "-m", &format!("init {name}")]);
	real_path(dir)
}

fn copy_tree(from: &Path, to: &Path) {
	fs::create_dir_all(to).unwrap();
	for entry in fs::read_dir(from).unwrap().flatten() {
		let target = to.join(entry.file_name());
		if entry.file_type().unwrap().is_dir() {
			copy_tree(&entry.path(), &target);
		} else {
			fs::copy(entry.path(), &target)
				.unwrap_or_else(|error| panic!("copying {}: {error}", entry.path().display()));
		}
	}
}

/// A git repo with one commit on `main`, copied from a template made once per process for `dir`'s folder name: a copy
/// costs one folder copy instead of three git spawns. Copies of one template share their root commit, which 0103 makes
/// one project, so tests about distinct repos or identity make their own.
pub(crate) fn copy_repo(dir: &Path) -> PathBuf {
	static TEMPLATES: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
	let name = dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
	let template = {
		let mut made = locked(&TEMPLATES);
		match made.iter().find(|(known, _)| *known == name) {
			Some((_, template)) => template.clone(),
			None => {
				let template = make_repo(&sandbox().join(format!("template-{}", made.len())).join(&name));
				made.push((name, template.clone()));
				template
			}
		}
	};
	if let Some(parent) = dir.parent() {
		fs::create_dir_all(parent).unwrap();
	}
	copy_tree(&template, dir);
	real_path(dir)
}

/// A registered git project named `demo`: a copy of the name's template repo (TST-4).
pub(crate) async fn demo_project(root: &Path, name: &str) -> Project {
	let repo = copy_repo(&root.join(name));
	let catalog = open_catalog().expect("the catalog");
	add_project(&catalog, &repo, None).await.expect("adding the project").project
}

/// Writes Jocasta settings (`jocasta.yaml`; JSON is YAML) in the temp config dir.
pub(crate) fn write_settings(config_dir: &Path, settings: &Value) {
	fs::create_dir_all(config_dir).unwrap();
	fs::write(config_dir.join("jocasta.yaml"), settings.to_string()).unwrap();
}

/// A commit on the thread's branch, as its task would make.
pub(crate) fn commit_in(path: &Path, text: &str, message: &str) {
	fs::write(path.join("README.md"), text).unwrap();
	sh(path, &["commit", "-qam", message]);
}

// ---- HTTP and the feed ----

/// A client for the test server: no proxy, no redirects followed (`redirect: "manual"`).
pub(crate) fn http() -> reqwest::Client {
	reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("an HTTP client")
}

/// A page's WebSocket, as `today` opens it: with the service token as a bearer.
pub(crate) struct Page {
	received: Arc<Mutex<Vec<ServerMessage>>>,
	changed: watch::Receiver<usize>,
	outgoing: mpsc::UnboundedSender<Message>,
	tasks: Vec<JoinHandle<()>>,
}

impl Page {
	/// Opens `/ws<query>` with the token as the bearer and waits for the service's snapshot, its first frame: the
	/// server subscribes to its messages before sending it, so nothing published after `open` returns is missed. Panics
	/// when refused.
	pub(crate) async fn open(server: &JocastaServer, query: &str) -> Page {
		let mut request = format!("ws://127.0.0.1:{}/ws{query}", server.port).into_client_request().unwrap();
		request.headers_mut().insert("authorization", format!("Bearer {}", server.token).parse().unwrap());
		let (socket, _) = tokio_tungstenite::connect_async(request).await.expect("WebSocket refused");
		let (mut sink, mut stream) = socket.split();
		let received = Arc::new(Mutex::new(Vec::new()));
		let (announce, changed) = watch::channel(0);
		let heard = Arc::clone(&received);
		let reader = tokio::spawn(async move {
			while let Some(Ok(frame)) = stream.next().await {
				if let Message::Text(text) = frame
					&& let Ok(message) = serde_json::from_str::<ServerMessage>(text.as_str())
				{
					let mut all = locked(&heard);
					all.push(message);
					announce.send_replace(all.len());
				}
			}
		});
		let (outgoing, mut queue) = mpsc::unbounded_channel::<Message>();
		let writer = tokio::spawn(async move {
			while let Some(message) = queue.recv().await {
				if sink.send(message).await.is_err() {
					return;
				}
			}
		});
		let page = Page { received, changed, outgoing, tasks: vec![reader, writer] };
		page.wait_for(|_| true).await;
		page
	}

	pub(crate) fn send(&self, message: &ClientMessage) {
		let _ = self.outgoing.send(Message::text(serde_json::to_string(message).unwrap()));
	}

	/// Everything received so far, in order.
	pub(crate) fn received(&self) -> Vec<ServerMessage> {
		locked(&self.received).clone()
	}

	/// The first message (already received or coming) matching `matches`; panics after [`WAIT_MS`].
	pub(crate) async fn wait_for(&self, matches: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
		self.wait_for_within(matches, WAIT_MS).await
	}

	pub(crate) async fn wait_for_within(
		&self,
		matches: impl Fn(&ServerMessage) -> bool,
		timeout_ms: i64,
	) -> ServerMessage {
		let mut changed = self.changed.clone();
		let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms.unsigned_abs());
		loop {
			if let Some(seen) = locked(&self.received).iter().find(|message| matches(message)) {
				return seen.clone();
			}
			let waited = tokio::time::timeout_at(deadline, changed.changed()).await;
			assert!(matches!(waited, Ok(Ok(()))), "timed out waiting for a server message");
		}
	}

	pub(crate) fn close(&self) {
		let _ = self.outgoing.send(Message::Close(None));
	}
}

impl Drop for Page {
	fn drop(&mut self) {
		for task in &self.tasks {
			task.abort();
		}
	}
}

// ---- the server ----

/// A test server with its fake herdr. Derefs to the [`JocastaServer`].
#[derive(Clone)]
pub(crate) struct TestServer {
	pub server: JocastaServer,
	pub herdr: Arc<FakeHerdr>,
}

impl std::ops::Deref for TestServer {
	type Target = JocastaServer;

	fn deref(&self) -> &JocastaServer {
		&self.server
	}
}

/// Every default of [`start_test_server`] under what the test gave.
pub(crate) fn with_test_defaults(mut options: ServerOptions, herdr: &Arc<FakeHerdr>) -> ServerOptions {
	let data = data_dir();
	options.herdr.get_or_insert_with(|| herdr.clone());
	options.bridge.get_or_insert_with(|| herdr.recording_bridge());
	let day = options.day.get_or_insert_with(|| DayOptions { pollers: Some(false), ..DayOptions::default() });
	// Never his disk: a nearly full one raises an urgent line that keeps the boss busy ahead of the test's own.
	day.disk_free.get_or_insert_with(|| Arc::new(|| 0.5));
	options.port.get_or_insert(0);
	options.sessions_dir.get_or_insert_with(|| data.join("omp-sessions"));
	options.agent_wait_ms.get_or_insert(0);
	options.refresh_ms.get_or_insert(60_000);
	options.pane_prompt.get_or_insert(None);
	// Never his mise: a test pane's env is its own.
	options.mise.get_or_insert(None);
	// Never his omp: the minute tick starts no weekly review (0470); a review would spawn a real `omp -p`.
	options.weekly_reviews.get_or_insert(false);
	// Never a brief of the day's real clock in the test's data folder: the tick builds none unless a test says so.
	options.auto_briefs.get_or_insert(false);
	// Never his omp's path: panes run `omp` by name, as where none is on PATH (`pane_omp_program` names the one found).
	options.omp.get_or_insert_with(|| vec!["omp".to_owned()]);
	options.ask_report.get_or_insert(false);
	options.boss_key.get_or_insert_with(|| TEST_BOSS_KEY.to_owned());
	options
		.confirm
		.get_or_insert_with(|| Arc::new(|_, _| async { Ok(Answer { allow: true, how: PopupEnd::Answered }) }.boxed()));
	options.reattach.get_or_insert_with(|| Arc::new(|| Ok(())));
	options
}

/// Starts a server for tests on a new fake herdr; see [`start_test_server_with`].
pub(crate) async fn start_test_server(options: ServerOptions) -> TestServer {
	start_test_server_with(Arc::new(FakeHerdr::new()), options).await
}

/// Starts a server for tests: `herdr` (so a test can restart the service on the same panes), pollers off (never the
/// real gh or az), a free port (never `service.port`), an empty sessions dir, no mise of his, the boss key
/// [`TEST_BOSS_KEY`], no terminal opened for the close question, every confirmation answered Allow (tests of Deny and of
/// the wait pass their own `confirm`), no steer for a missing report (its test passes `ask_report: Some(true)`), and the
/// boss's `jocasta` MCP entry as setup writes it, so no start warns that the boss has no tools (TST-14). Waits for the
/// service to start.
pub(crate) async fn start_test_server_with(herdr: Arc<FakeHerdr>, options: ServerOptions) -> TestServer {
	let data = data_dir();
	fs::create_dir_all(data.join("omp-sessions")).unwrap();

	let server = start_server(with_test_defaults(options, &herdr)).await.expect("the test server starts");
	// The service starts after the server listens; tests act on a started one.
	server.ready().await.expect("the service starts");
	TestServer { server, herdr }
}

/// Runs `run` against a test server and always stops it, so the temp dir can be removed (EBUSY on Windows); a panic in
/// `run` is raised again once it stopped.
pub(crate) async fn with_server<F, Fut>(options: ServerOptions, run: F)
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = ()>,
{
	let server = start_test_server(options).await;
	let outcome = std::panic::AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	if let Err(panic) = outcome {
		std::panic::resume_unwind(panic);
	}
}

/// Polls until `check` passes or [`WAIT_MS`] run out, which panics naming the caller's line.
#[track_caller]
pub(crate) fn until(check: impl FnMut() -> bool) -> impl Future<Output = ()> {
	until_within(check, WAIT_MS)
}

/// Polls until `check` passes or `timeout_ms` run out, which panics naming the caller's line.
#[track_caller]
pub(crate) fn until_within(mut check: impl FnMut() -> bool, timeout_ms: i64) -> impl Future<Output = ()> {
	let caller = std::panic::Location::caller();
	async move {
		let held = wait_for(|| std::future::ready(Ok(check())), timeout_ms, 10).await.unwrap();
		assert!(held, "timed out waiting for the test's condition at {caller}");
	}
}

/// Waits until the service has handled every pane state herdr pushed or a refresh caught up: it takes them on a task of
/// its own, so a step right after a push or a refresh can run before the state lands.
#[track_caller]
pub(crate) fn states_settled(server: &TestServer) -> impl Future<Output = ()> + '_ {
	until(|| server.service.states_settled())
}

/// herdr pushes the pane's `state`, and the test waits until the service has handled it ([`states_settled`]).
#[track_caller]
pub(crate) fn push_state<'a>(
	server: &'a TestServer,
	pane_id: &str,
	state: AgentState,
	session_path: Option<&str>,
) -> impl Future<Output = ()> + 'a {
	server.herdr.set_state(pane_id, state, session_path, true);
	states_settled(server)
}

/// [`until`] with a check that awaits.
pub(crate) async fn until_async<F, Fut>(mut check: F)
where
	F: FnMut() -> Fut,
	Fut: Future<Output = bool>,
{
	let held = wait_for(|| check().map(Ok), WAIT_MS, 10).await.unwrap();
	assert!(held, "timed out waiting for the test's condition");
}

// ---- omp and the threads ----

/// The guard every pane loads before its own settings (0450).
pub(crate) fn guard() -> Vec<String> {
	vec!["--config".to_owned(), shipped_config_dir(None).join("guard.yml").display().to_string()]
}

/// The task file folder (0540) of the project's open thread titled `title`, the newest when several.
pub(crate) fn task_dir_of(project_id: &str, title: &str) -> PathBuf {
	let root = project_dir(project_id).join("threads");
	let mut found: Option<(PathBuf, i64)> = None;
	for entry in fs::read_dir(&root).into_iter().flatten().flatten() {
		let Ok(text) = fs::read_to_string(entry.path().join("thread.json")) else { continue };
		let Ok(record) = serde_json::from_str::<Value>(&text) else { continue };
		let at = record["createdAt"].as_i64().unwrap_or(0);
		let open = record["closed"].as_bool() != Some(true);
		if record["title"] == title && open && found.as_ref().is_none_or(|(_, newest)| at > *newest) {
			found = Some((entry.path(), at));
		}
	}
	found.unwrap_or_else(|| panic!("no task file for {title}")).0
}

/// A thread pane's omp as started again: its task's launch prompt where the shipped prompt goes (0540), and its cost
/// cap, `threads.maxCostUsd` (0686).
pub(crate) fn thread_omp(project_id: &str, title: &str) -> Vec<String> {
	let prompt = task_dir_of(project_id, title).join("prompt.md");
	let mut argv = strings(&["omp", "--continue", "--append-system-prompt"]);
	argv.push(prompt.display().to_string());
	argv.extend(guard());
	argv.extend(bridge_flags());
	argv.extend(strings(&["--warn-cost", "50", "--config"]));
	argv.push(overlay_file(project_id).display().to_string());
	argv.extend(strings(&["--max-cost", "75"]));
	argv
}

/// The review prompt of a thread with its recorded brief (`None`: one found in herdr without one) and no report.
pub(crate) fn review(range: &str, brief: Option<&str>) -> String {
	let context = ReviewContext { brief: brief.map(str::to_owned), report: Some(None), ..ReviewContext::default() };
	review_prompt(range, &context)
}

/// The boss pane: workspace Jocasta's pane.
pub(crate) fn boss_pane(herdr: &FakeHerdr) -> Option<String> {
	let state = herdr.state();
	let jocasta = state.workspace_list.iter().find(|workspace| workspace.label == "Jocasta")?.id.clone();
	state.pane_list.iter().find(|pane| pane.workspace_id == jocasta).map(|pane| pane.pane_id.clone())
}

/// The thread pane in worktree `path` .
pub(crate) fn thread_pane(herdr: &FakeHerdr, path: &Path) -> String {
	let key = path_key(path);
	let state = herdr.state();
	let pane = state.pane_list.iter().find(|pane| {
		pane.cwd.as_deref().is_some_and(|cwd| path_key(cwd) == key) && pane.label.as_deref() != Some("review")
	});
	pane.map(|pane| pane.pane_id.clone()).unwrap_or_else(|| panic!("no thread pane in {}", path.display()))
}

/// A pane's tool call as the bridge hands it to the service (0675), from the boss pane with its key.
pub(crate) async fn call(server: &TestServer, tool: &str, args: &Value) -> ToolReply {
	let pane = boss_pane(&server.herdr);
	let boss_key = pane.is_some().then_some(TEST_BOSS_KEY);
	call_as(server, tool, args, pane.as_deref(), boss_key).await
}

/// `call` from `pane_id` (`None`: no pane) sending `boss_key` (`None`: none): the pane's own key (0578) is what names
/// it, read from the env herdr started it with.
pub(crate) async fn call_as(
	server: &TestServer,
	tool: &str,
	args: &Value,
	pane_id: Option<&str>,
	boss_key: Option<&str>,
) -> ToolReply {
	let pane_key = pane_id.and_then(|pane| server.herdr.env_of(pane)).and_then(|env| {
		env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key).filter(|key| !key.is_empty())
	});
	let caller = Caller { pane_key, boss_key: boss_key.map(str::to_owned) };
	server.service.call_tool(tool, args.as_object().unwrap_or(&Map::new()), &caller).await
}

/// `jocasta thread|worktree`'s call: a `cli` message of the bridge.
pub(crate) async fn cli(server: &TestServer, command: &str, args: &Value) -> ToolReply {
	server.service.cli_call(command, args.as_object().unwrap_or(&Map::new())).await
}

/// What [`start_thread`] starts: each unset field takes `demo`, `Readme line` and [`BRIEF`].
#[derive(Default)]
pub(crate) struct ThreadOptions<'a> {
	pub project: Option<&'a str>,
	pub title: Option<&'a str>,
	pub brief: Option<&'a str>,
	/// Its pane's omp reports idle only once herdr made the worktree, while `thread_start` already polls for it.
	pub late: bool,
	pub heavy: Option<bool>,
}

/// A started thread.
pub(crate) struct Started {
	pub pane_id: String,
	pub path: PathBuf,
	pub reply: String,
}

/// Starts a thread through the MCP endpoint. Its pane's omp reports idle as herdr makes it (TST-2), or, `late`, only
/// once herdr made the worktree, while `thread_start` already polls for it (agentUp's wait stays covered); `heavy`
/// starts it heavy (0590).
pub(crate) async fn start_thread(server: &TestServer, options: ThreadOptions<'_>) -> Started {
	let before = server.herdr.of("createWorktree").len();
	server.herdr.state().omp_up_on_create = (!options.late).then_some(AgentState::Idle);
	let mut args = json!({
		"project": options.project.unwrap_or("demo"),
		"title": options.title.unwrap_or("Readme line"),
		"brief": options.brief.unwrap_or(BRIEF),
	});
	if let Some(heavy) = options.heavy {
		args["heavy"] = json!(heavy);
	}
	let asked = server.clone();
	let replied = tokio::spawn(async move { call(&asked, "thread_start", &args).await });
	until(|| server.herdr.of("createWorktree").len() == before + 1).await;
	let pane = server.herdr.state().pane_list.last().cloned().expect("no thread pane");
	let path = pane.cwd.clone().expect("no thread pane");
	if options.late {
		server.herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
	}
	let reply = replied.await.expect("the call ran");
	states_settled(server).await;
	server.herdr.state().omp_up_on_create = None;
	Started { pane_id: pane.pane_id, path, reply: reply.text }
}

// ---- env ----

/// A Jocasta pane's whole env as a test expects it (0578, 0613): the given variables with git's pre-push hook entries,
/// computed as the product computes them from how this Jocasta starts and the environment it runs in, and a pane key
/// of whatever value. Compares equal to a pane's [`Env`].
pub(crate) struct Keyed(pub BTreeMap<String, String>);

impl PartialEq<Env> for Keyed {
	fn eq(&self, actual: &Env) -> bool {
		let mut actual: BTreeMap<String, String> = actual.iter().cloned().collect();
		actual.remove(BRIDGE_ENV);
		actual.remove("JOCASTA_PANE_KEY").is_some() && actual == self.0
	}
}

impl fmt::Debug for Keyed {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_map().entries(&self.0).entry(&"JOCASTA_PANE_KEY", &"<any>").finish()
	}
}

fn self_words() -> Vec<String> {
	self_command(None).iter().map(|word| word.display().to_string()).collect()
}

/// A Jocasta pane's whole env: `vars` with the pre-push hook's entries (see [`Keyed`]).
pub(crate) fn keyed(vars: &[(&str, &str)]) -> Keyed {
	let mut env: BTreeMap<String, String> =
		vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect();
	env.extend(push_hook_env(&self_words(), None));
	Keyed(env)
}

/// What every pane's omp starts with (0675): the bridge extension, loaded with `-e`.
pub(crate) fn bridge_flags() -> Vec<String> {
	vec!["-e".to_owned(), shipped_config_dir(None).join(BRIDGE_EXTENSION).display().to_string()]
}

/// A pane's omp pushes `event` over its bridge (0675): the recording bridge keeps `agent` as its state, and the service
/// takes the event as the bridge's reader would.
pub(crate) fn pane_reports(server: &TestServer, pane_id: &str, event: &PaneEvent, agent: PaneAgent) {
	use crate::frontier::server::bridge::{Resolved, Role};
	server.bridge.record(|recording| recording.agents.insert(pane_id.to_owned(), agent.clone()));
	let who = Resolved { pane_id: pane_id.to_owned(), role: Role::Thread, caller: Caller::default() };
	server.service.pane_event(&who, event, &agent);
}

/// A project pane's whole env: the same as any pane's, now that every pane loads the bridge extension (0675).
pub(crate) fn project_keyed(vars: &[(&str, &str)]) -> Keyed {
	keyed(vars)
}

// ---- omp's sessions ----

fn now_iso() -> String {
	chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn append(file: &Path, entries: &[Value]) {
	let mut out = OpenOptions::new().append(true).open(file).unwrap();
	for entry in entries {
		writeln!(out, "{entry}").unwrap();
	}
}

/// One assistant message of a session file.
pub(crate) struct Msg<'a> {
	pub text: &'a str,
	pub cost: f64,
	/// An `ask` call whose form's question is this (0589).
	pub ask: Option<&'a str>,
	/// A bash call with its result: its command and whether it passed.
	pub bash: Option<(&'a str, bool)>,
}

impl<'a> Msg<'a> {
	pub(crate) fn new(text: &'a str, cost: f64) -> Self {
		Self { text, cost, ask: None, bash: None }
	}

	pub(crate) fn ask(mut self, question: &'a str) -> Self {
		self.ask = Some(question);
		self
	}

	pub(crate) fn bash(mut self, command: &'a str, ok: bool) -> Self {
		self.bash = Some((command, ok));
		self
	}
}

fn message_entries(stem: &str, at: i64, index: usize, message: &Msg<'_>) -> Vec<Value> {
	let mut content = vec![json!({"type": "text", "text": message.text})];
	if let Some(question) = message.ask {
		// omp 18.22.0's `ask` call: one HTML form, its question the text before the group (0589).
		let html = format!(
			"<form><p>{}</p><label><input type=\"radio\" name=\"answer\" value=\"yes\" checked> Yes</label><label><input type=\"radio\" name=\"answer\" value=\"no\"> No</label></form>",
			question.replace('&', "&amp;").replace('<', "&lt;")
		);
		content.push(json!({"type": "toolCall", "id": "c1", "name": "ask", "arguments": {"html": html}}));
	}
	if let Some((command, _)) = message.bash {
		content.push(
			json!({"type": "toolCall", "id": format!("b{index}"), "name": "bash", "arguments": {"command": command}}),
		);
	}
	let mut entries = vec![json!({"type": "message", "id": format!("{stem}-m{index}"), "message": {
		"role": "assistant", "provider": "anthropic", "model": "claude-opus-5-5", "timestamp": at,
		"stopReason": "stop", "usage": {"cost": {"total": message.cost}}, "content": content}})];
	// The bash call's result: exit 0, or an error result with its exit code (omp `tools/bash.ts`).
	if let Some((_, ok)) = message.bash {
		let mut result = json!({"role": "toolResult", "toolCallId": format!("b{index}"), "toolName": "bash",
			"timestamp": at, "content": [{"type": "text", "text": "ok"}]});
		if !ok {
			result["isError"] = json!(true);
			result["details"] = json!({"exitCode": 1});
		}
		entries.push(json!({"type": "message", "id": format!("r{index}"), "message": result}));
	}
	entries
}

/// An omp session file in omp's shape (title, session header, assistant messages), each its own session.
pub(crate) fn session_file(cwd: &Path, messages: &[Msg<'_>]) -> PathBuf {
	static NEXT: AtomicUsize = AtomicUsize::new(0);
	let dir = data_dir().join("omp-sessions").join("--demo--");
	fs::create_dir_all(&dir).unwrap();
	// Each file its own session and message ids, as omp writes them: copies of one session count once.
	let stem = format!("{}{}", &uuid::Uuid::new_v4().simple().to_string()[..12], NEXT.fetch_add(1, Ordering::SeqCst));
	let file = dir.join(format!("2026-10-13T10-00-00-000Z_{stem}.jsonl"));
	let at = chrono::Utc::now().timestamp_millis();
	let mut lines = vec![
		json!({"type": "title", "v": 1, "title": "Readme line"}),
		json!({"type": "session", "id": format!("s-{stem}"), "cwd": cwd.display().to_string(), "timestamp": now_iso()}),
	];
	for (index, message) in messages.iter().enumerate() {
		lines.extend(message_entries(&stem, at, index, message));
	}
	fs::write(&file, "").unwrap();
	append(&file, &lines);
	file
}

fn plan_mode_entry(id: &str) -> Value {
	json!({"type": "mode_change", "id": id, "mode": "plan", "data": {"planFilePath": "local://readme-line-plan.md"}})
}

/// omp's proposal of a plan through the `propose` command, its turn silently aborted (omp 19.1.1's shape), appended to `file`.
pub(crate) fn append_proposal(file: &Path) {
	append(
		file,
		&[
			json!({"type": "message", "id": "t1", "message": {"role": "toolResult", "toolCallId": "c9", "toolName": "bash",
				"content": [{"type": "text", "text": "Plan ready for review."}],
				"details": {"toolCommands": [{"tool": "propose", "mode": "execute", "exitCode": 0, "args": {"title": "readme-line"},
					"inner": {"planFilePath": "local://readme-line-plan.md", "title": "readme-line", "planExists": true}}]},
				"isError": false}}),
			json!({"type": "message", "id": "a9", "message": {"role": "assistant", "content": [], "stopReason": "aborted"}}),
			plan_mode_entry("m2"),
		],
	);
}

/// A plan-mode session with no plan proposed yet.
pub(crate) fn plan_mode_file(cwd: &Path) -> PathBuf {
	let file = session_file(cwd, &[Msg::new("I wrote the plan.", 0.25)]);
	append(&file, &[plan_mode_entry("m1")]);
	file
}

/// A plan-mode session in omp 19.1.1's shape, its plan proposed through the `propose` command and the turn silently aborted.
pub(crate) fn proposed_plan_file(cwd: &Path) -> PathBuf {
	let file = plan_mode_file(cwd);
	append_proposal(&file);
	file
}

/// A `/handoff` entry in omp's shape, as the watcher and `handed_off` read it.
pub(crate) fn handoff_entry(file: &Path) {
	append(
		file,
		&[
			json!({"type": "compaction", "id": "h1", "method": "handoff", "summary": "Handed off.", "timestamp": now_iso()}),
		],
	);
}

/// The two P2 notes a reviewer hands in through `review_note` (0678), as they sit in the thread's
/// `review.json`.
pub(crate) fn hand_in_two_notes(task_dir: &Path) {
	let card = |severity: &str, line: usize| {
		json!({"id": format!("c{line}"), "severity": severity, "file": "README.md", "line": line, "quote": "A line.",
			"text": "Say more.", "verdict": "post", "proof": {"sha": "", "ok": true}})
	};
	let notes = json!({"head": "", "notes": [card("P2", 1), card("P2", 2)]});
	fs::write(task_dir.join("review.json"), notes.to_string()).unwrap();
}

// ---- the boss and the threads' turns ----

fn prompts(server: &TestServer) -> Vec<(String, String)> {
	let pair = |args: Vec<Value>| {
		let text = |at: usize| args.get(at).and_then(Value::as_str).unwrap_or_default().to_owned();
		(text(0), text(1))
	};
	server.herdr.of("prompt").into_iter().map(pair).collect()
}

/// Runs a thread's task to its `/handoff`: a commit, the review turn on its test line, then `/handoff` sent.
pub(crate) async fn to_handoff(server: &TestServer, pane_id: &str, path: &Path) -> PathBuf {
	let range = "main...feature/readme-line";
	server.herdr.set_state(pane_id, AgentState::Working, None, true);
	commit_in(path, "demo\nA line.\n", "Add a README line");
	server.herdr.set_state(pane_id, AgentState::Idle, None, true);
	let asked = review(range, Some(BRIEF));
	until(|| prompts(server).iter().any(|(pane, text)| pane == pane_id && *text == asked)).await;
	let tests = Msg::new("Reviewed.\n\n**TESTS: pass — bun test**", 0.1).bash("bun test", true);
	let file = session_file(path, &[tests]);
	server.herdr.set_state(pane_id, AgentState::Working, file.to_str(), true);
	server.herdr.set_state(pane_id, AgentState::Idle, None, true);
	until(|| prompts(server).iter().any(|(pane, text)| pane == pane_id && text == HANDOFF_PROMPT)).await;
	file
}

/// Restores the thread's pane as a shell, lets the service relaunch it, and brings its new omp up idle without an
/// event.
pub(crate) async fn relaunch_idle(server: &TestServer, herdr: &FakeHerdr, path: &Path, session: Option<&str>) {
	server.service.refresh().await;
	herdr.restore();
	server.service.refresh().await;
	let fresh = thread_pane(herdr, path);
	herdr.set_state(&fresh, AgentState::Idle, session, false);
	server.service.refresh().await;
	server.service.refresh().await;
}

/// The boss pane's `[event]` lines so far, one per line.
pub(crate) fn boss_lines(server: &TestServer) -> Vec<String> {
	let boss = boss_pane(&server.herdr);
	prompts(server)
		.into_iter()
		.filter(|(pane, _)| Some(pane) == boss.as_ref())
		.flat_map(|(_, text)| text.split('\n').map(str::to_owned).collect::<Vec<_>>())
		.collect()
}

/// The boss answered and is idle again: lines waiting for it go in.
pub(crate) fn boss_idle(server: &TestServer) {
	let boss = boss_pane(&server.herdr).expect("a boss pane");
	server.herdr.set_state(&boss, AgentState::Working, None, true);
	server.herdr.set_state(&boss, AgentState::Idle, None, true);
}

/// How many review prompts (0394) the fake has recorded: a review's start, now that no pane opens beside the thread.
pub(crate) fn review_asked(herdr: &FakeHerdr) -> usize {
	herdr
		.of("prompt")
		.iter()
		.filter(|args| args.get(1).and_then(Value::as_str).is_some_and(|text| text.starts_with(REVIEW_LEAD)))
		.count()
}
