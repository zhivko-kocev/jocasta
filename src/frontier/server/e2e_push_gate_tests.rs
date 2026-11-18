//! The cases of `test/frontier/server/push-gate.test.ts` through the real server: git's pre-push hook asks
//! `POST /api/push` with the service token, and his Allow box is the server's `confirm` option (0613). The parts of
//! the gate (the body, the texts, the box's details) are in `push_gate_tests.rs`.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::json;

use crate::frontier::server::allow::{Answer, Confirm, Confirmation};
use crate::frontier::server::helpers::{TestServer, commit_in, demo_project, http, sh, with_server};
use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::server::ServerOptions;
use crate::shared::paths::projects_file;
use crate::shared::projects_file::{PushWay, read_projects_file, set_push_way};
use crate::shared::push_hook::{PUSH_PATH, PushAnswer, PushedRef};
use crate::shared::testing::temp_jocasta;

const NONE: &str = "0000000000000000000000000000000000000000";
const REMOTE: &str = "https://user:secret@example.org/acme/demo.git";

/// What the hook sends for a push from `dir` updating `refs`.
async fn push_refs(server: &TestServer, dir: &Path, refs: Vec<PushedRef>) -> PushAnswer {
	let body = json!({ "dir": dir.display().to_string(), "remote": REMOTE, "refs": refs });
	let response = http()
		.post(format!("http://127.0.0.1:{}{PUSH_PATH}", server.port))
		.bearer_auth(&server.token)
		.json(&body)
		.send()
		.await
		.expect("the service answers");
	response.json().await.expect("a push answer")
}

/// What the hook sends for a push from `dir` updating `branches` (new branches by default).
async fn push(server: &TestServer, dir: &Path, branches: &[&str]) -> PushAnswer {
	let head = sh(dir, &["rev-parse", "HEAD"]);
	let refs = branches
		.iter()
		.map(|b| PushedRef { r#ref: format!("refs/heads/{b}"), local: head.clone(), remote: NONE.to_owned() })
		.collect();
	push_refs(server, dir, refs).await
}

fn allowed() -> PushAnswer {
	PushAnswer { allow: true, reason: None }
}

/// What his Allow box does with a question.
#[derive(Clone, Copy)]
enum Reply {
	Answer(bool, PopupEnd),
	/// Never answers, and the wait is 150 ms.
	Never,
}

/// A test server whose Allow box answers with `reply` and keeps what it was asked.
struct Box_ {
	asked: Arc<Mutex<Vec<Confirmation>>>,
	options: ServerOptions,
}

fn asking(reply: Reply) -> Box_ {
	let asked = Arc::new(Mutex::new(Vec::new()));
	let record = Arc::clone(&asked);
	let confirm: Confirm = Arc::new(move |ask, _| {
		record.lock().unwrap_or_else(PoisonError::into_inner).push(ask);
		Box::pin(async move {
			match reply {
				Reply::Answer(allow, how) => Ok(Answer { allow, how }),
				Reply::Never => std::future::pending().await,
			}
		})
	});
	let options = ServerOptions {
		confirm: Some(confirm),
		confirm_wait_ms: matches!(reply, Reply::Never).then_some(150),
		..ServerOptions::default()
	};
	Box_ { asked, options }
}

fn push_map() -> Vec<(String, crate::shared::projects_file::PushEntry)> {
	read_projects_file(None).unwrap().push
}

fn ways() -> Vec<PushWay> {
	push_map().into_iter().map(|(_, entry)| entry.way).collect()
}

const ANSWERED: Reply = Reply::Answer(true, PopupEnd::Answered);

#[tokio::test(flavor = "multi_thread")]
async fn a_repo_answered_direct_passes_unasked_one_answered_pull_requests_is_refused_unasked_naming_the_file_and_line_0613()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let box_ = asking(ANSWERED);
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		let file = projects_file();
		set_push_way(&project.main_path.display().to_string(), PushWay::Direct, Some(&file)).unwrap();
		assert_eq!(push(&server, &project.main_path, &["main"]).await, allowed());
		set_push_way(&project.main_path.display().to_string(), PushWay::PullRequests, Some(&file)).unwrap();
		let refused = push(&server, &project.main_path, &["main", "develop"]).await;
		assert!(!refused.allow);
		let line = push_map()[0].1.line;
		let reason = refused.reason.unwrap();
		assert!(reason.contains(&format!("{}, line {line}", file.display())), "{reason}");
		assert!(reason.contains("takes pull requests"));
		assert!(reason.contains("main, develop"));
		assert!(seen.lock().unwrap().is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_push_to_a_feature_branch_passes_unasked_and_writes_nothing_whatever_the_repos_answer_0613() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let box_ = asking(Reply::Answer(false, PopupEnd::Answered));
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		assert_eq!(push(&server, &project.main_path, &["feature/x", "release-2"]).await, allowed());
		assert!(seen.lock().unwrap().is_empty());
		assert!(push_map().is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_answer_he_is_asked_once_with_the_repo_branch_commits_and_pane_and_allow_writes_direct_and_passes_0613()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	commit_in(
		&project.main_path,
		"a\n",
		&format!("Add the first thing: a really long subject line that is cut by the box {}", "x".repeat(80)),
	);
	commit_in(&project.main_path, "b\n", "Fix the second thing");
	let box_ = asking(ANSWERED);
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		let head = sh(&project.main_path, &["rev-parse", "HEAD"]);
		let base = sh(&project.main_path, &["rev-parse", "HEAD~2"]);
		let refs = vec![PushedRef { r#ref: "refs/heads/main".to_owned(), local: head, remote: base }];
		assert_eq!(push_refs(&server, &project.main_path, refs).await, allowed());
		let asked = seen.lock().unwrap().clone();
		assert_eq!(asked.len(), 1);
		let ask = &asked[0];
		assert_eq!(ask.message, "Push to main in demo?");
		assert!(
			ask.detail.contains("A process without a Jocasta pane key asks to push to main of demo"),
			"{}",
			ask.detail
		);
		assert!(ask.detail.contains("(https://example.org/acme/demo.git)"));
		assert!(!ask.detail.contains("secret"));
		assert!(
			ask.detail.contains("2 commits to main: «Fix the second thing», «Add the first thing"),
			"{}",
			ask.detail
		);
		assert!(ask.detail.contains("…»"));
		assert_eq!(ways(), [PushWay::Direct]);
		// Later pushes to the repo pass unasked.
		assert_eq!(push(&server, &project.main_path, &["develop"]).await, allowed());
		assert_eq!(seen.lock().unwrap().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_writes_pull_requests_and_refuses_later_pushes_are_refused_unasked_0613() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let box_ = asking(Reply::Answer(false, PopupEnd::Answered));
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		let refused = push(&server, &project.main_path, &["main"]).await;
		assert!(!refused.allow);
		let reason = refused.reason.unwrap();
		assert!(reason.contains("chose Deny"), "{reason}");
		assert!(reason.contains("takes pull requests"));
		assert_eq!(ways(), [PushWay::PullRequests]);
		assert!(!push(&server, &project.main_path, &["master"]).await.allow);
		assert_eq!(seen.lock().unwrap().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_answer_in_the_wait_or_a_popup_closed_unanswered_refuses_and_writes_nothing_0613() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	for (reply, said) in [(Reply::Never, "No answer within"), (Reply::Answer(false, PopupEnd::Closed), "popup closed")]
	{
		let box_ = asking(reply);
		let main = project.main_path.clone();
		with_server(box_.options, |server| async move {
			let refused = push(&server, &main, &["main"]).await;
			assert!(!refused.allow);
			let reason = refused.reason.unwrap();
			assert!(reason.contains(said), "{reason}");
			assert!(push_map().is_empty());
		})
		.await;
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_push_from_a_linked_worktree_is_the_main_checkouts_its_entry_answers_and_the_answer_is_written_under_it_0613()
{
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let worktree = j.root().join("demo.wt").join("feature");
	sh(&project.main_path, &["worktree", "add", "-q", "-b", "feature/x", &worktree.display().to_string()]);
	let box_ = asking(ANSWERED);
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		assert_eq!(push(&server, &worktree, &["main"]).await, allowed());
		assert_eq!(seen.lock().unwrap().len(), 1);
		let pushes = push_map();
		assert_eq!(pushes.len(), 1);
		assert_eq!(
			pushes[0].0.to_lowercase(),
			project.main_path.display().to_string().replace('\\', "/").to_lowercase()
		);
		assert_eq!(push(&server, &project.main_path, &["main"]).await, allowed());
		assert_eq!(seen.lock().unwrap().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_projects_own_trunk_under_another_name_is_protected_two_pushes_at_once_to_a_repo_ask_once_0613() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["branch", "-m", "stable"]);
	let box_ = asking(ANSWERED);
	let seen = Arc::clone(&box_.asked);
	with_server(box_.options, |server| async move {
		assert_eq!(push(&server, &project.main_path, &["topic"]).await, allowed());
		assert!(seen.lock().unwrap().is_empty());
		let (first, second) = tokio::join!(
			push(&server, &project.main_path, &["stable"]),
			push(&server, &project.main_path, &["stable"])
		);
		assert_eq!((first, second), (allowed(), allowed()));
		assert_eq!(seen.lock().unwrap().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn his_projects_yaml_is_read_back_whole_after_the_answer_its_other_content_and_comments_are_unchanged_0613() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let file = projects_file();
	let before = fs::read_to_string(&file).unwrap();
	let box_ = asking(ANSWERED);
	with_server(box_.options, |server| async move {
		push(&server, &project.main_path, &["main"]).await;
		assert!(fs::read_to_string(&file).unwrap().starts_with(before.trim_end()));
	})
	.await;
}
