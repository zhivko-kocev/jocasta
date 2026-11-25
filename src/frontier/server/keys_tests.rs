use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::surface::PaneInfo;
use crate::frontier::server::thread::Thread;
use crate::frontier::server::wording::Overlap;
use crate::shared::git::git;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::projects::Project;
use crate::shared::testing::{TempDir, temp_dir};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn make_repo(dir: &Path, commit: bool) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	if commit {
		// Another text and message per repo: one root commit makes one project (0103).
		let name = dir.file_name().unwrap().to_string_lossy();
		fs::write(dir.join("README.md"), format!("{name}\n")).unwrap();
		sh(dir, &["add", "."]);
		sh(dir, &["commit", "-q", "-m", &format!("init {name}")]);
	}
	real_path(dir)
}

fn pane(pane_id: &str, workspace_id: &str, label: Option<&str>) -> PaneInfo {
	PaneInfo {
		pane_id: pane_id.to_owned(),
		workspace_id: workspace_id.to_owned(),
		label: label.map(str::to_owned),
		..PaneInfo::default()
	}
}

fn project(name: &str, main: &Path) -> Project {
	Project {
		id: format!("p-{name}"),
		identity: String::new(),
		name: name.to_owned(),
		main_path: main.to_path_buf(),
		git: true,
		paths: Vec::new(),
	}
}

/// What a refused key says.
fn refusal<T>(result: Result<T>) -> String {
	match result {
		Err(Error::User { message, .. }) => message,
		Err(other) => panic!("a refusal was expected, got {other}"),
		Ok(_) => panic!("a refusal was expected"),
	}
}

fn keys(t: &TestContext) -> Keys {
	Keys::new(t.ctx.clone())
}

fn stops(t: &TestContext) -> Vec<Vec<serde_json::Value>> {
	t.herdr.of("stop")
}

#[tokio::test]
async fn stop_refuses_an_idle_thread_with_why_and_stops_a_working_or_blocked_one() {
	let t = test_context(|_| {});
	let thread = t.with_thread("1-1", "Readme line");
	let keys = keys(&t);
	thread.with(|t| t.state = AgentState::Idle);
	assert_eq!(refusal(keys.stop(Some("1-1")).await), "Stop: demo/Readme line isn't working; nothing to stop.");
	assert!(stops(&t).is_empty());
	thread.with(|t| t.state = AgentState::Working);
	assert_eq!(keys.stop(Some("1-1")).await.unwrap(), "Stopped demo/Readme line's run.");
	assert_eq!(stops(&t), [vec![serde_json::json!("1-1")]]);
	thread.with(|t| t.state = AgentState::Blocked);
	assert_eq!(keys.stop(Some("1-1")).await.unwrap(), "Stopped demo/Readme line's run.");
	assert_eq!(stops(&t).len(), 2);
	assert!(t.ctx.calls.is_empty());
}

#[tokio::test]
async fn another_pane_of_the_threads_workspace_names_the_thread_and_any_other_pane_is_refused() {
	let t = test_context(|_| {});
	let thread = t.with_thread("1-1", "Readme line");
	thread.with(|t| t.state = AgentState::Working);
	t.herdr.state().pane_list.extend([
		pane("1-1", "w-1", None),
		pane("side-1", "w-1", Some("shell")),
		pane("other", "w-other", Some("shell")),
	]);
	let keys = keys(&t);
	assert_eq!(keys.stop(Some("side-1")).await.unwrap(), "Stopped demo/Readme line's run.");
	assert_eq!(stops(&t), [vec![serde_json::json!("1-1")]]);
	let plain = "Stop: the focused pane isn't a thread's; focus a thread or pick one in today.";
	assert_eq!(refusal(keys.stop(Some("other")).await), plain);
	assert_eq!(refusal(keys.stop(Some("gone")).await), plain);
	assert_eq!(refusal(keys.stop(None).await), plain);
	assert_eq!(refusal(keys.stop(Some("")).await), plain);
}

#[tokio::test]
async fn a_thread_key_on_a_pane_that_isnt_a_threads_gets_the_plain_refusal() {
	let t = test_context(|_| {});
	let boss = t.with_boss(AgentState::Idle).with(|b| b.pane_id.clone());
	t.herdr.state().pane_list.push(pane(&boss, "w-boss", Some("Jocasta")));
	let keys = keys(&t);
	assert_eq!(
		refusal(keys.stop(Some(&boss)).await),
		"Stop: the focused pane isn't a thread's; focus a thread or pick one in today."
	);
	assert_eq!(
		refusal(keys.finishable(Some(&boss)).await),
		"Finish: the focused pane isn't a thread's; focus a thread or pick one in today."
	);
	assert_eq!(
		refusal(keys.review(None).await),
		"Review: the focused pane isn't a thread's; focus a thread or pick one in today."
	);
}

#[tokio::test]
async fn finish_refuses_a_thread_that_closed_meanwhile() {
	let t = test_context(|_| {});
	assert_eq!(refusal(keys(&t).finish("1-1", FinishHow::Merge).await), "Finish: the thread closed meanwhile.");
	assert_eq!(FinishHow::Merge.as_str(), "merge");
	assert_eq!(FinishHow::Pr.as_str(), "pr");
}

#[tokio::test]
async fn finishable_names_the_thread_and_its_branch_through_the_finisher() {
	let w = review_world().await;
	fs::write(w.path.join("README.md"), "demo\nA line.\n").unwrap();
	sh(&w.path, &["commit", "-qam", "Add a README line"]);
	let found = keys(&w.t).finishable(Some("1-1")).await.unwrap();
	assert_eq!(found.pane_id, "1-1");
	assert_eq!(found.name, "demo/Readme line");
	assert_eq!(found.trunk, "main");
	assert_eq!(found.detail, "1 commit on feature/x, onto main.");
}

#[test]
fn the_finish_detail_counts_commits_and_adds_a_failed_review_and_open_overlaps() {
	let found = |ahead, failed: Option<&str>, overlaps| crate::frontier::server::finish::Finishable {
		name: "demo/Readme line".to_owned(),
		trunk: "main".to_owned(),
		ahead,
		failed: failed.map(str::to_owned),
		overlaps,
	};
	assert_eq!(finishable_detail("feature/x", &found(1, None, Vec::new())), "1 commit on feature/x, onto main.");
	assert_eq!(finishable_detail("feature/x", &found(3, Some(""), Vec::new())), "3 commits on feature/x, onto main.");
	let overlaps = vec![Overlap { title: "Other".to_owned(), files: vec!["a.ts".to_owned(), "b.ts".to_owned()] }];
	assert_eq!(
		finishable_detail("feature/x", &found(2, Some("2 tests failed"), overlaps)),
		"2 commits on feature/x, onto main.\nIts last review wasn't a pass (2 tests failed).\nOpen threads touch the same files: Other: a.ts, b.ts."
	);
}

#[test]
fn a_finish_text_with_a_but_is_half_done_an_error_and_any_other_is_not() {
	let said = ", but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay.";
	let half = finish_outcome(format!("Merged demo/Readme line into main{said}"));
	assert!(half.is_error);
	assert!(half.text.ends_with(said));
	assert!(
		!finish_outcome("Merged demo/Readme line into main; its workspace, folder and branch are gone.".to_owned())
			.is_error
	);
}

/// A real repo `demo` on `main` and a thread `Readme line` in a worktree of it on `feature/x`, in `phase` and `state`
/// of the test's choosing.
struct Review {
	t: TestContext,
	main: PathBuf,
	path: PathBuf,
	thread: ThreadRef,
	_dir: TempDir,
}

async fn review_world() -> Review {
	let dir = temp_dir("keys");
	let t = test_context(|options| {
		options.mise = Some(None);
	});
	let main = make_repo(&real_path(dir.path()).join("demo"), true);
	let path = real_path(dir.path()).join("demo.wt").join("x");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/x", &path.display().to_string()]);
	let mut thread = Thread::new("1-1", "w-1", project("demo", &main), "Readme line", "feature/x");
	thread.path.clone_from(&path);
	let thread = crate::frontier::server::context::Shared::new(thread);
	t.ctx.threads.add(thread.clone());
	Review { t, main, path, thread, _dir: dir }
}

#[tokio::test]
async fn review_refuses_a_thread_that_works_or_is_in_another_step() {
	let w = review_world().await;
	let keys = keys(&w.t);
	w.thread.with(|t| t.state = AgentState::Working);
	assert_eq!(
		refusal(keys.review(Some("1-1")).await),
		"Review: demo/Readme line isn't idle (working); review it once it is."
	);
	w.thread.with(|t| {
		t.state = AgentState::Idle;
		t.phase = Phase::Review;
	});
	assert_eq!(refusal(keys.review(Some("1-1")).await), "Review: demo/Readme line is in its review step; wait for it.");
	w.thread.with(|t| t.phase = Phase::Handoff);
	assert_eq!(
		refusal(keys.review(Some("1-1")).await),
		"Review: demo/Readme line is in its handoff step; wait for it."
	);
	assert!(w.t.herdr.of("prompt").is_empty());
}

#[tokio::test]
async fn review_refuses_a_thread_with_no_branch_or_no_trunk_and_one_with_no_commits_ahead_of_trunk() {
	let w = review_world().await;
	let keys = keys(&w.t);
	assert_eq!(
		refusal(keys.review(Some("1-1")).await),
		"Review: demo/Readme line has no commits ahead of main; nothing to review."
	);
	w.thread.with(|t| t.branch.clear());
	assert_eq!(refusal(keys.review(Some("1-1")).await), "Review: demo/Readme line has no branch.");
	// A project whose folder is no repository: no trunk to land on.
	let empty = w._dir.path().join("empty");
	fs::create_dir_all(&empty).unwrap();
	w.thread.with(|t| {
		t.project = project("demo", &empty);
		t.project.id = "p-empty".to_owned();
		t.branch = "feature/x".to_owned();
	});
	assert_eq!(refusal(keys.review(Some("1-1")).await), "Review: demo/Readme line has no trunk branch in its project.");
}

#[tokio::test]
async fn review_of_a_thread_with_commits_ahead_starts_the_review_step_and_says_so() {
	// Its review step runs for real: the review turn is typed.
	let w = review_world().await;
	fs::write(w.path.join("README.md"), "demo\nA line.\n").unwrap();
	sh(&w.path, &["commit", "-qam", "Add a README line"]);
	let said = keys(&w.t).review(Some("1-1")).await.unwrap();
	assert_eq!(said, "Reviewing demo/Readme line: the thread tests and reviews 1 new commit.");
	assert_eq!(w.thread.with(|t| t.phase), Phase::Review);
	assert!(w.main.exists());
	assert!(w.t.ctx.calls.is_empty());
}

/// Two git projects, `client-a` and `demo`, under one watched root; the places know them.
async fn projects_world(t: &TestContext, dir: &TempDir) -> (PathBuf, PathBuf) {
	let code = real_path(dir.path()).join("code");
	let demo = make_repo(&code.join("demo"), true);
	let client = make_repo(&code.join("client-a"), true);
	let plain = code.join("notes");
	fs::create_dir_all(&plain).unwrap();
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let roots =
		format!("version: 1\nroots:\n  - path: {}\n    depth: 2\n", code.display().to_string().replace('\\', "/"));
	fs::write(file, roots).unwrap();
	t.ctx.places.sync().await;
	(demo, client)
}

#[tokio::test]
async fn the_new_thread_form_lists_git_projects_and_picks_the_first_without_another_hint() {
	let t = test_context(|_| {});
	let dir = temp_dir("keys");
	assert_eq!(
		refusal(keys(&t).new_thread_form(None).await),
		"New thread: no git project to start one in; add one with `jocasta project add`."
	);
	projects_world(&t, &dir).await;
	let form = keys(&t).new_thread_form(None).await.unwrap();
	assert_eq!(
		form,
		NewThreadForm { projects: vec!["client-a".to_owned(), "demo".to_owned()], project: "client-a".to_owned() }
	);
	// A pane he picked that no thread, start or folder explains changes nothing.
	t.herdr.state().pane_list.push(pane("shell-1", "w-9", Some("shell")));
	assert_eq!(keys(&t).new_thread_form(Some("shell-1")).await.unwrap().project, "client-a");
}

#[tokio::test]
async fn the_new_thread_form_picks_the_project_of_the_picked_thread_or_of_a_pane_beside_it_or_of_the_panes_folder() {
	let t = test_context(|_| {});
	let dir = temp_dir("keys");
	let (demo, client) = projects_world(&t, &dir).await;
	let keys = keys(&t);
	// The thread picked in today wins.
	t.with_thread("1-1", "Readme line");
	assert_eq!(keys.new_thread_form(Some("1-1")).await.unwrap().project, "demo");
	// Another pane of its workspace (a shell beside it).
	t.herdr.state().pane_list.push(pane("side-1", "w-1", Some("shell")));
	assert_eq!(keys.new_thread_form(Some("side-1")).await.unwrap().project, "demo");
	// A pane in the other project's main checkout.
	let mut in_client = pane("shell-1", "w-2", Some("shell"));
	in_client.cwd = Some(client.clone());
	let mut in_demo = pane("shell-2", "w-3", Some("shell"));
	fs::create_dir_all(demo.join("src")).unwrap();
	in_demo.cwd = Some(demo.join("src"));
	t.herdr.state().pane_list.extend([in_client, in_demo]);
	assert_eq!(keys.new_thread_form(Some("shell-1")).await.unwrap().project, "client-a");
	assert_eq!(keys.new_thread_form(Some("shell-2")).await.unwrap().project, "demo");
}

#[tokio::test]
async fn the_new_thread_form_of_a_thread_of_a_project_not_in_the_list_falls_back_to_the_first() {
	let t = test_context(|_| {});
	let dir = temp_dir("keys");
	let (_, client) = projects_world(&t, &dir).await;
	let mut thread = Thread::new("2-1", "w-2", project("gone", &client), "Old", "feature/old");
	thread.path.clone_from(&client);
	t.ctx.threads.add(crate::frontier::server::context::Shared::new(thread));
	assert_eq!(keys(&t).new_thread_form(Some("2-1")).await.unwrap().project, "client-a");
}

#[tokio::test]
async fn start_gives_the_brief_to_the_thread_start_and_an_unfinished_start_is_an_error() {
	let t = test_context(|options| {
		options.agent_wait_ms = Some(0);
		options.mise = Some(None);
	});
	let dir = temp_dir("keys");
	projects_world(&t, &dir).await;
	// omp never comes up in the pane: started, its brief not sent; half done.
	let outcome = keys(&t).start("demo", "Add a README line.").await.unwrap();
	assert!(outcome.is_error);
	assert!(outcome.text.starts_with("Started demo/"), "{}", outcome.text);
	assert!(outcome.text.ends_with(", but omp didn't come up in its pane within 0 s; the brief wasn't sent."));
	// omp up: a start that went all the way is no error.
	t.herdr.state().omp_up_on_create = Some(AgentState::Idle);
	let outcome = keys(&t).start("demo", "Second line.").await.unwrap();
	assert!(!outcome.is_error, "{}", outcome.text);
	assert!(outcome.text.ends_with(": it plans first; approve its plan in its pane."), "{}", outcome.text);
	assert!(t.ctx.calls.is_empty());
}

#[tokio::test]
async fn stop_all_aborts_every_connected_pane_the_boss_included_and_journals_it_0686() {
	let t = test_context(|_| {});
	for pane in ["boss-1", "1-1", "1-2"] {
		t.ctx
			.bridge
			.record(|recording| {
				recording.agents.insert(pane.to_owned(), crate::frontier::server::bridge::PaneAgent::default())
			})
			.expect("a recording bridge");
	}
	assert_eq!(keys(&t).stop_all().await.unwrap(), "stopped 3 turns");
	let mut aborted: Vec<String> = stops(&t).iter().map(|args| args[0].as_str().unwrap().to_owned()).collect();
	aborted.sort();
	assert_eq!(aborted, ["1-1", "1-2", "boss-1"]);
	let journal = fs::read_to_string(crate::shared::paths::data_dir().join("journal.jsonl")).unwrap();
	let line: serde_json::Value = serde_json::from_str(journal.lines().last().unwrap()).unwrap();
	assert_eq!((line["kind"].as_str(), line["panes"].as_i64()), (Some("stop_all"), Some(3)));
}
