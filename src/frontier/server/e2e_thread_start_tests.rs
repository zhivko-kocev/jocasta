//! The cases of `test/frontier/server/service-thread-start.test.ts`, run on the real server (wave 12): `thread_start`'s
//! workspace, overlay, worktree and brief; a heavy thread; the worktree's folder and branch numbering; the shipped
//! prompt; the Enter after an unsubmitted brief; the events of a blocked thread and of a turn's end; a plan waiting in
//! Plan Review; a task's end through its review and `/handoff`; a task end with no trunk; the sidebar tokens and the
//! HUD file; and the settings every writer of the overlay agrees on.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::frontier::protocol::{NeedsYouKind, ServerMessage, ToastReason};
use crate::frontier::server::helpers::{
	BRIEF, Keyed, Msg, Page, TestServer, ThreadOptions, boss_pane, call, call_as, cli, commit_in, demo_project, guard,
	keyed, pane_check_flags, project_keyed, proposed_plan_file, push_state, review, session_file, sh,
	start_test_server_with, start_thread, task_dir_of, thread_omp, until, until_async, use_hunk_daemon, with_server,
};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Env};
use crate::frontier::server::hunk::{HUNK_PORT, ReviewContext, hunk_diff_args, hunk_env, review_prompt};
use crate::frontier::server::overlay::{overlay_file, overlay_text};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::tools::ToolReply;
use crate::frontier::server::wording::{HANDOFF_PROMPT, WHEN_YOU_FINISH, short_model};
use crate::frontier::server::worktrees::reviewed_file;
use crate::shared::install::{self_command, shipped_config_dir};
use crate::shared::pathkey::path_key;
use crate::shared::paths::{data_dir, lessons_dir, project_dir};
use crate::shared::projects::Project;
use crate::shared::prompt::without_leading_comment;
use crate::shared::push_hook::push_hook_env;
use crate::shared::testing::temp_jocasta;
use crate::shared::yaml::parse_value;

fn text_error(text: &str) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: Some(true) }
}

fn plain(text: &str) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: None }
}

fn prompts(server: &TestServer) -> Vec<(String, String)> {
	server
		.herdr
		.of("prompt")
		.into_iter()
		.map(|args| {
			let text = |at: usize| args.get(at).and_then(Value::as_str).unwrap_or_default().to_owned();
			(text(0), text(1))
		})
		.collect()
}

/// The prompts sent to `pane`, in order.
fn prompts_to(server: &TestServer, pane: &str) -> Vec<String> {
	prompts(server).into_iter().filter(|(to, _)| to == pane).map(|(_, text)| text).collect()
}

async fn status(server: &TestServer) -> String {
	call(server, "thread_status", &json!({})).await.text
}

/// A recorded env object as the [`Env`] a pane started with.
fn env_of(value: &Value) -> Env {
	value
		.as_object()
		.expect("an env object")
		.iter()
		.map(|(name, value)| (name.clone(), value.as_str().expect("a string").to_owned()))
		.collect()
}

/// A pane's env as the test expects it: hunk's variables for `port`, then `extra`, with the hook's entries (a project
/// pane's with its turn-end check).
fn expected_env(port: u16, extra: &[(&str, &str)], project: bool) -> Keyed {
	let hunk = hunk_env(port);
	let mut vars: Vec<(&str, &str)> = hunk.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
	vars.extend_from_slice(extra);
	if project { project_keyed(&vars) } else { keyed(&vars) }
}

fn strings(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

fn path_text(path: &Path) -> String {
	path.display().to_string()
}

fn append_line(file: &Path, entry: &Value) {
	let mut out = OpenOptions::new().append(true).open(file).unwrap();
	writeln!(out, "{entry}").unwrap();
}

fn read_json(file: &Path) -> Value {
	serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

fn options_with_wait() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

/// Runs `run` on a server started on `herdr` and always stops it, like `with_server` but over the test's own herdr.
async fn with_herdr<F, Fut>(herdr: Arc<FakeHerdr>, options: ServerOptions, run: F)
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = ()>,
{
	use futures_util::FutureExt;
	let server = start_test_server_with(herdr, options).await;
	let outcome = std::panic::AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	if let Err(panic) = outcome {
		std::panic::resume_unwind(panic);
	}
}

fn expected_overlay(project: &Project, relay_port: u16) -> Value {
	json!({
		"memory": { "backend": "off" },
		"task": { "completionProbe": false, "maxRecursionDepth": 1 },
		"async": { "widgets": false },
		"plan": { "defaultOnStartup": true },
		"compaction": {
			"methodOrder": ["handoff", "remote", "soft", "shake", "snapcompact"],
			"thresholdTokens": 300_000,
			"idleEnabled": true,
			"idleThresholdTokens": 100_000,
		},
		"contextFiles": { "extra": ["AGENTS.local.md"] },
		"skills": { "customDirectories": [path_text(&project_dir(&project.id))] },
		// The privacy settings ride in every pane (0478).
		"dev": { "autoqa": false },
		"disabledProviders": ["claude-plugins"],
		"modelRoles": { "web": "web/duckduckgo" },
		// Every session hosts in Jocasta's relay, and `/share` stays on this machine (0536, 0561).
		"collab": { "autoStart": "control", "relayUrl": format!("ws://127.0.0.1:{relay_port}") },
		"share": { "serverUrl": format!("http://127.0.0.1:{relay_port}/s") },
	})
}

/// The overlay and the lessons the first `thread_start` made, and the project workspace's and the thread's launches.
fn check_first_start(
	server: &TestServer,
	project: &Project,
	root: &Path,
	started: &crate::frontier::server::helpers::Started,
) {
	let overlay = overlay_file(&project.id);
	let parsed = parse_value(&fs::read_to_string(&overlay).unwrap()).unwrap();
	assert_eq!(parsed, expected_overlay(project, server.relay_port));
	// The project's lessons: one skill `lessons`, its index made from the shipped template (0459).
	assert_eq!(
		fs::read_to_string(lessons_dir(&project.id).join("SKILL.md")).unwrap(),
		fs::read_to_string(shipped_config_dir(None).join("lessons").join("SKILL.md")).unwrap()
	);
	let mut command = strings(&["omp", "--continue"]);
	command.extend(guard());
	command.extend(["--config".to_owned(), path_text(&overlay)]);
	command.extend(pane_check_flags());
	// The project workspace: label = project name, cwd = main checkout, the project pane running omp with Jocasta's
	// turn-end extension (0617).
	let ensured = server.herdr.of("ensureWorkspace");
	assert_eq!(ensured[1][..3], [json!("demo"), json!(path_text(&project.main_path)), json!(command)]);
	assert_eq!(expected_env(HUNK_PORT, &[], true), env_of(&ensured[1][3]));
	let project_ws =
		server.herdr.state().workspace_list.iter().find(|w| w.label == "demo").map(|w| w.id.clone()).unwrap();
	// Thread panes get Jocasta's hunk daemon port, as the review's hunk commands run in them (F-02, F-03), and their
	// own variables (0457); a new thread starts a fresh session, never `--continue` (0455).
	let made = server.herdr.of("createWorktree");
	assert_eq!(made.len(), 1);
	assert_eq!(made[0][0], json!(project_ws));
	let mut recorded = made[0][1].clone();
	let env = recorded.as_object_mut().unwrap().remove("env").unwrap();
	let prompt = task_dir_of(&project.id, "Readme line").join("prompt.md");
	let mut thread_command = strings(&["omp", "--append-system-prompt"]);
	thread_command.push(path_text(&prompt));
	thread_command.extend(guard());
	thread_command.extend(["--config".to_owned(), path_text(&overlay)]);
	// Every thread warns at $50 (0541) and appends its task's brief and done-when (0540).
	thread_command.extend(strings(&["--warn-cost", "50"]));
	assert_eq!(
		recorded,
		json!({
			"repo": path_text(&project.main_path),
			"branch": "feature/readme-line",
			"base": "main",
			"path": path_text(&root.join("demo.wt").join("readme-line")),
			"title": "Readme line",
			"command": thread_command,
		})
	);
	let thread_env = [
		("JOCASTA_THREAD", "readme-line"),
		("COMPOSE_PROJECT_NAME", "demo-readme-line"),
		("JOCASTA_PORT_OFFSET", "10"),
	];
	assert_eq!(expected_env(HUNK_PORT, &thread_env, false), env_of(&env));
	// Not heavy: its record has no `heavy` and its omp no heavy.yml (0590).
	assert!(read_json(&task_dir_of(&project.id, "Readme line").join("thread.json")).get("heavy").is_none());
	assert_eq!(started.path, root.join("demo.wt").join("readme-line"));
	assert_eq!(sh(&started.path, &["rev-parse", "--abbrev-ref", "HEAD"]), "feature/readme-line");
	// The brief ends with the lessons line (0459); the commit step is in prompt.md's When you finish (0577).
	assert!(prompts(server).contains(&(
		started.pane_id.clone(),
		"Add a README line.\n\nBefore you plan, read skill://lessons and cite each lesson your plan relies on as (lesson: <file>); a plan that goes against one says why.".to_owned()
	)));
}

/// The boss's tools on the started thread, and the refusal of every boss tool from anywhere but the boss pane.
async fn check_tools(server: &TestServer, pane: &str) {
	assert_eq!(status(server).await, "demo/Readme line [feature/readme-line] idle");
	let sent = call(server, "thread_send", &json!({ "thread": "feature/readme-line", "text": "Go on." })).await;
	assert_eq!(sent.text, "Sent to demo/Readme line.");
	let stopped = call(server, "thread_stop", &json!({ "thread": "Readme line" })).await;
	assert_eq!(stopped.text, "Stopped demo/Readme line's run.");
	assert_eq!(server.herdr.of("stop"), [[json!(pane)]]);
	assert_eq!(
		call(server, "thread_send", &json!({ "thread": "nope", "text": "x" })).await,
		text_error("no thread nope.")
	);
	// Boss tools take the boss key (SEC-1, 0484): a thread's pane, no pane, or the boss pane's id without the key or
	// with a wrong one is refused plainly, revealing nothing about the boss pane, whose omp has this run's key;
	// history_search isn't refused, and searches the thread's own project.
	let boss = boss_pane(&server.herdr);
	let askers: [(Option<&str>, Option<&str>); 5] = [
		(Some(pane), None),
		(None, None),
		(boss.as_deref(), None),
		(boss.as_deref(), Some("wrong-key")),
		(Some(pane), Some("wrong-key")),
	];
	for (from, key) in askers {
		let args = json!({ "thread": "Readme line", "how": "merge" });
		assert_eq!(
			call_as(server, "thread_finish", &args, from, key).await,
			text_error(
				"thread_finish is the boss's tool; only the boss pane Jocasta started in workspace Jocasta can use it."
			)
		);
	}
	let history = call_as(server, "history_search", &json!({ "query": "zebra stripes" }), Some(pane), None).await;
	assert_eq!(history, plain("Nothing in demo's history for that."));
	// A second thread reuses the project workspace and takes the next free number (0457).
	start_thread(server, ThreadOptions { title: Some("Second"), brief: Some("More."), ..ThreadOptions::default() })
		.await;
	assert_eq!(server.herdr.of("ensureWorkspace").len(), 2);
	let second = &server.herdr.of("createWorktree")[1][1]["env"];
	assert_eq!(second["JOCASTA_THREAD"], "second");
	assert_eq!(second["COMPOSE_PROJECT_NAME"], "demo-second");
	assert_eq!(second["JOCASTA_PORT_OFFSET"], "20");
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_start_the_project_workspace_with_its_pane_the_omp_overlay_a_worktree_workspace_on_feature_slug_from_trunk_running_a_fresh_omp_with_the_threads_env_then_the_brief_with_the_lessons_line_0388_0399_0400_0412_0414_0454_0455_0457_0459()
 {
	let j = temp_jocasta();
	let root = j.root().to_path_buf();
	let project = demo_project(&root, "demo").await;
	with_server(options_with_wait(), |server| async move {
		let started = start_thread(&server, ThreadOptions { late: true, ..ThreadOptions::default() }).await;
		assert_eq!(
			started.reply,
			"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane."
		);
		check_first_start(&server, &project, &root, &started);
		check_tools(&server, &started.pane_id).await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_heavy_thread_start_0590_is_recorded_on_its_task_and_loads_heavy_yml_after_the_overlay_at_its_start_and_when_started_again_a_heavy_that_isnt_true_or_false_is_refused_at_both_doors()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	with_server(options_with_wait(), |server| async move {
		let args = json!({ "project": "demo", "title": "Readme line", "brief": "Migrate it." });
		let mut by_mcp = args.clone();
		by_mcp["heavy"] = json!("yes");
		assert_eq!(call(&server, "thread_start", &by_mcp).await, text_error("thread_start: heavy is true or false."));
		let mut by_cli = args.clone();
		by_cli["heavy"] = json!(1);
		assert_eq!(
			cli(&server, "thread new", &by_cli).await,
			text_error("jocasta thread new: heavy is true or false.")
		);
		assert!(server.herdr.of("createWorktree").is_empty());

		let started = start_thread(&server, ThreadOptions { heavy: Some(true), ..ThreadOptions::default() }).await;
		assert_eq!(
			started.reply,
			"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane. Heavy: it runs on his slow role."
		);
		let heavy = strings(&["--config", &path_text(&shipped_config_dir(None).join("heavy.yml"))]);
		let record = read_json(&task_dir_of(&project.id, "Readme line").join("thread.json"));
		assert_eq!(record["heavy"], json!(true));
		let mut command = strings(&["omp", "--append-system-prompt"]);
		command.push(path_text(&task_dir_of(&project.id, "Readme line").join("prompt.md")));
		command.extend(guard());
		command.extend(["--config".to_owned(), path_text(&overlay_file(&project.id))]);
		command.extend(heavy.clone());
		command.extend(strings(&["--warn-cost", "50"]));
		assert_eq!(server.herdr.of("createWorktree")[0][1]["command"], json!(command));

		// herdr restores its pane as a shell: omp starts again, still heavy, heavy.yml after the overlay.
		server.service.refresh().await;
		server.herdr.restore();
		let before = server.herdr.state().calls.len();
		server.service.refresh().await;
		let respawned: Vec<Value> =
			server.herdr.state().calls.iter().skip(before).filter(|c| c.method == "respawn").map(|c| c.args[1].clone()).collect();
		let command_in = |dir: &Path| {
			respawned
				.iter()
				.find(|o| path_key(Path::new(o["cwd"].as_str().unwrap())) == path_key(dir))
				.map(|o| o["command"].clone())
		};
		let mut again = thread_omp(&project.id, "Readme line");
		again.extend(heavy.clone());
		assert_eq!(command_in(&started.path), Some(json!(again)));
		// The project pane is never heavy.
		let project_command = command_in(&project.main_path).expect("the project pane respawned");
		assert!(!project_command.as_array().unwrap().contains(&json!(heavy[1])));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_start_puts_the_worktree_beside_the_project_in_project_wt_slug_a_taken_branch_or_folder_makes_both_2_3_0445_0455()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let folder = j.root().join("demo.wt");
	with_server(options_with_wait(), |server| async move {
		let first = start_thread(&server, ThreadOptions::default()).await;
		assert_eq!(first.path, folder.join("readme-line"));
		// The first thread moved off its branch: a second one with the same title never picks up its branch.
		sh(&first.path, &["checkout", "--detach"]);
		let again = start_thread(&server, ThreadOptions { brief: Some("Again."), ..ThreadOptions::default() }).await;
		assert!(again.reply.starts_with("Started demo/Readme line on branch feature/readme-line-2"));
		let pane_cwd =
			server.herdr.state().pane_list.iter().find(|p| p.pane_id == again.pane_id).map(|p| p.cwd.clone());
		let created = &server.herdr.of("createWorktree")[1][1];
		assert_eq!(created["repo"], json!(path_text(&project.main_path)));
		assert_eq!(created["branch"], "feature/readme-line-2");
		assert_eq!(created["path"], json!(path_text(&folder.join("readme-line-2"))));
		assert_eq!(pane_cwd.expect("no thread pane"), Some(folder.join("readme-line-2")));
		assert_eq!(sh(&folder.join("readme-line-2"), &["rev-parse", "--abbrev-ref", "HEAD"]), "feature/readme-line-2");

		// The first one's folder is gone, its branch stays (a PR finish): the next takes -3 for both.
		sh(&project.main_path, &["worktree", "remove", "--force", &path_text(&first.path)]);
		start_thread(&server, ThreadOptions { brief: Some("Third."), ..ThreadOptions::default() }).await;
		let third = &server.herdr.of("createWorktree")[2][1];
		assert_eq!(third["branch"], "feature/readme-line-3");
		assert_eq!(third["path"], json!(path_text(&folder.join("readme-line-3"))));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn project_panes_append_the_shipped_ponytail_prompt_thread_panes_append_it_with_their_tasks_brief_and_done_when_in_one_file_0421_0427_0540()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let ponytail = shipped_config_dir(None).join("ponytail.md");
	// The default pane prompt is the shipped Ponytail; the test helpers' default is none, so it is named here.
	let options = ServerOptions { pane_prompt: Some(Some(ponytail.clone())), ..options_with_wait() };
	with_server(options, |server| async move {
		start_thread(&server, ThreadOptions::default()).await;
		// Project panes append Ponytail without its license comment (0577): a copy in <data> (PRM-6).
		let shipped = fs::read_to_string(&ponytail).unwrap();
		assert!(shipped.starts_with("<!--"));
		let stripped = data_dir().join("ponytail.md");
		assert_eq!(fs::read_to_string(&stripped).unwrap(), without_leading_comment(&shipped).trim_start());
		let mut command = strings(&["omp", "--continue", "--append-system-prompt"]);
		command.push(path_text(&stripped));
		command.extend(guard());
		command.extend(["--config".to_owned(), path_text(&overlay_file(&project.id))]);
		command.extend(pane_check_flags());
		let ensured = server.herdr.of("ensureWorkspace");
		assert_eq!(ensured[1][..3], [json!("demo"), json!(path_text(&project.main_path)), json!(command)]);
		assert_eq!(expected_env(HUNK_PORT, &[], true), env_of(&ensured[1][3]));
		// omp 18.18 takes one --append-system-prompt (the last), so Ponytail and the task share `prompt.md`.
		let prompt = task_dir_of(&project.id, "Readme line").join("prompt.md");
		let mut thread_command = strings(&["omp", "--append-system-prompt"]);
		thread_command.push(path_text(&prompt));
		thread_command.extend(guard());
		thread_command.extend(["--config".to_owned(), path_text(&overlay_file(&project.id))]);
		thread_command.extend(strings(&["--warn-cost", "50"]));
		assert_eq!(server.herdr.of("createWorktree")[0][1]["command"], json!(thread_command));
		// Then the task's sections, ending with how to finish (0577, PRM-7).
		let tail = WHEN_YOU_FINISH.join("\n");
		assert_eq!(
			fs::read_to_string(&prompt).unwrap(),
			format!(
				"{}\n\n# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- No commands given; the review's project check decides.\n\n{tail}\n",
				without_leading_comment(&shipped).trim()
			)
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_brief_left_unsubmitted_in_omps_input_gets_one_enter_one_omp_takes_at_once_gets_none() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { submit_wait_ms: Some(3_000), ..options_with_wait() };
	with_server(options, |server| async move {
		// The pane stays idle after the brief's paste: one Enter, only after the brief, never twice.
		let started = start_thread(&server, ThreadOptions::default()).await;
		let pane = started.pane_id;
		assert_eq!(server.herdr.of("submit"), [[json!(pane)]]);
		let order: Vec<String> =
			server.herdr.methods().into_iter().filter(|m| m == "prompt" || m == "submit").collect();
		assert_eq!(order, ["prompt", "submit"]);

		// A second thread starts working on its brief: no Enter.
		let asked = server.clone();
		let replied = tokio::spawn(async move {
			call(&asked, "thread_start", &json!({ "project": "demo", "title": "Second", "brief": "More." })).await
		});
		until(|| server.herdr.of("createWorktree").len() == 2).await;
		let second = server.herdr.state().pane_list.last().map(|p| p.pane_id.clone()).unwrap();
		server.herdr.set_state(&second, AgentState::Idle, None, true);
		until(|| prompts_to(&server, &second).iter().any(|text| text.starts_with("More."))).await;
		server.herdr.set_state(&second, AgentState::Working, None, true);
		assert!(replied.await.unwrap().text.starts_with("Started demo/Second on branch"));
		assert_eq!(server.herdr.of("submit"), [[json!(pane)]]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn events_a_blocked_thread_is_an_urgent_event_line_at_once_speech_and_a_toast_a_turns_end_waits_for_the_idle_boss_and_is_spoken_too_0405_0407_0413()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(options_with_wait(), |server| async move {
		let page = Page::open(&server, "").await;
		page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let pane = started.pane_id;
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		let file = session_file(&started.path, &[Msg::new("Which format?", 0.5).ask("CSV or JSON?")]);
		let file = path_text(&file);

		server.herdr.set_state(&pane, AgentState::Working, Some(&file), true);
		server.herdr.set_state(&pane, AgentState::Blocked, Some(&file), true);
		until(|| !prompts_to(&server, &boss).is_empty()).await;
		assert!(
			prompts(&server)
				.contains(&(boss.clone(), "[event] demo/Readme line — awaiting you: «CSV or JSON?»".to_owned()))
		);
		page.wait_for(|m| {
			matches!(m, ServerMessage::Toast { reason: ToastReason::Blocked, pane_id: Some(id), text, .. }
				if *id == pane && text.contains("CSV or JSON?"))
		})
		.await;

		// A turn that ends without commits: a normal line, held while the boss works, sent when it is idle.
		server.herdr.set_state(&boss, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		until(|| server.service.boss_state() == AgentState::Working).await;
		assert_eq!(prompts_to(&server, &boss).len(), 1);
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		until(|| prompts_to(&server, &boss).len() == 2).await;
		assert_eq!(
			prompts(&server).last().unwrap(),
			&(boss.clone(), "[event] demo/Readme line — finished its turn: «Which format?»".to_owned())
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_waiting_in_omps_plan_review_herdr_says_idle_waits_on_him_like_a_blocked_thread_herdrs_raw_done_isnt_done()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(options_with_wait(), |server| async move {
		let page = Page::open(&server, "").await;
		page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let pane = started.pane_id;
		let boss = server.herdr.state().pane_list[0].pane_id.clone();

		// herdr's `done` means idle and not yet looked at: not a finished task.
		push_state(&server, &pane, AgentState::Done, None).await;
		until(|| server.service.hud(None).is_ok_and(|hud| hud.running == 0)).await;
		assert_eq!(status(&server).await, "demo/Readme line [feature/readme-line] idle");

		let file = proposed_plan_file(&started.path);
		let file_text = path_text(&file);
		server.herdr.set_state(&pane, AgentState::Working, Some(&file_text), true);
		server.herdr.set_state(&pane, AgentState::Idle, Some(&file_text), true);
		until(|| !prompts_to(&server, &boss).is_empty()).await;
		assert_eq!(prompts_to(&server, &boss), ["[event] demo/Readme line — plan ready, awaiting you"]);
		page.wait_for(
			|m| matches!(m, ServerMessage::Toast { reason: ToastReason::Blocked, pane_id: Some(id), .. } if *id == pane),
		)
		.await;
		assert_eq!(server.service.hud(None).unwrap().blocked, 1);
		assert_eq!(status(&server).await, "demo/Readme line [feature/readme-line] plan ready\n  spend $0.25");
		assert_eq!(
			call(&server, "thread_send", &json!({ "thread": "Readme line", "text": "Go." })).await.is_error,
			Some(true)
		);
		page.wait_for(|m| {
			matches!(m, ServerMessage::Today { today }
				if today.needs_you.iter().any(|n| n.kind == NeedsYouKind::Blocked && n.pane_id.as_deref() == Some(pane.as_str())))
		})
		.await;

		// He approves: omp leaves plan mode and works; nothing waits on him any more.
		append_line(&file, &json!({ "type": "mode_change", "id": "m3", "mode": "none" }));
		server.herdr.set_state(&pane, AgentState::Working, Some(&file_text), true);
		until(|| server.service.hud(None).is_ok_and(|hud| hud.blocked == 0)).await;
		assert!(status(&server).await.starts_with("demo/Readme line [feature/readme-line] working"));
	})
	.await;
}

/// The review turn's first half: hunk beside the thread and the review prompt, nothing announced yet (UX-21).
async fn check_review_opens(server: &TestServer, pane: &str, path: &Path, boss: &str, hunk_port: u16) {
	let range = "main...feature/readme-line";
	let hunk = crate::frontier::server::helpers::hunk();
	let asked = review(range, Some(BRIEF));
	until(|| prompts(server).iter().any(|(to, text)| to == pane && *text == asked)).await;
	// 1. hunk to the right at half width on trunk...branch, watching, named "review", on Jocasta's hunk daemon;
	// 2. the review prompt.
	let mut split_env: BTreeMap<String, String> = hunk_env(hunk_port).into_iter().collect();
	split_env.extend(push_hook_env(&self_command(None).iter().map(|word| path_text(word)).collect::<Vec<_>>(), None));
	assert_eq!(
		server.herdr.of("splitBeside"),
		[vec![
			json!(pane),
			json!({
				"command": hunk_diff_args(&hunk, range),
				"cwd": path_text(path),
				"env": split_env,
				"label": "review",
				"direction": "right",
				"ratio": 0.5,
			})
		]]
	);
	let mut diff = hunk.clone();
	diff.extend(strings(&["diff", "--theme", "tokyo-night", range, "--watch"]));
	assert_eq!(hunk_diff_args(&hunk, range), diff);
	assert_eq!(
		hunk_env(1),
		[("HUNK_MCP_PORT".to_owned(), "1".to_owned()), ("HUNK_DISABLE_UPDATE_NOTICE".to_owned(), "1".to_owned())]
	);
	assert!(!prompts(server).iter().any(|(to, _)| to == boss));
	assert!(!prompts(server).iter().any(|(_, text)| text == HANDOFF_PROMPT));
	let prompt = review_prompt(&hunk, range, &ReviewContext::default());
	assert!(prompt.contains("session comment apply --repo . --stdin"));
	assert!(prompt.contains("First run the project's tests"));
	assert!(prompt.contains("`TESTS: pass — <the test command you ran>`"));
}

fn call_index(server: &TestServer, method: &str, text: Option<&str>) -> Option<usize> {
	server
		.herdr
		.state()
		.calls
		.iter()
		.position(|c| c.method == method && text.is_none_or(|text| c.args.get(1) == Some(&json!(text))))
}

/// The review turn's end: the order of what happened, the boss's line, the toast, the row and the kept commit.
async fn check_review_ends(server: &TestServer, page: &Page, pane: &str, path: &Path, boss: &str) {
	let range = "main...feature/readme-line";
	// What the review covered comes first, so the next task's line differs (N-2).
	let ready = "[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests pass, no report";
	let order = [
		call_index(server, "splitBeside", None),
		call_index(server, "prompt", Some(&review(range, Some(BRIEF)))),
		call_index(server, "prompt", Some(ready)),
		call_index(server, "prompt", Some(HANDOFF_PROMPT)),
	];
	assert!(order.iter().all(Option::is_some), "{order:?}");
	let mut sorted = order;
	sorted.sort();
	assert_eq!(sorted, order);
	assert_eq!(prompts_to(server, boss), [ready]);
	page.wait_for(|m| {
		matches!(m, ServerMessage::Toast { pane_id: Some(id), text, .. } if id == pane && text.contains("done and reviewed"))
	})
	.await;
	let toasts = page
		.received()
		.iter()
		.filter(|m| matches!(m, ServerMessage::Toast { pane_id: Some(id), .. } if id == pane))
		.count();
	assert_eq!(toasts, 1);
	// Reviewed and unfinished, it waits on him in "Awaiting you" (0456).
	until(|| {
		server.service.day().today().is_ok_and(|today| {
			today.needs_you.iter().any(|row| {
				row.kind == NeedsYouKind::Reviewed && row.text == "Readme line — done and reviewed: 2 notes, tests pass"
			})
		})
	})
	.await;
	// The reviewed commit is kept for a restart (0394).
	let reviewed = read_json(&reviewed_file());
	let head = sh(path, &["rev-parse", "HEAD"]);
	assert_eq!(reviewed.as_object().unwrap().values().cloned().collect::<Vec<_>>(), [json!(head)]);
	let last = server.herdr.of("reportPaneMetadata").pop().unwrap();
	assert_eq!(last[0], json!(pane));
	assert_eq!(last[1]["stateLabels"], json!({ "blocked": "awaiting you", "done": "review ready" }));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_end_hunk_opens_beside_the_thread_the_thread_tests_and_reviews_into_it_the_task_is_announced_once_when_its_review_is_ready_then_handoff_neither_turn_retriggers_and_a_handoff_herdr_never_reports_doesnt_swallow_the_next_task_0394_0400_0406_0425_ux_21_f_01()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let hunk = use_hunk_daemon().await;
	let mut options = options_with_wait();
	hunk.apply(&mut options);
	with_server(options, |server| async move {
		let page = Page::open(&server, "").await;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane, path) = (started.pane_id, started.path);
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		let range = "main...feature/readme-line";
		let reviews = |server: &TestServer| {
			prompts(server).iter().filter(|(_, text)| *text == review(range, Some(BRIEF))).count()
		};
		let handoffs = |server: &TestServer| prompts(server).iter().filter(|(_, text)| text == HANDOFF_PROMPT).count();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		fs::write(path.join("README.md"), "demo\nA line.\n").unwrap();
		sh(&path, &["commit", "-qam", "Add a README line"]);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		check_review_opens(&server, &pane, &path, &boss, hunk.hunk_port).await;

		// The review turn ends on its test line: 3. hunk's notes counted, 4. the boss hears it (and it is spoken and
		// toasted, once), 5. /handoff.
		let tests = Msg::new("Ran bun test, then the review: two notes.\n\n**TESTS: pass — `bun test`**", 0.1)
			.bash("bun test", true);
		let file = session_file(&path, &[tests]);
		server.herdr.set_state(&pane, AgentState::Working, Some(&path_text(&file)), true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		until(|| handoffs(&server) >= 1).await;
		check_review_ends(&server, &page, &pane, &path, &boss).await;

		// The handoff runs and ends: no second review, no second /handoff, no line.
		let before = server.herdr.state().calls.len();
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		server.service.refresh().await;
		let after: Vec<String> = server
			.herdr
			.state()
			.calls
			.iter()
			.skip(before)
			.filter(|c| c.method == "prompt" || c.method == "splitBeside")
			.map(|c| c.method.clone())
			.collect();
		assert!(after.is_empty(), "{after:?}");

		// The next task reviews in the same hunk pane (it watches the range).
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		commit_in(&path, "demo\nA line.\nAnother.\n", "Another line");
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		until(|| reviews(&server) == 2).await;
		assert_eq!(server.herdr.of("splitBeside").len(), 1);
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		until(|| handoffs(&server) == 2).await;

		// F-01: this handoff lands in the session without herdr ever reporting it as a turn; the next run is a task, and
		// its commit gets its review.
		crate::frontier::server::helpers::handoff_entry(&file);
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		// The run counts as the next task once its HEAD is noted (async git): only then may it commit.
		until_async(|| async { status(&server).await.starts_with("demo/Readme line [feature/readme-line] working") })
			.await;
		commit_in(&path, "demo\nA line.\nAnother.\nA third.\n", "A third line");
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		until(|| reviews(&server) == 3).await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn task_end_with_no_trunk_to_diff_against_no_review_pane_handoff_at_once_0400() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let hunk = use_hunk_daemon().await;
	let mut options = options_with_wait();
	hunk.apply(&mut options);
	let root = j.root().to_path_buf();
	with_server(options, |server| async move {
		// No trunk to diff against: no origin/HEAD, main, master or trunk, and the main checkout on a detached HEAD (from
		// the start: a project's trunk is kept with its places, 10 minutes).
		let main = root.join("demo");
		sh(&main, &["branch", "-m", "main", "work"]);
		sh(&main, &["checkout", "-q", "--detach"]);
		let started = start_thread(&server, ThreadOptions::default()).await;
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		fs::write(started.path.join("README.md"), "demo\nA line.\n").unwrap();
		sh(&started.path, &["commit", "-qam", "Add a README line"]);
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| prompts(&server).iter().any(|(_, text)| text == HANDOFF_PROMPT)).await;
		assert!(server.herdr.of("splitBeside").is_empty());
		// A finished task without a review count reads "done" once, not "done — done".
		assert_eq!(status(&server).await, "demo/Readme line [feature/readme-line] done");
	})
	.await;
}

/// One reported pane metadata is exactly `tokens`, `stateLabels` and a numeric `ttlMs`.
fn reported(herdr: &FakeHerdr, pane: &str, tokens: &Value, labels: &Value) -> bool {
	herdr.of("reportPaneMetadata").iter().any(|c| {
		c[0] == json!(pane)
			&& c[1].as_object().is_some_and(|meta| {
				meta.len() == 3
					&& meta["tokens"] == *tokens
					&& meta["stateLabels"] == *labels
					&& meta["ttlMs"].is_number()
			})
	})
}

#[tokio::test(flavor = "multi_thread")]
async fn sidebar_tokens_per_thread_pane_and_project_workspace_the_hud_and_the_tokens_cleared_when_the_service_stops_0402_0403_ux_04_ux_13()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	let pane_id = Arc::new(std::sync::Mutex::new(String::new()));
	// Every omp session of the day counts (the HUD matches omp's own stats), not only the threads'. The sessions folder
	// is listed every 30 s (PERF-3): this one is there from the start.
	session_file(j.root(), &[Msg::new("elsewhere", 9.0)]);
	let seen = Arc::clone(&pane_id);
	with_herdr(Arc::clone(&herdr), options_with_wait(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let pane = started.pane_id.clone();
		*seen.lock().unwrap() = pane.clone();
		let file = session_file(&started.path, &[Msg::new("Planning.", 1.25), Msg::new("Done planning.", 0.5)]);
		server.herdr.set_state(&pane, AgentState::Working, Some(&path_text(&file)), true);
		server.service.refresh().await;
		until(|| {
			server.herdr.of("reportPaneMetadata").iter().any(|c| Value::Array(c.clone()).to_string().contains("$1.75"))
		})
		.await;
		// "done" is labelled "review ready" only once a review's notes are counted (0394).
		let tokens = json!({ "model": "opus-5-5", "cost": "$1.75" });
		assert!(reported(&server.herdr, &pane, &tokens, &json!({ "blocked": "awaiting you" })));
		let project_ws =
			server.herdr.state().workspace_list.iter().find(|w| w.label == "demo").map(|w| w.id.clone()).unwrap();
		let workspace_metadata = |running: &str| {
			server.herdr.of("reportWorkspaceMetadata").contains(&vec![json!(project_ws), json!({ "running": running })])
		};
		until(|| workspace_metadata("1 at work")).await;
		// The HUD says NEEDS YOU's rows and the spend; the quota is in `today` (0554, 0581).
		let hud = |awaiting: u32| {
			server.service.hud(None).is_ok_and(|hud| (hud.awaiting, hud.spend_today) == (awaiting, Some(10.75)))
		};
		until(|| hud(0)).await;

		// A thread awaiting him counts once.
		server.herdr.set_state(&pane, AgentState::Blocked, None, true);
		until(|| hud(1)).await;
		until(|| workspace_metadata("1 awaiting you")).await;
	})
	.await;
	// Stopped: the tokens are cleared (UX-13).
	let pane = pane_id.lock().unwrap().clone();
	let last = herdr.of("reportPaneMetadata").pop().unwrap();
	assert_eq!(last, [json!(pane), json!({ "tokens": { "model": null, "cost": null } })]);
	assert_eq!(herdr.of("reportWorkspaceMetadata").pop().unwrap()[1], json!({ "running": null }));
	assert_eq!(short_model("anthropic/claude-opus-5-5"), "opus-5-5");
	assert_eq!(short_model("openai/gpt-5.5"), "gpt-5.5");
}

#[test]
fn the_compaction_and_privacy_settings_agree_wherever_they_are_written_the_project_overlay_the_bosss_boss_yml_and_a1s_arm_0399_0448_0478_svc_16()
 {
	let _j = temp_jocasta();
	let overlay = parse_value(&overlay_text("p-0123456789abcdef", None)).unwrap();
	let boss = parse_value(&fs::read_to_string(shipped_config_dir(None).join("boss.yml")).unwrap()).unwrap();
	let a1_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("evals").join("variants").join("a1.yaml");
	let a1 = parse_value(&fs::read_to_string(a1_file).unwrap()).unwrap()["settings"].clone();
	let shared = |config: &Value| {
		json!({
			"compaction": config.get("compaction"),
			"dev": config.get("dev"),
			"disabledProviders": config.get("disabledProviders"),
			"web": config.get("modelRoles").and_then(|roles| roles.get("web")),
		})
	};
	assert_eq!(shared(&boss), shared(&overlay));
	assert_eq!(shared(&a1), shared(&overlay));
}
