//! Tests of `finish.rs`: the cases of `service-finish.test.ts` and `service-regressions.test.ts` whose subject is
//! `thread_finish`, rewritten at part level on `context::test_context` (the service and its HTTP server come in a later
//! wave). The gh fake is a generated script (`.cmd` on Windows, `sh` elsewhere) rewritten from a state, as
//! `fake-gh.ts` took its state from a file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use super::*;
use crate::frontier::server::allow::{Answer, Confirm, Confirmation};
use crate::frontier::server::context::{BossRef, Shared, TestContext, test_context};
use crate::frontier::server::herdr::HerdrError;
use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::task_file::{NewTask, new_task, save_task, task_dir};
use crate::frontier::server::thread::{Thread, ThreadPlan};
use crate::shared::git::git;
use crate::shared::projects::Project;
use crate::shared::testing::{TempDir, temp_dir};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn slashed(path: &Path) -> String {
	path.display().to_string().replace('\\', "/")
}

/// Commits `text` as the README in `dir`, as the TypeScript's `commitIn`.
fn commit_in(dir: &Path, text: &str, message: &str) {
	fs::write(dir.join("README.md"), text).unwrap();
	sh(dir, &["add", "README.md"]);
	sh(dir, &["commit", "-q", "-m", message]);
}

fn lf(text: &str) -> String {
	text.replace("\r\n", "\n")
}

/// His Allow, answered at once; every question it was asked is kept.
fn allowing(allow: bool, asked: &Arc<Mutex<Vec<Confirmation>>>) -> Confirm {
	let asked = Arc::clone(asked);
	Arc::new(move |ask: Confirmation, _signal| -> BoxFuture<'static, Result<Answer>> {
		asked.lock().unwrap().push(ask);
		Box::pin(async move { Ok(Answer { allow, how: PopupEnd::Answered }) })
	})
}

/// A stand-in for hunk's session daemon: answers every request on a free loopback port `200 {"ok":true}` for as long
/// as the test process lives, so the service finds it up and never starts a real `hunk` that would outlive the test.
fn health_port() -> u16 {
	let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listening");
	let port = listener.local_addr().expect("the daemon's address").port();
	std::thread::spawn(move || {
		for mut stream in listener.incoming().flatten() {
			let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
			let _ = std::io::Read::read(&mut stream, &mut [0u8; 1024]);
			let body = r#"{"ok":true}"#;
			let head =
				format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n", body.len());
			let _ = std::io::Write::write_all(&mut stream, format!("{head}connection: close\r\n\r\n{body}").as_bytes());
		}
	});
	port
}

/// The options most cases run with: his Allow given, hunk installed (a new review can run) and its daemon up.
fn standard(options: &mut ServiceOptions) {
	options.confirm = Some(allowing(true, &Arc::default()));
	options.hunk = Some(Some(vec!["hunk".to_owned()]));
	options.hunk_port = Some(health_port());
}

/// The TypeScript's `demoProject` and `startThread`: a repo with a README on `main`, a thread `Readme line` in a linked
/// worktree on `feature/readme-line`, its task record, registered in the context.
struct World {
	t: TestContext,
	_root: TempDir,
	main: PathBuf,
	path: PathBuf,
	thread: ThreadRef,
	record: PathBuf,
}

fn world(options: impl FnOnce(&mut ServiceOptions)) -> World {
	let t = test_context(options);
	let root = temp_dir("finish");
	let main = root.path().join("demo");
	fs::create_dir_all(&main).unwrap();
	sh(&main, &["init", "-q", "-b", "main"]);
	sh(&main, &["config", "user.name", "Test"]);
	sh(&main, &["config", "user.email", "test@example.com"]);
	fs::write(main.join("README.md"), "demo\n").unwrap();
	sh(&main, &["add", "."]);
	sh(&main, &["commit", "-q", "-m", "init"]);
	let path = root.path().join("demo.wt").join("readme-line");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/readme-line", &path.display().to_string()]);
	let project = Project {
		id: "p-demo".to_owned(),
		identity: String::new(),
		name: "demo".to_owned(),
		main_path: main.clone(),
		git: true,
		paths: Vec::new(),
	};
	let mut thread = Thread::new("p_1", "w_1", project, "Readme line", "feature/readme-line");
	thread.path.clone_from(&path);
	thread.state = AgentState::Idle;
	let task = new_task(
		NewTask {
			project_id: "p-demo".to_owned(),
			project: "demo".to_owned(),
			title: "Readme line".to_owned(),
			path: Some(path.display().to_string()),
			branch: Some("feature/readme-line".to_owned()),
			..NewTask::default()
		},
		None,
	);
	save_task(&task).unwrap();
	let record = task_dir("p-demo", &task.id).join("thread.json");
	thread.task = Some(task);
	let thread = Shared::new(thread);
	t.ctx.threads.add(thread.clone());
	World { t, _root: root, main, path, thread, record }
}

impl World {
	async fn finish(&self, how: &str) -> Result<String> {
		self.t.ctx.finisher.finish(&self.thread, how, Door::Boss).await
	}

	async fn refused(&self, how: &str) -> String {
		self.finish(how).await.unwrap_err().to_string()
	}

	fn set_state(&self, state: AgentState) {
		self.thread.with(|t| t.state = state);
	}

	fn head(&self, dir: &Path) -> String {
		sh(dir, &["rev-parse", "HEAD"])
	}

	fn closed_workspaces(&self) -> usize {
		self.t.herdr.of("closeWorkspace").len()
	}

	fn two_commits(&self) {
		commit_in(&self.path, "demo\nA line.\n", "Add a README line");
		commit_in(&self.path, "demo\nA line.\nAnother.\n", "Another line");
	}

	fn one_commit(&self) {
		commit_in(&self.path, "demo\nA line.\n", "Add a README line");
	}

	/// Everything stays: the worktree, the branch, the workspace and the thread's place in the registry.
	fn assert_in_place(&self) {
		assert!(self.path.exists());
		assert!(sh(&self.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		assert_eq!(self.closed_workspaces(), 0);
		assert!(self.t.ctx.threads.current(&self.thread));
	}

	fn boss_lines(&self) -> Vec<String> {
		self.t
			.herdr
			.of("prompt")
			.into_iter()
			.filter(|call| call[0] == json!("boss-1"))
			.map(|call| call[1].as_str().unwrap().to_owned())
			.collect()
	}

	/// The boss answered the last line and is idle again.
	fn boss_idle(&self, boss: &BossRef) {
		boss.with(|boss| boss.state = AgentState::Idle);
	}

	fn closed_as(&self) -> Value {
		let record: Value = serde_json::from_str(&fs::read_to_string(&self.record).unwrap()).unwrap();
		record["close"].clone()
	}

	/// A pre-commit hook outside the checkout, so main stays clean, that runs `script`.
	fn hook(&self, script: &str) {
		let hooks = self._root.path().join("hooks");
		fs::create_dir_all(&hooks).unwrap();
		let file = hooks.join("pre-commit");
		fs::write(&file, format!("#!/bin/sh\n{script}\n")).unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
		}
		sh(&self.main, &["config", "core.hooksPath", &slashed(&hooks)]);
	}
}

/// `Merged demo/Readme line into main as <hex> (<commits> squashed)` followed by `tail`: the commit id is whatever git
/// made.
fn assert_merged(reply: &str, commits: &str, tail: &str) {
	let head = "Merged demo/Readme line into main as ";
	let rest = reply.strip_prefix(head).unwrap_or_else(|| panic!("{reply}"));
	let (id, rest) = rest.split_once(' ').unwrap_or_else(|| panic!("{reply}"));
	assert!(!id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit()), "{reply}");
	assert_eq!(rest, format!("({commits} squashed){tail}"));
}

#[tokio::test]
async fn thread_finish_merge_refuses_a_dirty_or_off_trunk_main_checkout_and_conflicts_leaving_all_in_place_0426() {
	let w = world(standard);
	assert_eq!(w.refused("merge").await, "demo/Readme line has no commits ahead of main; nothing to finish.");
	// A question he dismissed ends the turn (CO-3); with no commits yet it was just a turn.
	w.set_state(AgentState::Blocked);
	assert_eq!(w.refused("merge").await, "demo/Readme line isn't idle (blocked); finish it once it is.");
	w.set_state(AgentState::Idle);
	w.two_commits();
	assert_eq!(w.refused("rebase").await, "thread_finish: how is \"merge\" or \"pr\".");

	fs::write(w.main.join("README.md"), "edited by hand\n").unwrap();
	assert_eq!(w.refused("merge").await, "the main checkout of demo has uncommitted changes; nothing was merged.");
	sh(&w.main, &["checkout", "--", "README.md"]);
	sh(&w.main, &["checkout", "-q", "-b", "elsewhere"]);
	assert_eq!(w.refused("merge").await, "the main checkout of demo is on elsewhere, not main; nothing was merged.");
	sh(&w.main, &["checkout", "-q", "main"]);

	// trunk moved on with a clashing line: bringing it into the branch conflicts (0454); aborted, nothing changes.
	commit_in(&w.main, "demo\nClash.\n", "Clash");
	let (trunk_head, branch_head) = (w.head(&w.main), w.head(&w.path));
	assert_eq!(
		w.refused("merge").await,
		"main moved since demo/Readme line's review, and bringing it into feature/readme-line conflicts in README.md; that merge was aborted and everything stays."
	);
	assert_eq!(w.head(&w.main), trunk_head);
	assert_eq!(w.head(&w.path), branch_head);
	assert_eq!(sh(&w.main, &["status", "--porcelain"]), "");
	assert_eq!(sh(&w.path, &["status", "--porcelain"]), "");
	w.assert_in_place();
}

#[tokio::test]
async fn thread_finish_merge_squash_merges_removes_the_worktree_and_workspace_and_deletes_the_branch_0426() {
	let w = world(standard);
	w.two_commits();
	let reply = w.finish("merge").await.unwrap();
	assert_merged(&reply, "2 commits", "; its worktree, workspace and branch are removed.");
	assert_eq!(lf(&fs::read_to_string(w.main.join("README.md")).unwrap()), "demo\nA line.\nAnother.\n");
	assert_eq!(
		sh(&w.main, &["log", "-1", "--format=%s%n%b"]),
		"Readme line\nSquash-merged from feature/readme-line:\n- Add a README line\n- Another line"
	);
	assert_eq!(sh(&w.main, &["rev-list", "--count", "HEAD"]), "2");
	assert!(!w.path.exists());
	assert_eq!(sh(&w.main, &["branch", "--list", "feature/readme-line"]), "");
	assert_eq!(w.closed_workspaces(), 1);
	// 0621: when and how it closed, and the squash commit's full id.
	let closed = w.closed_as();
	assert_eq!(closed["how"], "merged");
	assert_eq!(closed["commit"], json!(w.head(&w.main)));
	assert!(closed["at"].is_number());
	// thread_status says "No threads.": the registry holds none.
	assert!(w.t.ctx.threads.all().is_empty());
	assert_eq!(w.t.ctx.finishing("w_1"), Some(Finishing::Removed));
}

#[tokio::test]
async fn thread_finish_merge_a_pre_commit_hook_refusing_the_squash_commit_merges_nothing_tst_11_co_10() {
	let w = world(standard);
	w.one_commit();
	w.hook("echo 'lint failed' >&2\nexit 1");
	let trunk_head = w.head(&w.main);
	assert_eq!(
		w.refused("merge").await,
		"demo/Readme line: the squash commit failed (lint failed); nothing was merged."
	);
	assert_eq!(w.head(&w.main), trunk_head);
	assert_eq!(sh(&w.main, &["status", "--porcelain"]), "");
	w.assert_in_place();
}

#[tokio::test]
async fn thread_finish_merge_herdr_failing_to_close_the_workspace_after_the_merge_keeps_the_thread_tst_11() {
	let w = world(standard);
	w.one_commit();
	w.t.herdr.state().close_workspace_fails = Some(HerdrError::new("transport", "herdr is gone"));
	let reply = w.finish("merge").await.unwrap();
	assert_merged(
		&reply,
		"1 commit",
		", but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay.",
	);
	assert_eq!(sh(&w.main, &["rev-list", "--count", "HEAD"]), "2");
	assert!(w.path.exists());
	assert!(sh(&w.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	// The thread is open again and its workspace is no longer being removed (CR-11).
	assert!(w.t.ctx.threads.current(&w.thread));
	assert_eq!(w.t.ctx.finishing("w_1"), None);
	assert_eq!(w.closed_workspaces(), 1);
}

#[tokio::test]
async fn sec_loc_7_finish_refuses_a_thread_whose_worktree_was_switched_to_another_branch() {
	let w = world(standard);
	w.one_commit();
	sh(&w.path, &["checkout", "-q", "-b", "elsewhere"]);
	let trunk_head = w.head(&w.main);
	assert_eq!(
		w.refused("merge").await,
		"demo/Readme line's worktree is on elsewhere, not feature/readme-line; finish it on its branch."
	);
	assert_eq!(w.head(&w.main), trunk_head);
	assert!(sh(&w.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	assert_eq!(w.closed_workspaces(), 0);
	sh(&w.path, &["checkout", "-q", "--detach"]);
	assert_eq!(
		w.refused("merge").await,
		"demo/Readme line's worktree is on a detached HEAD, not feature/readme-line; finish it on its branch."
	);
}

/// Waits until `done` holds, at most five seconds.
async fn until(mut done: impl FnMut() -> bool) {
	for _ in 0..3_000 {
		if done() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	panic!("the condition never held");
}

/// A confirm that holds his answer until `release` is notified, and says when it was asked.
fn waiting_allow(asked: &Arc<AtomicBool>, release: &Arc<tokio::sync::Notify>) -> Confirm {
	let (asked, release) = (Arc::clone(asked), Arc::clone(release));
	Arc::new(move |_ask: Confirmation, _signal| -> BoxFuture<'static, Result<Answer>> {
		asked.store(true, Ordering::SeqCst);
		let release = Arc::clone(&release);
		Box::pin(async move {
			release.notified().await;
			Ok(Answer { allow: true, how: PopupEnd::Answered })
		})
	})
}

/// Starts `thread_finish` merge on its own task (as a tool call runs) and waits until his Allow was asked.
async fn finishing_until_asked(w: &World, asked: &Arc<AtomicBool>) -> tokio::task::JoinHandle<Result<String>> {
	let (ctx, thread) = (w.t.ctx.clone(), w.thread.clone());
	let finishing = tokio::spawn(async move { ctx.finisher.finish(&thread, "merge", Door::Boss).await });
	until(|| asked.load(Ordering::SeqCst)).await;
	finishing
}

#[tokio::test]
async fn svc_5_thread_finish_looks_again_after_his_allow_a_thread_at_work_meanwhile_isnt_merged_0544() {
	let (asked, release) = (Arc::new(AtomicBool::new(false)), Arc::new(tokio::sync::Notify::new()));
	let w = world(|options| options.confirm = Some(waiting_allow(&asked, &release)));
	w.one_commit();
	let trunk_head = w.head(&w.main);
	let finishing = finishing_until_asked(&w, &asked).await;
	// While his Allow waits, the thread is at work again.
	w.set_state(AgentState::Working);
	release.notify_one();
	let reply = finishing.await.unwrap();
	assert_eq!(reply.unwrap_err().to_string(), "demo/Readme line isn't idle (working); finish it once it is.");
	assert_eq!(w.head(&w.main), trunk_head);
	assert_eq!(w.closed_workspaces(), 0);
}

#[tokio::test]
async fn svc_5_a_thread_closed_during_his_allow_isnt_merged() {
	let (asked, release) = (Arc::new(AtomicBool::new(false)), Arc::new(tokio::sync::Notify::new()));
	let w = world(|options| options.confirm = Some(waiting_allow(&asked, &release)));
	w.one_commit();
	let finishing = finishing_until_asked(&w, &asked).await;
	w.t.ctx.threads.drop_pane("p_1");
	release.notify_one();
	let reply = finishing.await.unwrap();
	assert_eq!(reply.unwrap_err().to_string(), "demo/Readme line closed meanwhile; nothing was merged.");
	assert!(w.path.exists());
	assert_eq!(w.closed_workspaces(), 0);
}

#[tokio::test]
async fn svc_13_a_merge_in_flight_lands_and_removes_its_thread_while_the_service_is_closing_0549() {
	let w = world(standard);
	w.one_commit();
	// The repo's pre-commit hook takes a while (CO-10); Jocasta is closed meanwhile.
	let marker = w._root.path().join("hook-ran");
	w.hook(&format!("touch \"{}\"\nsleep 1", slashed(&marker)));
	let (ctx, thread) = (w.t.ctx.clone(), w.thread.clone());
	let finishing = tokio::spawn(async move {
		let inner = ctx.clone();
		ctx.calls.run(async move { inner.finisher.finish(&thread, "merge", Door::Boss).await }).await
	});
	until(|| marker.exists()).await;
	w.t.ctx.set_stopping();
	assert!(!w.t.ctx.calls.settled_within(50).await, "the merge in flight is waited for");
	assert!(w.t.ctx.calls.settled_within(60_000).await);
	assert_merged(&finishing.await.unwrap().unwrap(), "1 commit", "; its worktree, workspace and branch are removed.");
	assert!(!w.path.exists());
}

/// What a stand-in holding a folder does: the test executable again, with its working folder in the worktree, until
/// the file `HOLDER_VAR` names appears (the TypeScript's `bun -e "await Bun.sleep(…)"`, but ended by the test on a
/// condition, not a timer a loaded machine can outrun). It gives up by itself after a minute.
const HOLDER_VAR: &str = "JOCASTA_FINISH_TEST_HOLDER_RELEASE";

#[test]
fn holder_main() {
	if let Some(release) = crate::shared::env::var(HOLDER_VAR) {
		for _ in 0..3_000 {
			if Path::new(&release).exists() {
				return;
			}
			std::thread::sleep(Duration::from_millis(20));
		}
	}
}

/// A holder started in a folder; ended (and waited for) when dropped.
struct Holder {
	child: std::process::Child,
	release: PathBuf,
}

impl Holder {
	fn spawn(cwd: &Path, release: PathBuf) -> Holder {
		let mut command = Command::new(std::env::current_exe().unwrap());
		command
			.args(["--exact", "frontier::server::finish::tests::holder_main", "--nocapture", "--test-threads=1"])
			.env(HOLDER_VAR, &release)
			.current_dir(cwd)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null());
		crate::shared::env::apply_to(&mut command);
		Holder { child: command.spawn().unwrap(), release }
	}

	fn running(&mut self) -> bool {
		self.child.try_wait().unwrap().is_none()
	}

	fn release(&self) {
		fs::write(&self.release, "").unwrap();
	}
}

impl Drop for Holder {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

#[tokio::test]
async fn thread_finish_merge_removes_the_worktree_only_once_no_process_works_in_it_any_more_0486_f_02_cr_11() {
	let w = world(standard);
	w.one_commit();
	// A process outside the pane's tree (he opened it, or it outlived an earlier pane) works in a subfolder.
	fs::create_dir(w.path.join("sub")).unwrap();
	let mut holder = Holder::spawn(&w.path.join("sub"), w._root.path().join("release-holder"));
	let (ctx, thread) = (w.t.ctx.clone(), w.thread.clone());
	let finishing = tokio::spawn(async move { ctx.finisher.finish(&thread, "merge", Door::Boss).await });
	// The workspace closed, the thread gone from the registry, the folder kept for as long as the holder runs; a
	// refresh meanwhile neither finds the thread again nor reports its workspace closed (CR-11).
	until(|| w.t.ctx.finishing("w_1") == Some(Finishing::Removed)).await;
	for _ in 0..10 {
		assert!(holder.running(), "the holder ended before it was observed");
		assert!(w.path.exists());
		assert!(!w.t.ctx.threads.has("p_1"));
		assert_eq!(w.t.ctx.finishing("w_1"), Some(Finishing::Removed));
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	holder.release();
	let reply = finishing.await.unwrap().unwrap();
	assert_merged(&reply, "1 commit", "; its worktree, workspace and branch are removed.");
	assert!(!w.path.exists());
	assert_eq!(sh(&w.main, &["branch", "--list", "feature/readme-line"]), "");
	assert_eq!(w.t.ctx.finishing("w_1"), Some(Finishing::Removed));
	assert_eq!(w.t.ctx.reviewed.of(&w.path), None);
}

#[tokio::test]
async fn finish_refuses_a_wrong_how_naming_the_door() {
	let w = world(standard);
	let finisher = &w.t.ctx.finisher;
	let cli = finisher.finish(&w.thread, "rebase", Door::Cli).await.unwrap_err().to_string();
	assert_eq!(cli, "jocasta thread finish: how is \"merge\" or \"pr\".");
	let key = finisher.finish(&w.thread, "", Door::Key).await.unwrap_err().to_string();
	assert_eq!(key, "thread_finish: how is \"merge\" or \"pr\".");
}

#[tokio::test]
async fn finishable_refuses_a_thread_that_works_waits_on_him_or_is_mid_turn_0426_0544() {
	let w = world(standard);
	let finisher = &w.t.ctx.finisher;
	let thread = &w.thread;
	let refusal = |door: Door| async move { finisher.finishable(thread, door).await.unwrap_err().to_string() };
	w.set_state(AgentState::Working);
	assert_eq!(refusal(Door::Boss).await, "demo/Readme line isn't idle (working); finish it once it is.");
	w.set_state(AgentState::Idle);
	w.thread.with(|t| t.plan = Some(ThreadPlan { awaiting: Some("A plan".to_owned()), ..ThreadPlan::default() }));
	assert_eq!(refusal(Door::Boss).await, "demo/Readme line isn't idle (its plan waits on him); finish it once it is.");
	assert_eq!(refusal(Door::Cli).await, "demo/Readme line isn't idle (its plan waits on you); finish it once it is.");
	w.thread.with(|t| t.plan = None);
	for (phase, turn) in [(Phase::Review, "review"), (Phase::Finish, "PR")] {
		w.thread.with(|t| t.phase = phase);
		assert_eq!(refusal(Door::Boss).await, format!("demo/Readme line is still in its {turn} turn; wait for it."));
	}
	// A handoff that never comes stops holding it after 5 minutes (CO-2).
	w.thread.with(|t| {
		t.phase = Phase::Handoff;
		t.handoff_at = Some(now_ms());
	});
	assert_eq!(refusal(Door::Boss).await, "demo/Readme line is still writing its handoff; try again in a minute.");
	w.thread.with(|t| t.handoff_at = Some(now_ms() - HANDOFF_HOLD_MS - 1_000));
	assert_eq!(refusal(Door::Boss).await, "demo/Readme line has no commits ahead of main; nothing to finish.");
}

#[tokio::test]
async fn finishable_names_trunk_and_commits_ahead_and_refuses_uncommitted_changes_or_no_branch() {
	let w = world(standard);
	w.two_commits();
	let found = w.t.ctx.finisher.finishable(&w.thread, Door::Boss).await.unwrap();
	let expected = Finishable {
		name: "demo/Readme line".to_owned(),
		trunk: "main".to_owned(),
		ahead: 2,
		failed: None,
		overlaps: Vec::new(),
	};
	assert_eq!(found, expected);
	fs::write(w.path.join("README.md"), "dirty\n").unwrap();
	let said = w.t.ctx.finisher.finishable(&w.thread, Door::Boss).await.unwrap_err().to_string();
	assert_eq!(said, "demo/Readme line has uncommitted changes in its worktree; have it commit or drop them first.");
	sh(&w.path, &["checkout", "--", "README.md"]);
	w.thread.with(|t| t.branch = String::new());
	let said = w.t.ctx.finisher.finishable(&w.thread, Door::Boss).await.unwrap_err().to_string();
	assert_eq!(said, "demo/Readme line: the thread has no branch.");
	let bare = temp_dir("finish-no-trunk");
	w.thread.with(|t| {
		t.project.main_path = bare.path().to_path_buf();
		t.project.id = "p-bare".to_owned();
		t.branch = "feature/readme-line".to_owned();
	});
	let said = w.t.ctx.finisher.finishable(&w.thread, Door::Boss).await.unwrap_err().to_string();
	assert_eq!(said, "demo/Readme line: the project has no trunk branch.");
}

#[tokio::test]
async fn the_allow_window_names_what_it_does_and_a_deny_leaves_everything_in_place_0484_0544() {
	let asked = Arc::new(Mutex::new(Vec::new()));
	let w = world(|options| options.confirm = Some(allowing(false, &asked)));
	w.two_commits();
	assert_eq!(w.refused("merge").await, "He chose Deny; nothing was merged.");
	w.t.ctx.finisher.finish(&w.thread, "merge", Door::Cli).await.unwrap_err();
	assert_eq!(w.refused("pr").await, "He chose Deny; no PR was opened.");
	let asks = asked.lock().unwrap().clone();
	assert_eq!(asks[0].message, "Merge demo/Readme line into main?");
	assert_eq!(
		asks[0].detail,
		"The boss asks to squash-merge branch feature/readme-line (2 commits) into main in demo's main checkout, then remove the thread's worktree and workspace and delete the branch."
	);
	assert!(asks[1].detail.starts_with("A `jocasta thread finish` command asks to squash-merge"), "{}", asks[1].detail);
	assert_eq!(asks[2].message, "Open a PR for demo/Readme line?");
	assert_eq!(
		asks[2].detail,
		"The boss asks to have the thread push branch feature/readme-line (2 commits) and open a pull request against main with gh; its worktree and workspace go once the PR is open."
	);
	w.assert_in_place();
}

#[tokio::test]
async fn his_finish_key_is_his_confirmation_and_asks_nothing_0544_0568() {
	let asked = Arc::new(Mutex::new(Vec::new()));
	let w = world(|options| options.confirm = Some(allowing(false, &asked)));
	w.one_commit();
	let reply = w.t.ctx.finisher.finish(&w.thread, "merge", Door::Key).await.unwrap();
	assert_merged(&reply, "1 commit", "; its worktree, workspace and branch are removed.");
	assert!(asked.lock().unwrap().is_empty());
}

/// Trunk moved on with a line that doesn't clash, after the thread's review.
fn trunk_moves_on(w: &World, name: &str) {
	fs::write(w.main.join(name), "other\n").unwrap();
	sh(&w.main, &["add", name]);
	sh(&w.main, &["commit", "-q", "-m", &format!("Add {name}")]);
}

#[tokio::test]
async fn merge_when_trunk_moved_brings_it_into_the_branch_for_a_new_review_0454() {
	let w = world(standard);
	w.one_commit();
	trunk_moves_on(&w, "other.txt");
	let trunk_head = w.head(&w.main);
	let reply = w.finish("merge").await.unwrap();
	assert_eq!(
		reply,
		"main moved since demo/Readme line's review: it was merged into feature/readme-line, which is being tested and reviewed again; it merges into main only when that passes, and you hear either way."
	);
	assert!(w.thread.with(|t| t.merge_after_review));
	assert_eq!(w.thread.with(|t| t.head.clone()), Some(w.head(&w.path)));
	assert!(w.path.join("other.txt").exists());
	assert_eq!(w.head(&w.main), trunk_head);
	w.assert_in_place();
}

#[tokio::test]
async fn merge_when_trunk_moved_and_hunk_isnt_installed_refuses_0454() {
	let w = world(|options| {
		options.confirm = Some(allowing(true, &Arc::default()));
		options.hunk = Some(None);
	});
	w.one_commit();
	trunk_moves_on(&w, "other.txt");
	assert_eq!(
		w.refused("merge").await,
		"main moved since demo/Readme line's review and no new review can run (hunk isn't installed); nothing was merged."
	);
	assert!(!w.thread.with(|t| t.merge_after_review));
	w.assert_in_place();
}

#[tokio::test]
async fn after_review_squashes_a_merge_whose_new_review_passed_and_says_it_as_one_event_0454() {
	let w = world(standard);
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	trunk_moves_on(&w, "other.txt");
	w.t.ctx.finisher.merge(&w.thread, "main").await.unwrap();
	w.t.ctx.finisher.after_review(&w.thread, "no notes").await.unwrap();
	let lines = w.boss_lines();
	assert_eq!(lines.len(), 1, "{lines:?}");
	let said = "[event] demo/Readme line — its new review passed (no notes); merged demo/Readme line into main as ";
	assert!(lines[0].starts_with(said), "{}", lines[0]);
	assert!(lines[0].ends_with("; its worktree, workspace and branch are removed."), "{}", lines[0]);
	assert!(!w.path.exists());
	assert!(w.main.join("other.txt").exists());
	assert_eq!(sh(&w.main, &["branch", "--list", "feature/readme-line"]), "");
	w.boss_idle(&boss);
}

#[tokio::test]
async fn after_review_doesnt_merge_when_trunk_moved_again_or_main_isnt_ready_and_asks_for_the_handoff_0454() {
	let w = world(standard);
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	trunk_moves_on(&w, "other.txt");
	w.t.ctx.finisher.merge(&w.thread, "main").await.unwrap();
	trunk_moves_on(&w, "third.txt");
	w.t.ctx.finisher.after_review(&w.thread, "no notes").await.unwrap();
	assert_eq!(
		w.boss_lines(),
		[
			"[event] demo/Readme line — its new review passed (no notes); not merged: main moved again during its review; nothing was merged (finish it again)."
		]
	);
	w.boss_idle(&boss);
	fs::write(w.main.join("README.md"), "edited by hand\n").unwrap();
	w.t.ctx.finisher.after_review(&w.thread, "no notes").await.unwrap();
	let lines = w.boss_lines();
	assert_eq!(
		lines[1],
		"[event] demo/Readme line — its new review passed (no notes); not merged: the main checkout of demo has uncommitted changes; nothing was merged."
	);
	w.assert_in_place();
}

#[tokio::test]
async fn main_ready_refuses_a_changed_or_off_trunk_main_checkout_and_ignores_untracked_files_0426() {
	let w = world(standard);
	let finisher = &w.t.ctx.finisher;
	finisher.main_ready(&w.thread, "main").await.unwrap();
	fs::write(w.main.join("new.txt"), "untracked\n").unwrap();
	finisher.main_ready(&w.thread, "main").await.unwrap();
	fs::write(w.main.join("README.md"), "edited by hand\n").unwrap();
	let said = finisher.main_ready(&w.thread, "main").await.unwrap_err().to_string();
	assert_eq!(said, "the main checkout of demo has uncommitted changes; nothing was merged.");
	sh(&w.main, &["checkout", "--", "README.md"]);
	sh(&w.main, &["checkout", "-q", "--detach"]);
	let said = finisher.main_ready(&w.thread, "main").await.unwrap_err().to_string();
	assert_eq!(said, "the main checkout of demo is on a detached HEAD, not main; nothing was merged.");
}

/// The state `fake-gh.ts` took from its state file: `auth status` passes unless `auth_fails`; `pr view` prints an open
/// PR whose head is `head` (exit 1 when unset, as gh does with no PR); `hang` never answers (CO-8); `pr create` leaves
/// its arguments in `gh-create.txt` and the body file it was given in `gh-body.md` beside it, then prints the PR's URL.
#[derive(Default, Clone)]
struct GhState {
	auth_fails: bool,
	head: Option<String>,
	state: Option<&'static str>,
	hang: bool,
	/// `pr create` exits 1 after 25 lines on stderr.
	create_fails: bool,
}

/// Writes the fake gh for `state` into `dir` and returns how to start it. The same path every time: a test changes
/// what gh says by writing it again.
fn fake_gh(dir: &Path, state: &GhState) -> Vec<String> {
	let pr = state.head.as_ref().map(|head| {
		let open = state.state.unwrap_or("OPEN");
		format!("{{\"url\":\"https://github.com/test/demo/pull/7\",\"state\":\"{open}\",\"headRefOid\":\"{head}\"}}")
	});
	let login = "You are not logged into any GitHub hosts. To log in, run: gh auth login";
	let (file, text) = if cfg!(windows) {
		let wait = if state.hang { "ping -n 30 127.0.0.1 >nul\r\nexit /b 0\r\n" } else { "" };
		let auth =
			if state.auth_fails { format!("echo {login} 1>&2\r\nexit /b 1") } else { "echo github.com".to_owned() };
		let view = match &pr {
			Some(pr) => format!("echo {pr}\r\nexit /b 0"),
			None => "echo no pull requests found for branch \"%3\" 1>&2\r\nexit /b 1".to_owned(),
		};
		let create = if state.create_fails {
			"for /l %%i in (1,1,25) do echo line %%i 1>&2\r\nexit /b 1".to_owned()
		} else {
			format!(
				"set \"here=%~dp0\"\r\n>\"%here%gh-create.txt\" echo %*\r\n:scan\r\nif \"%~1\"==\"\" goto scanned\r\nif \"%~1\"==\"--body-file\" copy /y \"%~2\" \"%here%gh-body.md\" >nul\r\nshift\r\ngoto scan\r\n:scanned\r\necho {PR_URL}\r\nexit /b 0"
			)
		};
		let script = format!(
			"@echo off\r\n{wait}if \"%1 %2\"==\"auth status\" goto auth\r\nif \"%1 %2\"==\"pr view\" goto view\r\nif \"%1 %2\"==\"pr create\" goto create\r\nexit /b 0\r\n:auth\r\n{auth}\r\nexit /b 0\r\n:view\r\n{view}\r\nexit /b 0\r\n:create\r\n{create}\r\n"
		);
		("gh.cmd", script)
	} else {
		let wait = if state.hang { "sleep 30\n" } else { "" };
		let auth = if state.auth_fails { format!("echo '{login}' >&2; exit 1") } else { "echo github.com".to_owned() };
		let view = match &pr {
			Some(pr) => format!("echo '{pr}'"),
			None => "echo \"no pull requests found for branch \\\"$3\\\"\" >&2; exit 1".to_owned(),
		};
		let create = if state.create_fails {
			"for i in $(seq 1 25); do echo \"line $i\" >&2; done; exit 1".to_owned()
		} else {
			format!(
				"dir=$(dirname \"$0\"); echo \"$@\" > \"$dir/gh-create.txt\"; while [ $# -gt 0 ]; do if [ \"$1\" = --body-file ]; then cp \"$2\" \"$dir/gh-body.md\"; fi; shift; done; echo {PR_URL}"
			)
		};
		let script = format!(
			"#!/bin/sh\n{wait}case \"$1 $2\" in\n\"auth status\") {auth};;\n\"pr view\") {view};;\n\"pr create\") {create};;\nesac\n"
		);
		("gh", script)
	};
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

/// A world whose gh is the fake, in the state `state` until a test writes another.
fn pr_world(state: &GhState, extra: impl FnOnce(&mut ServiceOptions)) -> (World, TempDir, Vec<String>) {
	let dir = temp_dir("finish-gh");
	let gh = fake_gh(dir.path(), state);
	let argv = gh.clone();
	let w = world(move |options| {
		standard(options);
		options.gh = Some(argv);
		extra(options);
	});
	sh(&w.main, &["remote", "add", "origin", GITHUB_REMOTE]);
	// The push stays on this machine: GitHub's URL pushes to a bare repository of the test's.
	let bare = w._root.path().join("remote.git");
	fs::create_dir_all(&bare).unwrap();
	sh(&bare, &["init", "--bare", "-q"]);
	let key = format!("url.{}.pushInsteadOf", bare.to_string_lossy().replace('\\', "/"));
	sh(&w.main, &["config", &key, GITHUB_REMOTE]);
	(w, dir, gh)
}

const GITHUB_REMOTE: &str = "https://github.com/test/demo.git";

/// The words of the arguments the fake gh's `pr create` got, without quotes.
fn created_with(dir: &Path) -> Vec<String> {
	let said = fs::read_to_string(dir.join("gh-create.txt")).expect("gh pr create ran");
	said.replace('"', "").split_whitespace().map(str::to_owned).collect()
}

const PR_URL: &str = "https://github.com/test/demo/pull/7";

#[tokio::test]
async fn thread_finish_pr_gh_missing_is_a_clear_refusal_0426() {
	let w = world(|options| {
		standard(options);
		options.gh = Some(vec!["jocasta-test-no-such-gh".to_owned()]);
	});
	sh(&w.main, &["remote", "add", "origin", "https://github.com/test/demo.git"]);
	w.one_commit();
	assert_eq!(
		w.refused("pr").await,
		"gh isn't installed: install GitHub CLI (https://cli.github.com), then gh auth login."
	);
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	w.assert_in_place();
}

#[tokio::test]
async fn thread_finish_pr_logged_out_gh_refuses_worded_for_the_door_0426_0581() {
	let (w, dir, _) = pr_world(&GhState { auth_fails: true, ..GhState::default() }, |_| {});
	w.one_commit();
	let said =
		"gh isn't logged in (gh auth status: You are not logged into any GitHub hosts. To log in, run: gh auth login)";
	assert_eq!(w.refused("pr").await, format!("{said}; he runs gh auth login."));
	let finisher = &w.t.ctx.finisher;
	let key = finisher.open_pr(&w.thread, "main", Door::Key).await.unwrap_err().to_string();
	assert_eq!(key, format!("{said}: run gh auth login, then Finish again."));
	let cli = finisher.open_pr(&w.thread, "main", Door::Cli).await.unwrap_err().to_string();
	assert_eq!(cli, format!("{said}: run gh auth login, then jocasta thread finish again."));
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	drop(dir);
}

#[tokio::test]
async fn thread_finish_pr_on_github_pushes_and_opens_the_pr_by_code_and_removes_the_worktree_0426_0677() {
	let (w, dir, _) = pr_world(&GhState::default(), |_| {});
	w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let reply = w.finish("pr").await.unwrap();
	assert_eq!(
		reply,
		format!(
			"Opened a PR for demo/Readme line: {PR_URL}; its worktree and workspace are removed, the branch stays."
		)
	);
	// gh's argv is exactly the plan's, and no prompt went to the pane.
	let argv = created_with(dir.path());
	assert_eq!(argv[..8], ["pr", "create", "--base", "main", "--head", "feature/readme-line", "--title", "Readme"]);
	assert_eq!(argv[8], "line");
	assert_eq!(argv[9], "--body-file");
	assert!(w.t.herdr.of("prompt").iter().all(|args| args[0] != json!("p_1")), "a prompt went to the thread's pane");
	// The branch reached the remote; the body has the checks and the commits.
	let bare = w._root.path().join("remote.git");
	assert!(sh(&bare, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	let body = fs::read_to_string(dir.path().join("gh-body.md")).unwrap();
	assert!(body.contains("## Checks"), "{body}");
	assert!(body.contains("## Commits\n\n- Add a README line"), "{body}");
	// Closed as a PR on its head commit; the worktree and workspace are gone.
	let head = sh(&bare, &["rev-parse", "feature/readme-line"]);
	let closed = w.closed_as();
	assert_eq!((closed["how"].as_str(), closed["commit"].as_str()), (Some("pr"), Some(head.as_str())));
	assert!(!w.path.exists());
	assert_eq!(w.closed_workspaces(), 1);
	assert!(sh(&w.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
}

#[tokio::test]
async fn thread_finish_pr_by_code_that_fails_says_its_last_20_stderr_lines_and_retries_nothing_0677() {
	let (w, _dir, _) = pr_world(&GhState { create_fails: true, ..GhState::default() }, |_| {});
	w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let said = w.refused("pr").await;
	let lines: Vec<&str> = said.lines().collect();
	assert_eq!(lines[0], "gh pr create failed:");
	assert_eq!(lines[1].trim(), "line 6");
	assert_eq!(lines[20].trim(), "line 25");
	assert_eq!(lines[21], "Nothing was retried; the thread, its worktree and its branch stay.");
	assert!(w.t.herdr.of("prompt").iter().all(|args| args[0] != json!("p_1")), "a prompt went to the thread's pane");
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	w.assert_in_place();
}

#[tokio::test]
async fn thread_finish_pr_on_azure_opens_the_pr_with_az_after_the_push_0677() {
	let dir = temp_dir("finish-az");
	let argv = fake_az(dir.path());
	let (w, _dir, _) = pr_world(&GhState::default(), move |options| options.az = Some(argv));
	let remote = "https://dev.azure.com/example-org/client-a/_git/app";
	sh(&w.main, &["remote", "set-url", "origin", remote]);
	let bare = w._root.path().join("remote.git");
	let key = format!("url.{}.pushInsteadOf", bare.to_string_lossy().replace('\\', "/"));
	sh(&w.main, &["config", &key, remote]);
	w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let reply = w.finish("pr").await.unwrap();
	assert_eq!(
		reply,
		"Opened a PR for demo/Readme line: https://dev.azure.com/example-org/client-a/_git/app/pullrequest/42; its worktree and workspace are removed, the branch stays."
	);
	assert!(sh(&bare, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	assert!(w.t.herdr.of("prompt").iter().all(|args| args[0] != json!("p_1")), "a prompt went to the thread's pane");
	let said = fs::read_to_string(dir.path().join("az-create.txt")).unwrap().replace('"', "");
	let words: Vec<&str> = said.split_whitespace().collect();
	assert_eq!(words[..3], ["repos", "pr", "create"]);
	let after = |flag: &str| words[words.iter().position(|word| *word == flag).unwrap() + 1];
	assert_eq!(after("--org"), "https://dev.azure.com/example-org");
	assert_eq!(after("--project"), "client-a");
	assert_eq!(after("--repository"), "app");
	assert_eq!(after("--source-branch"), "feature/readme-line");
	assert_eq!(after("--target-branch"), "main");
}

/// A fake az that logs its first arguments to `az-create.txt` and answers a created PR's JSON.
fn fake_az(dir: &Path) -> Vec<String> {
	let (file, text) = if cfg!(windows) {
		("az.cmd", "@echo off\r\n>\"%~dp0az-create.txt\" echo %*\r\necho {\"pullRequestId\":42}\r\n")
	} else {
		("az", "#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/az-create.txt\"\necho '{\"pullRequestId\":42}'\n")
	};
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

#[tokio::test]
async fn thread_finish_pr_on_another_host_has_the_thread_push_and_open_the_pr_and_an_open_pr_on_its_last_commit_removes_the_worktree_0426_0677()
 {
	let (w, dir, gh) = pr_world(&GhState::default(), |_| {});
	sh(&w.main, &["remote", "set-url", "origin", "https://git.example.org/test/demo.git"]);
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let reply = w.finish("pr").await.unwrap();
	assert_eq!(
		reply,
		"Asked demo/Readme line to push feature/readme-line and open a PR against main; once it's open, its worktree and workspace are removed and you hear it."
	);
	let asked = w.t.herdr.of("prompt").pop().unwrap();
	assert_eq!(asked[0], json!("p_1"));
	let text = asked[1].as_str().unwrap();
	assert!(text.contains("git push -u origin feature/readme-line"), "{text}");
	assert!(text.contains("gh pr create --base main --head feature/readme-line"), "{text}");
	assert_eq!(w.thread.with(|t| t.phase), Phase::Finish);

	// The PR turn ends without the branch's last commit on GitHub: everything stays.
	fake_gh(dir.path(), &GhState { head: Some("0000000".to_owned()), ..GhState::default() });
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	let not_pushed = "[event] demo/Readme line — no PR: its PR doesn't have the branch's last commit (not pushed?); everything stays";
	assert_eq!(w.boss_lines(), [not_pushed]);
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	w.assert_in_place();
	// The boss answers that line and is idle again.
	w.boss_idle(&boss);

	// Asked again; this time the PR has it: the worktree and workspace go, the branch stays.
	w.finish("pr").await.unwrap();
	let head = w.head(&w.path);
	fake_gh(dir.path(), &GhState { head: Some(head.clone()), ..GhState::default() });
	assert_eq!(gh.len(), 1);
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	let opened = format!(
		"[event] demo/Readme line — PR open; its worktree and workspace are removed, the branch stays · {PR_URL}"
	);
	assert_eq!(w.boss_lines().last(), Some(&opened));
	assert!(!w.path.exists());
	assert!(sh(&w.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	// 0621: closed as a PR, its commit the pushed head.
	let closed = w.closed_as();
	assert_eq!((closed["how"].as_str(), closed["commit"].as_str()), (Some("pr"), Some(head.as_str())));
	assert_eq!(w.closed_workspaces(), 1);
	assert!(!w.t.ctx.threads.has("p_1"));
}

#[tokio::test]
async fn thread_finish_pr_without_a_remote_refuses_and_a_remote_other_than_origin_is_pushed_to() {
	let (w, _dir, _) = pr_world(&GhState::default(), |_| {});
	sh(&w.main, &["remote", "remove", "origin"]);
	w.one_commit();
	assert_eq!(w.refused("pr").await, "demo has no git remote to push feature/readme-line to.");
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	sh(&w.main, &["remote", "add", "upstream", "https://github.com/test/demo.git"]);
	w.finish("pr").await.unwrap();
	let asked = w.t.herdr.of("prompt").pop().unwrap();
	assert!(asked[1].as_str().unwrap().contains("git push -u upstream feature/readme-line"), "{}", asked[1]);
}

#[tokio::test]
async fn thread_finish_pr_gh_that_doesnt_answer_is_refused_and_changes_nothing_co_8() {
	let (w, _dir, _) =
		pr_world(&GhState { hang: true, ..GhState::default() }, |options| options.gh_timeout_ms = Some(300));
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	assert_eq!(w.refused("pr").await, "gh didn't answer in a minute; nothing was changed.");
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	// The PR turn's end with a gh that doesn't answer: everything stays.
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	assert_eq!(w.boss_lines(), ["[event] demo/Readme line — no PR: gh didn't answer in a minute; everything stays"]);
	w.boss_idle(&boss);
	w.assert_in_place();
}

#[tokio::test]
async fn pr_opened_says_why_nothing_was_removed_when_gh_finds_no_open_pr_0426() {
	let (w, dir, _) = pr_world(&GhState::default(), |_| {});
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let line = |why: &str| format!("[event] demo/Readme line — no PR: {why}; everything stays");
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	let no_pr = "gh finds no PR for feature/readme-line: no pull requests found for branch \"feature/readme-line\"";
	assert_eq!(w.boss_lines(), [line(no_pr)]);
	w.boss_idle(&boss);
	let head = w.head(&w.path);
	fake_gh(dir.path(), &GhState { head: Some(head), state: Some("CLOSED"), ..GhState::default() });
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	assert_eq!(w.boss_lines()[1], line("its PR is closed"));
	w.boss_idle(&boss);
	w.thread.with(|t| t.phase = Phase::Task);
	w.assert_in_place();
}

#[tokio::test]
async fn pr_opened_without_gh_says_gh_finds_no_pr() {
	let w = world(|options| {
		standard(options);
		options.gh = Some(vec!["jocasta-test-no-such-gh".to_owned()]);
	});
	w.t.with_boss(AgentState::Idle);
	w.one_commit();
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	assert_eq!(
		w.boss_lines(),
		["[event] demo/Readme line — no PR: gh finds no PR for feature/readme-line; everything stays"]
	);
	w.assert_in_place();
}

#[tokio::test]
async fn pr_opened_says_what_stays_when_herdr_or_git_couldnt_remove_it_0426() {
	let (w, dir, _) = pr_world(&GhState::default(), |_| {});
	let boss = w.t.with_boss(AgentState::Idle);
	w.one_commit();
	let head = w.head(&w.path);
	fake_gh(dir.path(), &GhState { head: Some(head), ..GhState::default() });
	// herdr can't close the workspace: it all stays, and the thread is open again.
	w.t.herdr.state().close_workspace_fails = Some(HerdrError::new("transport", "herdr is gone"));
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	let line = w.boss_lines().remove(0);
	assert_eq!(
		line,
		format!(
			"[event] demo/Readme line — PR open, but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay · {PR_URL}"
		)
	);
	assert!(w.path.exists());
	assert!(w.t.ctx.threads.current(&w.thread));
	assert_eq!(w.t.ctx.finishing("w_1"), None);
	w.boss_idle(&boss);
	// The workspace closes, but the worktree holds a file git won't drop: the folder and branch stay.
	w.t.herdr.state().close_workspace_fails = None;
	fs::write(w.path.join("scratch.txt"), "work in progress\n").unwrap();
	w.t.ctx.finisher.pr_opened(&w.thread).await.unwrap();
	let line = w.boss_lines().remove(1);
	let said = format!(
		"[event] demo/Readme line — PR open, but its workspace is closed, but its worktree folder {}",
		w.path.display()
	);
	assert!(line.starts_with(&said), "{line}");
	assert!(line.ends_with(&format!("); the folder and branch stay · {PR_URL}")), "{line}");
	assert!(w.path.exists());
	assert!(!w.t.ctx.threads.has("p_1"));
	assert_eq!(w.closed_as()["how"], "pr");
}
