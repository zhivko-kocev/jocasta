//! Thread dependencies and the running limit (0687): a start with `after` or over `threads.maxRunning` creates nothing and
//! is stored; it starts by code, from trunk as it is then, once its parents are in trunk and there is room.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::frontier::server::helpers::{
	TestServer, ThreadOptions, boss_pane, call, commit_in, demo_project, sh, start_thread, until, with_server,
};
use crate::frontier::server::pending::{After, Pending, PendingArgs};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::surface::AgentState;
use crate::shared::paths::{data_dir, project_dir};
use crate::shared::projects::Project;
use crate::shared::testing::temp_jocasta;

fn wait_options() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

fn git_config(project: &Project) {
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
}

fn pending_file(project: &Project) -> PathBuf {
	project_dir(&project.id).join("pending.json")
}

fn read_json(file: &PathBuf) -> Value {
	serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

fn prompts_to(server: &TestServer, pane: &str) -> Vec<String> {
	server
		.herdr
		.of("prompt")
		.into_iter()
		.filter(|args| args.first().and_then(Value::as_str) == Some(pane))
		.map(|args| args.get(1).and_then(Value::as_str).unwrap_or_default().to_owned())
		.collect()
}

fn journal_kinds() -> Vec<String> {
	fs::read_to_string(data_dir().join("journal.jsonl"))
		.unwrap_or_default()
		.lines()
		.filter_map(|line| serde_json::from_str::<Value>(line).ok())
		.filter_map(|line| line["kind"].as_str().map(str::to_owned))
		.collect()
}

fn start_args(title: &str, after: &[&str]) -> Value {
	json!({ "project": "demo", "title": title, "brief": "Do more.", "after": after })
}

fn pending(blocked: bool, after: Vec<After>) -> Pending {
	Pending {
		id: "q-0001".to_owned(),
		project_id: "p".to_owned(),
		title: "B".to_owned(),
		args: PendingArgs {
			project: "demo".to_owned(),
			title: "B".to_owned(),
			brief: "Do it.".to_owned(),
			done_when: Vec::new(),
			heavy: false,
			budget: None,
			door: "boss".to_owned(),
			start_at: None,
			pr: None,
		},
		after,
		queued_at_ms: 0,
		blocked,
	}
}

fn after(name: &str, met: bool) -> After {
	After { thread: format!("t-{name}"), name: name.to_owned(), met, pr: None }
}

#[test]
fn a_waiting_starts_line_names_what_it_waits_for_or_that_it_only_waits_for_room_or_is_blocked() {
	assert_eq!(pending(false, vec![after("A", false), after("C", true)]).line(), "waiting: B · after A");
	assert_eq!(pending(false, vec![after("A", false), after("C", false)]).line(), "waiting: B · after A, C");
	assert_eq!(pending(false, vec![after("A", true)]).line(), "queued: B");
	assert_eq!(pending(false, Vec::new()).line(), "queued: B");
	assert_eq!(pending(true, vec![after("A", false)]).line(), "blocked: B");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_after_reference_is_the_tools_error_and_nothing_is_stored() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	with_server(wait_options(), |server| async move {
		let reply = call(&server, "thread_start", &start_args("Follow up", &["nope"])).await;
		assert_eq!(reply.is_error, Some(true));
		assert_eq!(reply.text, "thread_start: after: no thread nope.");
		assert!(!pending_file(&project).exists());
		assert!(server.herdr.of("createWorktree").is_empty());
		let bad =
			call(&server, "thread_start", &json!({ "project": "demo", "title": "T", "brief": "b", "after": "A" }))
				.await;
		assert_eq!(bad.text, "thread_start: after is a list of threads.");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_after_a_thread_waits_for_its_merge_then_starts_from_trunk_with_no_prompt_to_the_boss() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	with_server(wait_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let reply = call(&server, "thread_start", &start_args("Follow up", &["Readme line"])).await;
		assert_eq!(reply.is_error, None, "{}", reply.text);
		assert_eq!(reply.text, "Waiting: Follow up starts after Readme line.");
		assert_eq!(server.herdr.of("createWorktree").len(), 1, "nothing is created while it waits");
		let stored = read_json(&pending_file(&project));
		assert_eq!(stored[0]["title"], "Follow up");
		assert_eq!(stored[0]["blocked"], false);
		assert_eq!(stored[0]["after"][0]["met"], false);
		// Both lists say so.
		let waiting = "waiting: Follow up · after Readme line";
		assert!(call(&server, "thread_status", &json!({})).await.text.ends_with(waiting));
		assert_eq!(call(&server, "thread_status", &json!({ "thread": "Follow up" })).await.text, waiting);
		assert!(call(&server, "today", &json!({})).await.text.contains(waiting));
		// A tick with its parent unmerged starts nothing.
		let ctx = server.service.context().clone();
		ctx.pending.tick().await;
		assert_eq!(server.herdr.of("createWorktree").len(), 1);

		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		let merged = call(&server, "thread_finish", &json!({ "thread": "Readme line", "how": "merge" })).await;
		assert_eq!(merged.is_error, None, "{}", merged.text);
		until(|| ctx.threads.find("Follow up").is_ok()).await;
		let follow = ctx.threads.find("Follow up").unwrap();
		let path = follow.with(|t| t.path.clone());
		// It branched from trunk as it is then: the merged line is in its checkout.
		assert!(fs::read_to_string(path.join("README.md")).unwrap().replace("\r\n", "\n").contains("A line."));
		until(|| !pending_file(&project).exists()).await;
		assert_eq!(server.herdr.of("createWorktree").len(), 2);
		let boss = boss_pane(&server.herdr).expect("the boss pane");
		assert!(
			prompts_to(&server, &boss).iter().all(|text| !text.contains("Follow up")),
			"the boss took no turn for it"
		);
		let kinds = journal_kinds();
		assert!(kinds.contains(&"queued".to_owned()) && kinds.contains(&"started_from_queue".to_owned()));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fifth_start_with_four_running_is_queued_and_starts_when_one_thread_sleeps() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	with_server(wait_options(), |server| async move {
		let mut first = None;
		for title in ["One", "Two", "Three", "Four"] {
			let started = start_thread(&server, ThreadOptions { title: Some(title), ..ThreadOptions::default() }).await;
			first.get_or_insert(started.pane_id);
		}
		assert_eq!(server.herdr.of("createWorktree").len(), 4);
		let reply = call(&server, "thread_start", &start_args("Five", &[])).await;
		assert_eq!(reply.is_error, None, "{}", reply.text);
		assert_eq!(reply.text, "Queued: Five (4 running); it starts when one finishes or sleeps.");
		assert_eq!(server.herdr.of("createWorktree").len(), 4);
		assert_eq!(read_json(&pending_file(&project))[0]["after"], json!([]));
		assert!(call(&server, "thread_status", &json!({})).await.text.ends_with("queued: Five"));
		let ctx = server.service.context().clone();
		ctx.pending.tick().await;
		assert_eq!(server.herdr.of("createWorktree").len(), 4, "no room yet");

		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		ctx.sleep.sleep(&first.unwrap()).await.unwrap();
		until(|| ctx.threads.find("Five").is_ok()).await;
		assert_eq!(server.herdr.of("createWorktree").len(), 5);
		until(|| !pending_file(&project).exists()).await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_running_limit_is_the_setting_and_zero_is_no_limit() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	std::fs::write(crate::shared::paths::config_dir().join("jocasta.yaml"), "threads:\n  maxRunning: 1\n").unwrap();
	with_server(wait_options(), |server| async move {
		start_thread(&server, ThreadOptions::default()).await;
		let queued = call(&server, "thread_start", &start_args("Second", &[])).await;
		assert_eq!(queued.text, "Queued: Second (1 running); it starts when one finishes or sleeps.");
		std::fs::write(crate::shared::paths::config_dir().join("jocasta.yaml"), "threads:\n  maxRunning: 0\n").unwrap();
		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		let ctx = server.service.context().clone();
		ctx.pending.tick().await;
		assert!(ctx.threads.find("Second").is_ok(), "no limit: it starts");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_that_is_stopped_blocks_what_waits_for_it_and_he_hears_it_once() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	with_server(wait_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		call(&server, "thread_start", &start_args("Follow up", &["Readme line"])).await;
		let again = call(&server, "thread_start", &start_args("Last", &["Follow up"])).await;
		assert_eq!(again.text, "Waiting: Last starts after Follow up.", "a waiting start is a valid reference");
		let stopped = call(&server, "thread_stop", &json!({ "thread": "Readme line" })).await;
		assert_eq!(stopped.is_error, None, "{}", stopped.text);
		let stored = read_json(&pending_file(&project));
		assert_eq!(stored[0]["blocked"], true);
		assert_eq!(stored[1]["blocked"], true, "what waits for a blocked start is blocked in turn");
		let status = call(&server, "thread_status", &json!({})).await.text;
		assert!(status.ends_with("blocked: Follow up\nblocked: Last"), "{status}");
		let boss = boss_pane(&server.herdr).expect("the boss pane");
		let said = prompts_to(&server, &boss);
		let told = |wanted: &str| said.iter().filter(|text| text.contains(wanted)).count();
		assert_eq!(told("Follow up waits on Readme line, which was stopped"), 1, "{said:?}");
		assert_eq!(told("Last waits on Follow up, which was stopped"), 1, "{said:?}");

		// Its parent merging later starts nothing: a blocked start never goes by itself.
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let ctx = server.service.context().clone();
		ctx.pending.tick().await;
		assert_eq!(server.herdr.of("createWorktree").len(), 1);

		// Stopping a waiting start removes it.
		let removed = call(&server, "thread_stop", &json!({ "thread": "Last" })).await;
		assert_eq!(removed.text, "Removed the waiting start Last.");
		assert_eq!(read_json(&pending_file(&project)).as_array().unwrap().len(), 1);
		call(&server, "thread_stop", &json!({ "thread": "Follow up" })).await;
		assert!(!pending_file(&project).exists());
		let unknown = call(&server, "thread_stop", &json!({ "thread": "Follow up" })).await;
		assert_eq!(unknown.text, "no thread Follow up.");
	})
	.await;
}
