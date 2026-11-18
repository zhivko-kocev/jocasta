use std::fs;
use std::path::PathBuf;

use serde_json::json;

use super::*;
use crate::frontier::server::allow::{Answer, Confirm};
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::popups::PopupEnd;
use crate::shared::git::git;
use crate::shared::projects_file::read_projects_file;
use crate::shared::testing::{TempDir, temp_dir};

const NONE: &str = "0000000000000000000000000000000000000000";
const REMOTE: &str = "https://user:secret@example.org/acme/demo.git";

/// What his Allow box does with a question.
#[derive(Clone, Copy)]
enum Reply {
	Answer(bool, PopupEnd),
	/// Never answers, and the wait is 150 ms.
	Never,
}

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn commit_in(dir: &Path, text: &str, message: &str) {
	fs::write(dir.join("README.md"), text).unwrap();
	sh(dir, &["commit", "-qam", message]);
}

struct World {
	t: TestContext,
	asked: Arc<Mutex<Vec<Confirmation>>>,
	gate: PushGate,
	main: PathBuf,
	code: PathBuf,
	_dir: TempDir,
}

impl World {
	fn asked(&self) -> Vec<Confirmation> {
		self.asked.lock().unwrap().clone()
	}

	fn pushes(&self) -> Vec<(String, PushWay)> {
		read_projects_file(None).unwrap().push.into_iter().map(|(key, entry)| (key, entry.way)).collect()
	}

	fn head(&self, dir: &Path) -> String {
		sh(dir, &["rev-parse", "HEAD"])
	}

	/// What the hook sends for a push from `dir` updating `branches` (new branches).
	async fn push(&self, dir: &Path, branches: &[&str], pane_key: Option<&str>) -> PushAnswer {
		let head = self.head(dir);
		let refs = branches
			.iter()
			.map(|b| PushedRef { r#ref: format!("refs/heads/{b}"), local: head.clone(), remote: NONE.to_owned() })
			.collect();
		self.push_refs(dir, refs, pane_key).await
	}

	async fn push_refs(&self, dir: &Path, refs: Vec<PushedRef>, pane_key: Option<&str>) -> PushAnswer {
		let request = PushRequest {
			dir: dir.to_path_buf(),
			remote: REMOTE.to_owned(),
			refs,
			pane_key: pane_key.map(str::to_owned),
		};
		self.gate.verdict(&request).await.unwrap()
	}
}

/// A watched project `demo` (a repo on `main` with one commit) under a temp folder, his box answering with `reply`.
async fn world(reply: Reply) -> World {
	let asked = Arc::new(Mutex::new(Vec::new()));
	let record = Arc::clone(&asked);
	let t = test_context(|options| {
		let confirm: Confirm = Arc::new(move |ask, _| {
			record.lock().unwrap().push(ask);
			Box::pin(async move {
				match reply {
					Reply::Answer(allow, how) => {
						tokio::time::sleep(std::time::Duration::from_millis(20)).await;
						Ok(Answer { allow, how })
					}
					Reply::Never => std::future::pending().await,
				}
			})
		});
		options.confirm = Some(confirm);
		if matches!(reply, Reply::Never) {
			options.confirm_wait_ms = Some(150);
		}
	});
	let dir = temp_dir("push-gate");
	let code = real_path(dir.path()).join("code");
	let main = code.join("demo");
	fs::create_dir_all(&main).unwrap();
	sh(&main, &["init", "-q", "-b", "main"]);
	fs::write(main.join("README.md"), "demo\n").unwrap();
	sh(&main, &["add", "."]);
	sh(&main, &["commit", "-q", "-m", "init demo"]);
	let main = real_path(&main);
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let roots = format!(
		"version: 1\nroots:\n  - path: {}\n    depth: 2\n    ignore: [\"*.wt\"]\n",
		code.display().to_string().replace('\\', "/")
	);
	fs::write(file, roots).unwrap();
	t.ctx.places.sync().await;
	let gate = PushGate::new(Arc::clone(&t.ctx));
	World { t, asked, gate, main, code, _dir: dir }
}

const ANSWERED: Reply = Reply::Answer(true, PopupEnd::Answered);

#[test]
fn a_body_is_a_push_request_only_with_a_dir_a_remote_and_refs_of_three_strings() {
	let body = |value: serde_json::Value| value.as_object().unwrap().clone();
	let ref_ = json!({ "ref": "refs/heads/main", "local": "a", "remote": "b" });
	let parsed =
		parse_push_request(&body(json!({ "dir": "/r", "remote": "o", "refs": [ref_], "paneKey": "k" }))).unwrap();
	assert_eq!(
		parsed,
		PushRequest {
			dir: "/r".into(),
			remote: "o".to_owned(),
			refs: vec![PushedRef {
				r#ref: "refs/heads/main".to_owned(),
				local: "a".to_owned(),
				remote: "b".to_owned()
			}],
			pane_key: Some("k".to_owned()),
		}
	);
	// The pane key is optional, and only a string counts.
	assert_eq!(
		parse_push_request(&body(json!({ "dir": "/r", "remote": "o", "refs": [], "paneKey": 3 }))).unwrap().pane_key,
		None
	);
	for bad in [
		json!({ "remote": "o", "refs": [] }),
		json!({ "dir": 1, "remote": "o", "refs": [] }),
		json!({ "dir": "/r", "refs": [] }),
		json!({ "dir": "/r", "remote": "o" }),
		json!({ "dir": "/r", "remote": "o", "refs": "x" }),
		json!({ "dir": "/r", "remote": "o", "refs": [{ "ref": "a", "local": "b" }] }),
		json!({ "dir": "/r", "remote": "o", "refs": [{ "ref": "a", "local": "b", "remote": 1 }] }),
		json!({ "dir": "/r", "remote": "o", "refs": [5] }),
		json!({ "dir": "/r", "remote": "o", "refs": [null] }),
	] {
		assert_eq!(parse_push_request(&body(bad.clone())), None, "{bad}");
	}
}

#[test]
fn an_address_is_shown_without_its_user_name_and_password() {
	assert_eq!(without_credentials("https://user:secret@example.org/a/b.git"), "https://example.org/a/b.git");
	assert_eq!(without_credentials("HTTPS://token@example.org/a"), "HTTPS://example.org/a");
	assert_eq!(without_credentials("git@github.com:acme/demo.git"), "git@github.com:acme/demo.git");
	assert_eq!(without_credentials("https://example.org/a@b"), "https://example.org/a@b");
}

#[tokio::test]
async fn a_repo_answered_direct_passes_unasked_one_answered_pull_requests_is_refused_unasked_naming_the_file_and_line()
{
	let w = world(ANSWERED).await;
	let file = projects_file();
	set_push_way(&w.main.display().to_string(), PushWay::Direct, Some(&file)).unwrap();
	assert_eq!(w.push(&w.main, &["main"], None).await, PushAnswer { allow: true, reason: None });
	set_push_way(&w.main.display().to_string(), PushWay::PullRequests, Some(&file)).unwrap();
	let refused = w.push(&w.main, &["main", "develop"], None).await;
	assert!(!refused.allow);
	let line = read_projects_file(None).unwrap().push[0].1.line;
	let reason = refused.reason.unwrap();
	assert!(reason.contains(&format!("{}, line {line}", file.display())), "{reason}");
	assert!(reason.contains("takes pull requests"));
	assert!(reason.contains("main, develop"));
	assert!(w.asked().is_empty());
}

#[tokio::test]
async fn a_refusal_for_a_pull_requests_repo_reads_byte_for_byte() {
	let w = world(ANSWERED).await;
	let file = projects_file();
	set_push_way(&w.main.display().to_string(), PushWay::PullRequests, Some(&file)).unwrap();
	let line = read_projects_file(None).unwrap().push[0].1.line;
	let refused = w.push(&w.main, &["main"], None).await;
	assert_eq!(
		refused.reason.unwrap(),
		format!(
			"demo takes pull requests, so a push to main is refused: push a feature branch and open a pull request instead. The user chose this for the repo in {}, line {line}, and can change it there.",
			file.display()
		)
	);
}

#[tokio::test]
async fn a_push_to_a_feature_branch_passes_unasked_and_writes_nothing_whatever_the_repos_answer() {
	let w = world(Reply::Answer(false, PopupEnd::Answered)).await;
	assert_eq!(w.push(&w.main, &["feature/x", "release-2"], None).await, PushAnswer { allow: true, reason: None });
	assert!(w.asked().is_empty());
	assert!(w.pushes().is_empty());
}

#[tokio::test]
async fn without_an_answer_he_is_asked_once_with_the_repo_branch_commits_and_pane_and_allow_writes_direct_and_passes() {
	let w = world(ANSWERED).await;
	commit_in(
		&w.main,
		"a\n",
		&format!("Add the first thing: a really long subject line that is cut by the box {}", "x".repeat(80)),
	);
	commit_in(&w.main, "b\n", "Fix the second thing");
	let head = w.head(&w.main);
	let base = sh(&w.main, &["rev-parse", "HEAD~2"]);
	let refs = vec![PushedRef { r#ref: "refs/heads/main".to_owned(), local: head, remote: base }];
	assert_eq!(w.push_refs(&w.main, refs, None).await, PushAnswer { allow: true, reason: None });
	let asked = w.asked();
	assert_eq!(asked.len(), 1);
	let ask = &asked[0];
	assert_eq!(ask.message, "Push to main in demo?");
	assert!(ask.detail.contains("A process without a Jocasta pane key asks to push to main of demo"), "{}", ask.detail);
	assert!(ask.detail.contains("(https://example.org/acme/demo.git)"));
	assert!(!ask.detail.contains("secret"));
	assert!(ask.detail.contains("2 commits to main: «Fix the second thing», «Add the first thing"), "{}", ask.detail);
	assert!(ask.detail.contains("…»"));
	let file = projects_file();
	assert!(ask.detail.ends_with(&format!(
		"Allow: demo takes pushes straight to its protected branches from now on, unasked. Deny: it takes pull requests instead. Either is kept in {}.",
		file.display()
	)));
	assert_eq!(w.pushes().into_iter().map(|(_, way)| way).collect::<Vec<_>>(), [PushWay::Direct]);
	// Later pushes to the repo pass unasked.
	assert_eq!(w.push(&w.main, &["develop"], None).await, PushAnswer { allow: true, reason: None });
	assert_eq!(w.asked().len(), 1);
}

#[tokio::test]
async fn deny_writes_pull_requests_and_refuses_later_pushes_are_refused_unasked() {
	let w = world(Reply::Answer(false, PopupEnd::Answered)).await;
	let refused = w.push(&w.main, &["main"], None).await;
	assert!(!refused.allow);
	let reason = refused.reason.unwrap();
	assert!(reason.contains("chose Deny"), "{reason}");
	assert!(reason.contains("takes pull requests"));
	assert_eq!(
		reason,
		format!(
			"He chose Deny; nothing was pushed. demo now takes pull requests, as written in {}.",
			projects_file().display()
		)
	);
	assert_eq!(w.pushes().into_iter().map(|(_, way)| way).collect::<Vec<_>>(), [PushWay::PullRequests]);
	assert!(!w.push(&w.main, &["master"], None).await.allow);
	assert_eq!(w.asked().len(), 1);
}

#[tokio::test]
async fn no_answer_in_the_wait_or_a_popup_closed_unanswered_refuses_and_writes_nothing() {
	for (reply, said) in [(Reply::Never, "No answer within"), (Reply::Answer(false, PopupEnd::Closed), "popup closed")]
	{
		let w = world(reply).await;
		let refused = w.push(&w.main, &["main"], None).await;
		assert!(!refused.allow);
		let reason = refused.reason.unwrap();
		assert!(reason.contains(said), "{reason}");
		assert!(w.pushes().is_empty());
	}
}

#[tokio::test]
async fn a_push_from_a_linked_worktree_is_the_main_checkouts_its_entry_answers_and_the_answer_is_written_under_it() {
	let w = world(ANSWERED).await;
	let worktree = w.code.join("demo.wt").join("feature");
	sh(&w.main, &["worktree", "add", "-q", "-b", "feature/x", &worktree.display().to_string()]);
	assert_eq!(w.push(&worktree, &["main"], None).await, PushAnswer { allow: true, reason: None });
	assert_eq!(w.asked().len(), 1);
	let pushes = w.pushes();
	assert_eq!(pushes.len(), 1);
	assert_eq!(pushes[0].0.to_lowercase(), w.main.display().to_string().replace('\\', "/").to_lowercase());
	assert_eq!(w.push(&w.main, &["main"], None).await, PushAnswer { allow: true, reason: None });
	assert_eq!(w.asked().len(), 1);
}

#[tokio::test]
async fn the_projects_own_trunk_under_another_name_is_protected_and_two_pushes_at_once_to_a_repo_ask_once() {
	let w = world(ANSWERED).await;
	sh(&w.main, &["branch", "-m", "stable"]);
	assert_eq!(w.push(&w.main, &["topic"], None).await, PushAnswer { allow: true, reason: None });
	assert!(w.asked().is_empty());
	let (first, second) = tokio::join!(w.push(&w.main, &["stable"], None), w.push(&w.main, &["stable"], None));
	let allowed = PushAnswer { allow: true, reason: None };
	assert_eq!((first, second), (allowed.clone(), allowed));
	assert_eq!(w.asked().len(), 1);
}

#[tokio::test]
async fn his_projects_yaml_is_read_back_whole_after_the_answer_its_other_content_is_unchanged() {
	let w = world(ANSWERED).await;
	let file = projects_file();
	let before = fs::read_to_string(&file).unwrap();
	w.push(&w.main, &["main"], None).await;
	assert!(fs::read_to_string(&file).unwrap().starts_with(before.trim_end()));
}

#[tokio::test]
async fn a_folder_that_is_no_git_repository_is_refused_with_the_folder_named() {
	let w = world(ANSWERED).await;
	let plain = w.code.join("plain");
	fs::create_dir_all(&plain).unwrap();
	let answer = w.push_refs(&plain, Vec::new(), None).await;
	assert_eq!(
		answer,
		PushAnswer {
			allow: false,
			reason: Some(format!("Jocasta found no git repository at {}, so this push is refused.", plain.display()))
		}
	);
}

#[tokio::test]
async fn the_box_names_the_pane_that_pushed_a_thread_the_boss_or_a_project_pane() {
	let w = world(Reply::Answer(false, PopupEnd::Closed)).await;
	let main = w.main.display().to_string();
	w.t.with_thread("p-thread", "Readme line");
	w.t.with_boss(crate::frontier::server::herdr::AgentState::Idle);
	w.t.ctx.launched.add("p-thread", &main, Some("key-thread")).unwrap();
	w.t.ctx.launched.add("boss-1", &main, Some("key-boss")).unwrap();
	w.t.ctx.launched.add("p-project", &main, Some("key-project")).unwrap();
	for key in ["key-thread", "key-boss", "key-project", "key-unknown"] {
		w.push(&w.main, &["main"], Some(key)).await;
	}
	let details: Vec<String> = w.asked().iter().map(|ask| ask.detail.lines().next().unwrap().to_owned()).collect();
	let ends = " asks to push to main of demo (https://example.org/acme/demo.git).";
	assert_eq!(
		details,
		[
			format!("The thread demo/Readme line{ends}"),
			format!("The boss{ends}"),
			format!("A project pane{ends}"),
			format!("A process without a Jocasta pane key{ends}"),
		]
	);
}

#[tokio::test]
async fn the_box_says_deletes_nothing_new_and_how_many_more_commits_than_it_names() {
	let w = world(Reply::Answer(false, PopupEnd::Closed)).await;
	for n in 1..=6 {
		commit_in(&w.main, &format!("{n}\n"), &format!("Commit {n}"));
	}
	let head = w.head(&w.main);
	let refs = vec![
		PushedRef { r#ref: "refs/heads/main".to_owned(), local: NONE.to_owned(), remote: head.clone() },
		PushedRef { r#ref: "refs/heads/develop".to_owned(), local: head.clone(), remote: head.clone() },
		PushedRef { r#ref: "refs/heads/master".to_owned(), local: head, remote: NONE.to_owned() },
	];
	w.push_refs(&w.main, refs, None).await;
	let detail = w.asked()[0].detail.clone();
	let lines: Vec<&str> = detail.lines().collect();
	assert_eq!(lines[1], "It deletes main.");
	assert_eq!(lines[2], "Nothing new reaches develop.");
	assert_eq!(
		lines[3],
		"7 commits to master: «Commit 6», «Commit 5», «Commit 4», «Commit 3», «Commit 2», and 2 more."
	);
	assert_eq!(w.asked()[0].message, "Push to main, develop, master in demo?");
}
