//! `test/frontier/server/service-tools.test.ts`, case for case, on the real server (wave 12): the tools and the client's
//! messages, a thread closed in herdr, the panes herdr restored as shells, the boss key of an earlier run, herdr's
//! default workspace (0495), and the tool registry against the tool specs.

use std::path::Path;
use std::sync::{Arc, Mutex};

use regex::Regex;
use serde_json::{Value, json};

use crate::frontier::protocol::{ClientMessage, ServerMessage};
use crate::frontier::server::bridge::{BridgeService, Role};
use crate::frontier::server::helpers::{
	HunkDaemon, Page, README_THREAD_ENV, TEST_BOSS_KEY, TestServer, ThreadOptions, boss_pane, bridge_flags, call,
	call_as, commit_in, demo_project, guard, keyed, project_keyed, push_state, review, sh, start_test_server_with,
	start_thread, thread_omp, thread_pane, until, use_hunk_daemon, with_server,
};
use crate::frontier::server::helpers_procs::{ShimTree, with_server_on};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Env, Herdr, SplitOptions};
use crate::frontier::server::hunk::hunk_env;
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::tools::{TOOLS, ToolReply};
use crate::frontier::server::wording::HANDOFF_PROMPT;
use crate::shared::proctree::pid_alive;
use crate::shared::proctree::tests::{settles, sleeper_tree};
use crate::shared::testing::temp_jocasta;

fn error_reply(text: &str) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: Some(true) }
}

/// The text of each prompt sent to a pane, with the pane: `[pane, text]` of every `prompt` call.
fn prompts(herdr: &FakeHerdr) -> Vec<(String, String)> {
	herdr
		.of("prompt")
		.into_iter()
		.map(|args| {
			let at = |index: usize| args.get(index).and_then(Value::as_str).unwrap_or_default().to_owned();
			(at(0), at(1))
		})
		.collect()
}

fn workspace_of(herdr: &FakeHerdr, pane_id: &str) -> String {
	let state = herdr.state();
	state.pane_list.iter().find(|p| p.pane_id == pane_id).map(|p| p.workspace_id.clone()).expect("the pane")
}

#[tokio::test(flavor = "multi_thread")]
async fn history_search_today_note_and_the_clients_pane_focus_and_link_open_go_where_they_should_speak_is_no_tool_0552_0581_0603()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		// A pane Jocasta didn't start has no project (nor a pane key, 0578); the boss must name one.
		let none = call_as(&server, "history_search", &json!({"query": "why"}), Some("p_none"), None).await;
		assert_eq!(
			none.text,
			"No project here: history_search answers in the project and thread panes Jocasta started; a pane Jocasta started before panes had keys gets one when Jocasta starts it again, once it is idle."
		);
		assert_eq!(
			call(&server, "history_search", &json!({"query": "why"})).await,
			error_reply("history_search: name the project whose history to search.")
		);
		assert_eq!(call(&server, "speak", &json!({"text": "Done."})).await, error_reply("no tool speak."));
		let noted = call(&server, "note", &json!({"text": "Tidy the docs.", "project": "demo"})).await.text;
		assert!(Regex::new(r"^Filed into demo's next work \(inbox \d+\)\.$").unwrap().is_match(&noted), "{noted}");
		assert!(call(&server, "today", &json!({})).await.text.contains("demo: Tidy the docs."));

		let page = Page::open(&server, "").await;
		let snapshot = page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;
		let ServerMessage::Snapshot { today, .. } = snapshot else { unreachable!("a snapshot") };
		assert_eq!(today.next.len(), 1);
		assert_eq!(today.next[0].text, "Tidy the docs.");
		assert_eq!(today.next[0].project.as_deref(), Some("demo"));
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		page.send(&ClientMessage::PaneFocus { pane_id: boss.clone() });
		until(|| server.herdr.of("focus").len() == 1).await;
		assert_eq!(server.herdr.of("focus"), vec![vec![json!(boss)]]);
		// A link that isn't one today or the dashboard shows is never opened (C9).
		page.send(&ClientMessage::LinkOpen { url: "https://example.org/not-shown".to_owned() });
		assert_eq!(
			page.wait_for(|m| matches!(m, ServerMessage::Error { .. })).await,
			ServerMessage::Error { message: "That link isn't one today shows.".to_owned() }
		);
		// Without whiteboard nothing is shown, and the boss is told so.
		assert_eq!(
			call(&server, "show", &json!({"review": "r-1"})).await,
			error_reply("show: whiteboard isn't running in this Jocasta; nothing can be shown.")
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn show_review_opens_only_a_review_whiteboard_has_in_his_browser_otherwise_it_says_why_0393_0552() {
	let _j = temp_jocasta();
	let opened: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
	let heard = Arc::clone(&opened);
	let options = ServerOptions {
		find_review: Some(Arc::new(|id: String| {
			Box::pin(async move { Ok((id == "r-1").then(|| "Readme line".to_owned())) })
		})),
		open_review: Some(Arc::new(move |id: &str| heard.lock().unwrap().push(id.to_owned()))),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		assert_eq!(
			call(&server, "show", &json!({"review": "demo/readme-line"})).await,
			error_reply(
				"show: no whiteboard review demo/readme-line; create one with whiteboard's session_create and pass its sessionId."
			)
		);
		assert_eq!(
			call(&server, "show", &json!({"review": "r-1"})).await.text,
			"Opened review \"Readme line\" (r-1) in his browser."
		);
		assert_eq!(*opened.lock().unwrap(), vec!["r-1".to_owned()]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_closed_in_herdr_the_boss_hears_it_once_unspoken_and_its_worktree_and_branch_stay_0405() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let page = Page::open(&server, "").await;
		page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		let workspace = workspace_of(&server.herdr, &started.pane_id);
		server.herdr.close_workspace(&workspace).await.unwrap();
		server.service.refresh().await;
		server.service.refresh().await;
		let line = "[event] demo/Readme line — closed; its worktree and branch stay";
		until(|| prompts(&server.herdr).iter().any(|(p, t)| *p == boss && t == line)).await;
		assert_eq!(prompts(&server.herdr).iter().filter(|(_, t)| t == line).count(), 1);
		assert!(started.path.exists());
		assert!(sh(&project.main_path, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		assert_eq!(call(&server, "thread_status", &json!({})).await.text, "No threads.");
		drop(page);
	})
	.await;
}

/// A real process another terminal started in the worktree: it stays up until killed.
async fn other_in(path: &Path) -> crate::shared::proctree::tests::SleeperTree {
	sleeper_tree(path, false).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_pane_he_closes_in_herdr_ends_what_its_omp_left_running_the_workers_omp_starts_on_its_way_out_included_found_through_the_shims_child_a_process_something_else_started_in_its_worktree_keeps_running_0490()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		leftover_wait_ms: Some(10_000),
		leftover_again_ms: Some(200),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// The pane's shim (real processes, working in the worktree); its real omp isn't up yet.
		let mut tree = ShimTree::start(&path.join("pane")).await;
		// He started it in the worktree from another terminal: a real process that stays up until killed.
		let other = other_in(&path).await;
		server.herdr.state().roots.insert(pane_id.clone(), tree.shim);
		// The refresh tick reads the pane's shim once herdr names it, then its omp on a later tick.
		server.service.refresh().await;
		tree.start_omp().await;
		server.service.refresh().await;
		// He closes the thread's workspace in herdr: the shim goes, omp starts its workers and goes.
		tree.exit();
		tree.kill_shim();
		let workspace = workspace_of(&server.herdr, &pane_id);
		server.herdr.close_workspace(&workspace).await.unwrap();
		server.service.refresh().await;
		assert!(settles(|| tree.workers().len() == 2).await);
		assert!(settles(|| !tree.workers().into_iter().any(pid_alive)).await);
		assert!(!pid_alive(tree.omp().expect("omp ran")));
		assert!(pid_alive(other.root));
		assert!(path.exists());
		drop(tree);
		drop(other);
	})
	.await;
}

/// The thread-pane options of a respawned pane, read from the fake's record.
struct Respawned {
	cwd: String,
	command: Vec<String>,
	env: Option<Env>,
}

fn respawns_since(herdr: &FakeHerdr, before: usize) -> Vec<Respawned> {
	let state = herdr.state();
	state.calls[before..]
		.iter()
		.filter(|call| call.method == "respawn")
		.map(|call| {
			let options = &call.args[1];
			Respawned {
				cwd: options["cwd"].as_str().unwrap().to_owned(),
				command: options["command"]
					.as_array()
					.unwrap()
					.iter()
					.map(|w| w.as_str().unwrap().to_owned())
					.collect(),
				env: options["env"].as_object().map(|map| {
					map.iter().map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned())).collect()
				}),
			}
		})
		.collect()
}

fn with_vars(first: Vec<(String, String)>, then: &[(&str, &str)]) -> Vec<(String, String)> {
	first.into_iter().chain(then.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))).collect()
}

fn keyed_of(vars: &[(String, String)]) -> crate::frontier::server::helpers::Keyed {
	let pairs: Vec<(&str, &str)> = vars.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
	keyed(&pairs)
}

fn project_keyed_of(vars: &[(String, String)]) -> crate::frontier::server::helpers::Keyed {
	let pairs: Vec<(&str, &str)> = vars.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
	project_keyed(&pairs)
}

fn same(a: &str, b: &Path) -> bool {
	Path::new(a) == b
}

#[tokio::test(flavor = "multi_thread")]
async fn herdr_restored_its_panes_as_shells_jocasta_goes_first_the_boss_project_and_thread_panes_run_omp_again_with_their_configs_the_threads_unreviewed_commit_gets_its_review_and_a_second_start_leaves_all_alone_0394_0404_0424()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	let hunk: HunkDaemon = use_hunk_daemon().await;
	let options = |boss_key: Option<&str>, wait: Option<i64>| {
		let mut options =
			ServerOptions { agent_wait_ms: wait, boss_key: boss_key.map(str::to_owned), ..ServerOptions::default() };
		hunk.apply(&mut options);
		options
	};
	let range = "main...feature/readme-line";
	let root = j.root().to_path_buf();
	let (thread_pane_before, path) = with_server_on(herdr.clone(), options(None, Some(10_000)), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		// The refresh tick notes each new pane's terminal.
		server.service.refresh().await;
		// Another workspace made before Jocasta's (he opened it) pushes Jocasta off first after the restore.
		let scratch = server.herdr.ensure_workspace("scratch", &root, None, None).await.unwrap();
		server.herdr.move_workspace(&scratch, 0).await.unwrap();
		(started.pane_id, started.path)
	})
	.await;
	// The task ends while the service is off; then herdr restarts and restores every pane as a shell.
	commit_in(&path, "demo\nA line.\n", "Add a README line");
	herdr.restore();
	let before = herdr.state().calls.len();
	let project_id = project.id.clone();
	let main_path = project.main_path.clone();
	let port = hunk.hunk_port;
	let keep = (herdr.clone(), path.clone());
	with_server_on(herdr.clone(), options(None, None), |server| async move {
		let (herdr, path) = keep;
		let respawned = respawns_since(&herdr, before);
		let mut overlay: Vec<String> = vec!["omp".to_owned(), "--continue".to_owned()];
		overlay.extend(guard());
		overlay.extend(bridge_flags());
		overlay.push("--config".to_owned());
		overlay.push(overlay_file(&project_id).display().to_string());
		let thread = thread_omp(&project_id, "Readme line");
		let heads: Vec<Vec<String>> = respawned.iter().map(|r| r.command[..3].to_vec()).collect();
		let words = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
		assert_eq!(
			heads,
			vec![
				words(&["omp", "--continue", "--config"]),
				words(&["omp", "--continue", "--append-system-prompt"]),
				words(&["omp", "--continue", "--config"]),
			]
		);
		assert!(respawned[0].command[3].ends_with("boss.yml"), "{:?}", respawned[0].command);
		assert!(respawned[0].cwd.ends_with("home"), "{}", respawned[0].cwd);
		// Only the boss pane gets the run's boss key (0484); every pane its own pane key (0578).
		let boss_env = respawned[0].env.clone().expect("the boss pane's env");
		assert!(keyed(&[("JOCASTA_BOSS_KEY", TEST_BOSS_KEY)]) == boss_env, "{boss_env:?}");
		assert!(same(&respawned[1].cwd, &path), "{}", respawned[1].cwd);
		assert_eq!(respawned[1].command, thread);
		let thread_env = respawned[1].env.clone().expect("the thread pane's env");
		let expected = keyed_of(&with_vars(hunk_env(port), &README_THREAD_ENV));
		assert!(expected == thread_env, "{thread_env:?}");
		assert!(same(&respawned[2].cwd, &main_path), "{}", respawned[2].cwd);
		assert_eq!(respawned[2].command, overlay);
		let project_env = respawned[2].env.clone().expect("the project pane's env");
		assert!(project_keyed_of(&hunk_env(port)) == project_env, "{project_env:?}");
		let keys: std::collections::HashSet<Option<String>> = respawned
			.iter()
			.map(|r| {
				r.env
					.as_ref()
					.and_then(|env| env.iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key.clone()))
			})
			.collect();
		assert_eq!(keys.len(), 3);
		// The scratch shell he opened isn't Jocasta's: left alone.
		assert!(!respawned.iter().any(|r| same(&r.cwd, j.root())));
		assert_eq!(herdr.state().workspace_list[0].label, "Jocasta");

		// omp comes up idle in the thread's new pane: its unreviewed commit is reviewed.
		let fresh = thread_pane(&herdr, &path);
		assert_ne!(fresh, thread_pane_before);
		push_state(&server, &fresh, AgentState::Idle, None).await;
		server.service.refresh().await;
		let asked = review(range, Some(crate::frontier::server::helpers::BRIEF));
		until(|| prompts(&herdr).iter().any(|(p, t)| *p == fresh && *t == asked)).await;
		herdr.set_state(&fresh, AgentState::Working, None, true);
		herdr.set_state(&fresh, AgentState::Idle, None, true);
		until(|| prompts(&herdr).iter().any(|(p, t)| *p == fresh && t == HANDOFF_PROMPT)).await;
	})
	.await;

	// The service starts again with omp running everywhere (the boss pane included) and the commit reviewed: nothing is
	// redone, except that the idle boss's omp, started by the earlier run, starts again with this run's boss key
	// (0484), resuming its session.
	let boss = boss_pane(&herdr).expect("the boss pane");
	herdr.set_state(&boss, AgentState::Idle, None, true);
	let again = herdr.state().calls.len();
	with_server_on(herdr.clone(), options(Some("next-run-key"), None), |server| async move {
		server.service.refresh().await;
		let redone: Vec<(String, Vec<Value>)> = {
			let state = server.herdr.state();
			state.calls[again..]
				.iter()
				.filter(|call| ["respawn", "prompt", "splitBeside"].contains(&call.method.as_str()))
				.map(|call| (call.method.clone(), call.args.clone()))
				.collect()
		};
		assert_eq!(redone.len(), 1, "{redone:?}");
		assert_eq!(redone[0].0, "respawn");
		assert_eq!(redone[0].1[0], json!(boss));
		let env: Env = redone[0].1[1]["env"]
			.as_object()
			.expect("the respawn's env")
			.iter()
			.map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned()))
			.collect();
		assert!(keyed(&[("JOCASTA_BOSS_KEY", "next-run-key")]) == env, "{env:?}");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_boss_pane_an_earlier_run_started_is_told_it_lacks_the_key_only_when_it_sends_none_a_wrong_key_gets_the_plain_refusal_0484()
 {
	let _j = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	let hunk = use_hunk_daemon().await;
	let mut first = ServerOptions::default();
	hunk.apply(&mut first);
	let server = start_test_server_with(herdr.clone(), first).await;
	server.stop().await;
	// The earlier run's boss is still at work, so this run doesn't start it again with its key yet.
	let boss = boss_pane(&herdr).expect("the boss pane");
	herdr.set_state(&boss, AgentState::Working, None, true);
	let mut next = ServerOptions { boss_key: Some("next-run-key".to_owned()), ..ServerOptions::default() };
	hunk.apply(&mut next);
	with_server_on(herdr.clone(), next, |server: TestServer| async move {
		server.service.refresh().await;
		assert_eq!(server.herdr.of("respawn"), Vec::<Vec<Value>>::new());
		assert_eq!(
			call_as(&server, "projects", &json!({}), Some(&boss), None).await,
			error_reply(
				"projects: this boss session started before Jocasta's current run and lacks its boss key; Jocasta starts it again with the key once it is idle."
			)
		);
		assert_eq!(
			call_as(&server, "projects", &json!({}), Some(&boss), Some("wrong-key")).await,
			error_reply(
				"projects is the boss's tool; only the boss pane Jocasta started in workspace Jocasta can use it."
			)
		);
	})
	.await;
}

/// What herdr's PowerShell shows in a new pane: its prompt alone.
const PROMPT: &str = "PS C:\\Users\\example>";

/// What changes herdr's default workspace after it was made.
#[derive(Clone, Copy)]
enum Touch {
	Nothing,
	SecondPane,
	Agent,
	AnotherFolder,
	CommandRun,
	TypedNotRun,
	AnotherWorkspace,
	/// His home folder as herdr reports it through a link (macOS's `/tmp` is `/private/tmp`).
	#[cfg(unix)]
	HomeThroughLink,
}

/// What happens to herdr's session before the default workspace is made.
#[derive(Clone, Copy)]
enum Before {
	Nothing,
	ServerAlreadyRunning,
	JocastaWorkspaceRestored,
}

fn home() -> std::path::PathBuf {
	std::env::home_dir().expect("a home folder")
}

async fn touch_default(herdr: &FakeHerdr, pane_id: &str, touch: Touch, root: &Path) {
	match touch {
		Touch::Nothing => {}
		Touch::SecondPane => {
			let split = SplitOptions {
				command: vec!["pwsh".to_owned()],
				cwd: home(),
				env: None,
				label: None,
				ratio: None,
				direction: None,
			};
			herdr.split_beside(pane_id, &split).await.unwrap();
		}
		Touch::Agent => herdr.set_state(pane_id, AgentState::Idle, None, false),
		Touch::AnotherFolder => {
			let mut state = herdr.state();
			if let Some(pane) = state.pane_list.iter_mut().find(|p| p.pane_id == pane_id) {
				pane.cwd = Some(root.to_path_buf());
			}
		}
		Touch::CommandRun => {
			let shown = format!("{PROMPT} dir\n\n    Directory: C:\\Users\\example\n{PROMPT}");
			herdr.state().screens.insert(pane_id.to_owned(), shown);
		}
		Touch::TypedNotRun => {
			herdr.state().screens.insert(pane_id.to_owned(), format!("{PROMPT} git sta"));
		}
		Touch::AnotherWorkspace => {
			herdr.ensure_workspace("notes", &home(), None, None).await.unwrap();
		}
		#[cfg(unix)]
		Touch::HomeThroughLink => {
			let link = root.join("home-link");
			std::os::unix::fs::symlink(home(), &link).unwrap();
			let mut state = herdr.state();
			if let Some(pane) = state.pane_list.iter_mut().find(|p| p.pane_id == pane_id) {
				pane.cwd = Some(link);
			}
		}
	}
}

/// A session as Jocasta's service finds it: herdr's default workspace (a shell in his home folder, made when his client
/// attached to the empty session) changed by `touch`, after `before`; then the service starts and refreshes once.
async fn default_workspace_after_start(
	touch: Touch,
	before: Before,
	root: &Path,
) -> (Arc<FakeHerdr>, String, Vec<String>) {
	let herdr = Arc::new(FakeHerdr::new());
	match before {
		Before::Nothing => {}
		Before::ServerAlreadyRunning => herdr.state().started_server = false,
		Before::JocastaWorkspaceRestored => {
			herdr.ensure_workspace("Jocasta", &home(), None, None).await.unwrap();
		}
	}
	let workspace_id = herdr.ensure_workspace("example", &home(), None, None).await.unwrap();
	let pane_id = {
		let state = herdr.state();
		state.pane_list.iter().find(|p| p.workspace_id == workspace_id).map(|p| p.pane_id.clone()).expect("a pane")
	};
	herdr.state().screens.insert(pane_id.clone(), format!("{PROMPT}\n\n"));
	touch_default(&herdr, &pane_id, touch, root).await;
	let hunk = use_hunk_daemon().await;
	let mut options = ServerOptions::default();
	hunk.apply(&mut options);
	let server = start_test_server_with(herdr.clone(), options).await;
	server.service.refresh().await;
	server.stop().await;
	let closed = herdr.of("closeWorkspace").into_iter().map(|args| args[0].as_str().unwrap().to_owned()).collect();
	(herdr, workspace_id, closed)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_jocasta_created_loses_herdrs_untouched_default_workspace_once_jocastas_exists_0495() {
	let j = temp_jocasta();
	let (herdr, workspace_id, closed) = default_workspace_after_start(Touch::Nothing, Before::Nothing, j.root()).await;
	assert_eq!(closed, vec![workspace_id]);
	let labels: Vec<String> = herdr.state().workspace_list.iter().map(|w| w.label.clone()).collect();
	assert_eq!(labels, vec!["Jocasta".to_owned()]);

	// An empty new session (Jocasta started its server, no client attached yet) has nothing to close.
	let empty = Arc::new(FakeHerdr::new());
	let hunk = use_hunk_daemon().await;
	let mut options = ServerOptions::default();
	hunk.apply(&mut options);
	let server = start_test_server_with(empty.clone(), options).await;
	server.service.refresh().await;
	server.stop().await;
	assert_eq!(empty.of("closeWorkspace"), Vec::<Vec<Value>>::new());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_untouched_default_workspace_closes_when_herdr_names_his_home_through_a_link_0495() {
	let j = temp_jocasta();
	let (_, workspace_id, closed) =
		default_workspace_after_start(Touch::HomeThroughLink, Before::Nothing, j.root()).await;
	assert_eq!(closed, vec![workspace_id]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_jocasta_didnt_create_keeps_every_workspace_the_untouched_default_included_0495() {
	let j = temp_jocasta();
	// herdr's server was already running (his client started it, or an earlier run): the same untouched default stays.
	let (_, _, closed) = default_workspace_after_start(Touch::Nothing, Before::ServerAlreadyRunning, j.root()).await;
	assert_eq!(closed, Vec::<String>::new());
	// herdr restored a session that had Jocasta's workspace.
	let (_, _, closed) =
		default_workspace_after_start(Touch::Nothing, Before::JocastaWorkspaceRestored, j.root()).await;
	assert_eq!(closed, Vec::<String>::new());
	// A session with two workspaces before Jocasta's: one of them is his.
	let (_, _, closed) = default_workspace_after_start(Touch::AnotherWorkspace, Before::Nothing, j.root()).await;
	assert_eq!(closed, Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_touched_default_workspace_stays_a_second_pane_an_agent_another_folder_or_something_typed_0495() {
	let j = temp_jocasta();
	let touches = [
		("a second pane", Touch::SecondPane),
		("an agent", Touch::Agent),
		("another folder", Touch::AnotherFolder),
		("a command run", Touch::CommandRun),
		("text typed, not run", Touch::TypedNotRun),
	];
	for (what, touch) in touches {
		let (_, _, closed) = default_workspace_after_start(touch, Before::Nothing, j.root()).await;
		assert_eq!(closed, Vec::<String>::new(), "{what}");
	}
}

/// The tool names the service lists a pane of `role` (`BridgeService::tools`), sorted.
fn listed_names(server: &TestServer, role: Role) -> Vec<String> {
	let mut names: Vec<String> = BridgeService::tools(&*server.service, role)
		.iter()
		.filter_map(|tool| tool["name"].as_str().map(str::to_owned))
		.collect();
	names.sort();
	names
}

#[tokio::test(flavor = "multi_thread")]
async fn the_services_tool_registry_agrees_with_the_tool_specs_a_threads_pane_lists_exactly_the_tools_any_pane_may_call_the_bosss_the_rest_0484_0539_0542_svc_15()
 {
	let named = |boss: bool| {
		let mut names: Vec<String> =
			TOOLS.iter().filter(|tool| tool.boss == boss).map(|tool| tool.name.to_owned()).collect();
		names.sort();
		names
	};
	with_server(ServerOptions::default(), |server| async move {
		assert_eq!(listed_names(&server, Role::Thread), named(false));
		let boss = listed_names(&server, Role::Boss);
		let rest: Vec<String> = boss.into_iter().filter(|name| !named(false).contains(name)).collect();
		assert_eq!(rest, named(true));
	})
	.await;
}
