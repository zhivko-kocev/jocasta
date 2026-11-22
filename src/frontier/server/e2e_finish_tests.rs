//! The cases of `test/frontier/server/service-finish.test.ts`, run on the real server (wave 12): `thread_finish` by merge
//! and by PR, a restart's reuse of the hunk pane, events raised together, a lost idle event, the worktree's removal
//! once nothing holds it, a review that can't count notes, the feed's text, the server before its service, and
//! yesterday's worklog.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use regex::Regex;
use serde_json::{Value, json};

use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::day::DayOptions;
use crate::frontier::server::helpers::{
	Page, TestServer, ThreadOptions, boss_pane, call, commit_in, demo_project, push_state, relay_test_options, review,
	sh, start_test_server_with, start_thread, task_dir_of, until, use_hunk_daemon, with_server,
};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Herdr, HerdrError, TabSpec};
use crate::frontier::server::server::{ServerOptions, start_server};
use crate::frontier::server::service::JocastaService;
use crate::frontier::server::wording::{HANDOFF_PROMPT, quoted};
use crate::frontier::server::worklog::worklog_path;
use crate::shared::paths::data_dir;
use crate::shared::projects::Project;
use crate::shared::testing::{temp_dir, temp_jocasta};

/// The stand-in `finish_tests.rs` runs again to hold a folder until the file named by `HOLDER_VAR` exists.
const HOLDER_TEST: &str = "frontier::server::finish::tests::holder_main";
const HOLDER_VAR: &str = "JOCASTA_FINISH_TEST_HOLDER_RELEASE";

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn text_error(text: &str) -> crate::frontier::server::tools::ToolReply {
	crate::frontier::server::tools::ToolReply { text: text.to_owned(), is_error: Some(true) }
}

fn path_text(path: &Path) -> String {
	path.display().to_string()
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

fn prompts_to(server: &TestServer, pane: &str) -> Vec<String> {
	prompts(server).into_iter().filter(|(to, _)| to == pane).map(|(_, text)| text).collect()
}

fn count_of(server: &TestServer, wanted: &str) -> usize {
	prompts(server).iter().filter(|(_, text)| text == wanted).count()
}

async fn status(server: &TestServer) -> String {
	call(server, "thread_status", &json!({})).await.text
}

fn git_config(project: &Project) {
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
}

fn read_json(file: &Path) -> Value {
	serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

fn wait_options() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

fn ended(
	server: &TestServer,
	thread_args: &Value,
) -> impl Future<Output = crate::frontier::server::tools::ToolReply> + use<> {
	let server = server.clone();
	let args = thread_args.clone();
	async move { call(&server, "thread_finish", &args).await }
}

fn merge_args() -> Value {
	json!({ "thread": "Readme line", "how": "merge" })
}

/// Runs `run` on a server over `herdr` and always stops it.
async fn with_herdr<F, Fut>(herdr: Arc<FakeHerdr>, options: ServerOptions, run: F)
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = ()>,
{
	let server = start_test_server_with(herdr, options).await;
	let outcome = std::panic::AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	if let Err(panic) = outcome {
		std::panic::resume_unwind(panic);
	}
}

// ---- a gh that answers from what the test last wrote ----

/// Writes the fake gh into `dir` and returns how to start it: `auth status` passes unless `auth_fails`; `pr view`
/// prints an open PR whose head is `head` (exit 1 when unset, as gh does with no PR). The same path every time: a
/// test changes what gh says by writing it again (the TypeScript's state file).
fn fake_gh(dir: &Path, auth_fails: bool, head: Option<&str>) -> Vec<String> {
	let pr = head.map(|head| {
		format!("{{\"url\":\"https://github.com/test/demo/pull/7\",\"state\":\"OPEN\",\"headRefOid\":\"{head}\"}}")
	});
	let login = "You are not logged into any GitHub hosts. To log in, run: gh auth login";
	let (file, text) = if cfg!(windows) {
		let auth = if auth_fails { format!("echo {login} 1>&2\r\nexit /b 1") } else { "echo github.com".to_owned() };
		let view = match &pr {
			Some(pr) => format!("echo {pr}\r\nexit /b 0"),
			None => "echo no pull requests found for branch \"%3\" 1>&2\r\nexit /b 1".to_owned(),
		};
		let script = format!(
			"@echo off\r\nif \"%1 %2\"==\"auth status\" goto auth\r\nif \"%1 %2\"==\"pr view\" goto view\r\nexit /b 0\r\n:auth\r\n{auth}\r\nexit /b 0\r\n:view\r\n{view}\r\n"
		);
		("gh.cmd", script)
	} else {
		let auth = if auth_fails { format!("echo '{login}' >&2; exit 1") } else { "echo github.com".to_owned() };
		let view = match &pr {
			Some(pr) => format!("echo '{pr}'"),
			None => "echo \"no pull requests found for branch \\\"$3\\\"\" >&2; exit 1".to_owned(),
		};
		("gh", format!("#!/bin/sh\ncase \"$1 $2\" in\n\"auth status\") {auth};;\n\"pr view\") {view};;\nesac\n"))
	};
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	make_runnable(&path);
	vec![path_text(&path)]
}

#[cfg(unix)]
fn make_runnable(path: &Path) {
	use std::os::unix::fs::PermissionsExt;
	fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_runnable(_path: &Path) {}

/// A hunk that always fails (the TypeScript's `no-such-hunk.ts`): its notes can't be counted.
fn broken_hunk(dir: &Path) -> Vec<String> {
	let (name, text) =
		if cfg!(windows) { ("hunk.cmd", "@echo off\r\nexit /b 1\r\n") } else { ("hunk", "#!/bin/sh\nexit 1\n") };
	let path = dir.join(name);
	fs::write(&path, text).unwrap();
	make_runnable(&path);
	vec![path_text(&path)]
}

// ---- processes holding a folder ----

/// A process working in a folder until released: the test executable again on `holder_main`, ended when dropped.
struct Holder {
	child: Child,
	release: PathBuf,
}

impl Holder {
	fn spawn(cwd: &Path, release: PathBuf) -> Holder {
		let mut command = Command::new(std::env::current_exe().unwrap());
		command
			.args(["--exact", HOLDER_TEST, "--nocapture", "--test-threads=1"])
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

fn merged_reply(text: &str, commits: &str, tail: &str) -> bool {
	let pattern =
		format!(r"^Merged demo/Readme line into main as [0-9a-f]+ \({commits} squashed\){}$", regex::escape(tail));
	Regex::new(&pattern).unwrap().is_match(text)
}

// ---- thread_finish merge ----

/// The refusals before the merge: no commits, not idle, a wrong `how`, a changed or off-trunk main checkout, trunk
/// moved on with a clash.
async fn check_merge_refusals(server: &TestServer, project: &Project, pane: &str, path: &Path) {
	let main = &project.main_path;
	let finish = |how: &'static str| {
		let server = server.clone();
		async move { call(&server, "thread_finish", &json!({ "thread": "Readme line", "how": how })).await }
	};
	assert_eq!(finish("merge").await, text_error("demo/Readme line has no commits ahead of main; nothing to finish."));
	// A question he dismissed ends the turn (CO-3); with no commits yet it was just a turn.
	push_state(server, pane, AgentState::Blocked, None).await;
	assert_eq!(finish("merge").await.text, "demo/Readme line isn't idle (blocked); finish it once it is.");
	push_state(server, pane, AgentState::Idle, None).await;
	commit_in(path, "demo\nA line.\n", "Add a README line");
	commit_in(path, "demo\nA line.\nAnother.\n", "Another line");
	assert_eq!(finish("rebase").await.text, "thread_finish: how is \"merge\" or \"pr\".");

	fs::write(main.join("README.md"), "edited by hand\n").unwrap();
	assert_eq!(
		finish("merge").await,
		text_error("the main checkout of demo has uncommitted changes; nothing was merged.")
	);
	sh(main, &["checkout", "--", "README.md"]);
	sh(main, &["checkout", "-q", "-b", "elsewhere"]);
	assert_eq!(finish("merge").await.text, "the main checkout of demo is on elsewhere, not main; nothing was merged.");
	sh(main, &["checkout", "-q", "main"]);

	// trunk moved on with a clashing line: bringing it into the branch conflicts (0454); aborted, nothing changes.
	commit_in(main, "demo\nClash.\n", "Clash");
	let trunk_head = sh(main, &["rev-parse", "HEAD"]);
	let branch_head = sh(path, &["rev-parse", "HEAD"]);
	assert_eq!(
		finish("merge").await,
		text_error(
			"main moved since demo/Readme line's review, and bringing it into feature/readme-line conflicts in README.md; that merge was aborted and everything stays."
		)
	);
	assert_eq!(sh(main, &["rev-parse", "HEAD"]), trunk_head);
	assert_eq!(sh(path, &["rev-parse", "HEAD"]), branch_head);
	assert_eq!(sh(main, &["status", "--porcelain"]), "");
	assert_eq!(sh(path, &["status", "--porcelain"]), "");
	assert!(path.exists());
	assert!(server.herdr.of("closeWorkspace").is_empty());
	sh(main, &["reset", "-q", "--hard", "HEAD~1"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_refuses_a_dirty_or_off_trunk_main_checkout_and_conflicts_leaving_all_in_place_then_squash_merges_removes_the_worktree_and_workspace_and_deletes_the_branch_0426()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	with_server(wait_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane, path) = (started.pane_id, started.path);
		check_merge_refusals(&server, &project, &pane, &path).await;
		let main = &project.main_path;
		let record = task_dir_of(&project.id, "Readme line").join("thread.json");
		let reply = ended(&server, &merge_args()).await;
		assert!(reply.is_error.is_none());
		assert!(
			merged_reply(&reply.text, "2 commits", "; its worktree, workspace and branch are removed."),
			"{}",
			reply.text
		);
		assert_eq!(
			fs::read_to_string(main.join("README.md")).unwrap().replace("\r\n", "\n"),
			"demo\nA line.\nAnother.\n"
		);
		assert_eq!(
			sh(main, &["log", "-1", "--format=%s%n%b"]),
			"Readme line\nSquash-merged from feature/readme-line:\n- Add a README line\n- Another line"
		);
		assert_eq!(sh(main, &["rev-list", "--count", "HEAD"]), "2");
		assert!(!path.exists());
		assert_eq!(sh(main, &["branch", "--list", "feature/readme-line"]), "");
		assert_eq!(server.herdr.of("closeWorkspace").len(), 1);
		// 0621: when and how it closed, and the squash commit's full id.
		let close = &read_json(&record)["close"];
		assert_eq!(close["how"], "merged");
		assert_eq!(close["commit"], json!(sh(main, &["rev-parse", "HEAD"])));
		assert!(close["at"].is_number());
		assert_eq!(status(&server).await, "No threads.");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_a_pre_commit_hook_refusing_the_squash_commit_merges_nothing_and_leaves_his_main_checkout_as_it_was_herdr_failing_to_close_the_workspace_after_the_merge_keeps_the_thread_tst_11_co_10()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	let root = j.root().to_path_buf();
	with_server(ServerOptions::default(), |server| async move {
		let main = &project.main_path;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let path = started.path;
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		// The repo's own hook, outside the checkout so main stays clean: it refuses every commit from now on.
		let hooks = root.join("hooks");
		fs::create_dir_all(&hooks).unwrap();
		fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho 'lint failed' >&2\nexit 1\n").unwrap();
		make_runnable(&hooks.join("pre-commit"));
		sh(main, &["config", "core.hooksPath", &path_text(&hooks)]);
		let trunk_head = sh(main, &["rev-parse", "HEAD"]);
		assert_eq!(
			ended(&server, &merge_args()).await,
			text_error("demo/Readme line: the squash commit failed (lint failed); nothing was merged.")
		);
		assert_eq!(sh(main, &["rev-parse", "HEAD"]), trunk_head);
		assert_eq!(sh(main, &["status", "--porcelain"]), "");
		assert!(path.exists());
		assert!(sh(main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		assert!(server.herdr.of("closeWorkspace").is_empty());
		assert!(status(&server).await.starts_with("demo/Readme line [feature/readme-line]"));

		// The hook lets it through; herdr then fails to close the workspace: the commit has landed, the rest stays.
		sh(main, &["config", "--unset", "core.hooksPath"]);
		server.herdr.state().close_workspace_fails = Some(HerdrError::new("error", "herdr is gone"));
		let reply = ended(&server, &merge_args()).await;
		assert!(
			merged_reply(
				&reply.text,
				"1 commit",
				", but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay."
			),
			"{}",
			reply.text
		);
		assert_eq!(sh(main, &["rev-list", "--count", "HEAD"]), "2");
		assert!(path.exists());
		assert!(sh(main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		assert!(status(&server).await.starts_with("demo/Readme line [feature/readme-line]"));
	})
	.await;
}

// ---- thread_finish pr ----

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_pr_gh_missing_or_logged_out_is_a_clear_refusal_else_the_thread_pushes_and_opens_the_pr_and_an_open_pr_on_its_last_commit_removes_the_worktree_0426()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["remote", "add", "origin", "https://github.com/test/demo.git"]);
	let missing = ServerOptions { gh: Some(vec!["jocasta-test-no-such-gh".to_owned()]), ..wait_options() };
	with_server(missing, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let reply = ended(&server, &json!({ "thread": "Readme line", "how": "pr" })).await;
		assert_eq!(
			reply,
			text_error("gh isn't installed: install GitHub CLI (https://cli.github.com), then gh auth login.")
		);
	})
	.await;
	sh(
		&project.main_path,
		&["worktree", "remove", "--force", &path_text(&j.root().join("demo.wt").join("readme-line"))],
	);
	sh(&project.main_path, &["branch", "-D", "feature/readme-line"]);

	let dir = temp_dir("e2e-finish-gh");
	let gh = fake_gh(dir.path(), false, None);
	let options = ServerOptions { gh: Some(gh), ..wait_options() };
	let (gh_dir, main) = (dir.path().to_path_buf(), project.main_path.clone());
	with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane, path) = (started.pane_id, started.path);
		// The remote added above gives the project its id; the data folder holds this one project.
		let project_id = fs::read_dir(data_dir().join("projects")).unwrap().next().unwrap().unwrap().file_name();
		let record = task_dir_of(&project_id.to_string_lossy(), "Readme line").join("thread.json");
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		let pr = json!({ "thread": "Readme line", "how": "pr" });
		fake_gh(&gh_dir, true, None);
		assert_eq!(
			ended(&server, &pr).await.text,
			"gh isn't logged in (gh auth status: You are not logged into any GitHub hosts. To log in, run: gh auth login); he runs gh auth login."
		);
		fake_gh(&gh_dir, false, None);
		assert_eq!(
			ended(&server, &pr).await.text,
			"Asked demo/Readme line to push feature/readme-line and open a PR against main; once it's open, its worktree and workspace are removed and you hear it."
		);
		let (asked_pane, asked) = prompts(&server).pop().unwrap();
		assert_eq!(asked_pane, pane);
		assert!(asked.contains("git push -u origin feature/readme-line"));
		assert!(asked.contains("gh pr create --base main --head feature/readme-line"));

		// The PR turn ends without the branch's last commit on GitHub: everything stays.
		fake_gh(&gh_dir, false, Some("0000000"));
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		let not_pushed = "[event] demo/Readme line — no PR: its PR doesn't have the branch's last commit (not pushed?); everything stays";
		until(|| prompts_to(&server, &boss).iter().any(|text| text == not_pushed)).await;
		assert!(server.herdr.of("closeWorkspace").is_empty());
		// The boss answers that line and is idle again.
		server.herdr.set_state(&boss, AgentState::Idle, None, true);

		// Asked again; this time the PR has it: the worktree and workspace go, the branch stays.
		ended(&server, &pr).await;
		let head = sh(&path, &["rev-parse", "HEAD"]);
		fake_gh(&gh_dir, false, Some(&head));
		server.herdr.set_state(&pane, AgentState::Working, None, true);
		server.herdr.set_state(&pane, AgentState::Idle, None, true);
		let opened = "[event] demo/Readme line — PR open; its worktree and workspace are removed, the branch stays · https://github.com/test/demo/pull/7";
		until(|| prompts_to(&server, &boss).iter().any(|text| text == opened)).await;
		assert!(!path.exists());
		assert!(sh(&main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		// 0621: closed as a PR, its commit the pushed head.
		let close = &read_json(&record)["close"];
		assert_eq!(close["how"], "pr");
		assert_eq!(close["commit"], json!(head));
	})
	.await;
}

// ---- a restart, events together, a lost event ----

#[tokio::test(flavor = "multi_thread")]
async fn after_a_service_restart_the_next_task_reuses_the_threads_hunk_pane_a_hunk_pane_or_a_plain_shell_in_a_worktree_is_never_a_thread_cr_01_cr_02_f_13()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	let range = "main...feature/readme-line";
	let hunk = use_hunk_daemon().await;
	let mut first = wait_options();
	hunk.apply(&mut first);
	let (pane, path) = {
		let server = start_test_server_with(herdr.clone(), first).await;
		let outcome = std::panic::AssertUnwindSafe(async {
			let started = start_thread(&server, ThreadOptions::default()).await;
			herdr.set_state(&started.pane_id, AgentState::Working, None, true);
			commit_in(&started.path, "demo\nA line.\n", "Add a README line");
			herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
			until(|| herdr.of("splitBeside").len() == 1).await;
			herdr.set_state(&started.pane_id, AgentState::Working, None, true);
			herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
			until(|| herdr.of("prompt").iter().any(|args| args[1] == json!(HANDOFF_PROMPT))).await;
			(started.pane_id, started.path)
		})
		.catch_unwind()
		.await;
		server.stop().await;
		outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
	};
	// He opens a shell beside it, in another tab of the thread's workspace.
	let workspace_id =
		herdr.state().pane_list.iter().find(|p| p.pane_id == pane).map(|p| p.workspace_id.clone()).unwrap();
	let tab = TabSpec { title: "shell".to_owned(), cwd: path.clone(), command: vec!["pwsh".to_owned()], env: None };
	let shell = herdr.create_tab(&workspace_id, &tab).await.unwrap();
	let mut second = ServerOptions::default();
	hunk.apply(&mut second);
	with_herdr(herdr.clone(), second, |server| {
		let herdr = herdr.clone();
		async move {
			assert_eq!(status(&server).await.split('\n').count(), 1);
			// The next task: the same hunk pane, no second one.
			herdr.set_state(&pane, AgentState::Working, None, true);
			commit_in(&path, "demo\nA line.\nAnother.\n", "Another line");
			herdr.set_state(&pane, AgentState::Idle, None, true);
			let asked = review(range, Some(crate::frontier::server::helpers::BRIEF));
			until(|| herdr.of("prompt").iter().filter(|args| args[1] == json!(asked)).count() == 2).await;
			assert_eq!(herdr.of("splitBeside").len(), 1);

			// Its omp pane closes: the hunk pane and the shell left in the workspace aren't adopted as a thread.
			herdr.state().pane_list.retain(|p| p.pane_id != pane);
			server.service.refresh().await;
			server.service.refresh().await;
			assert_eq!(status(&server).await, "No threads.");
			assert!(herdr.state().pane_list.iter().any(|p| p.pane_id == shell.pane_id));
			assert!(!herdr.of("reportPaneMetadata").iter().any(|args| args[0] == json!(shell.pane_id)));
		}
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn events_raised_together_reach_a_busy_boss_as_one_prompt_after_its_turn_not_back_to_back_cr_08() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { submit_wait_ms: Some(0), ..wait_options() };
	with_server(options, |server| async move {
		let first = start_thread(&server, ThreadOptions::default()).await;
		let second = start_thread(
			&server,
			ThreadOptions { title: Some("Two"), brief: Some("More."), ..ThreadOptions::default() },
		)
		.await;
		assert!(second.reply.starts_with("Started demo/Two"));
		let boss = boss_pane(&server.herdr).unwrap();
		// The closes below must find the boss idle, or both lines wait and go out as one prompt when that idle lands.
		push_state(&server, &boss, AgentState::Idle, None).await;
		// Both threads' workspaces close in one refresh.
		let ids: Vec<String> = {
			let state = server.herdr.state();
			let mut ids: Vec<String> = Vec::new();
			for pane in state.pane_list.iter().filter(|p| p.pane_id != boss) {
				let demo =
					state.workspace_list.iter().find(|w| w.id == pane.workspace_id).is_some_and(|w| w.label == "demo");
				if !ids.contains(&pane.workspace_id) && !demo {
					ids.push(pane.workspace_id.clone());
				}
			}
			ids
		};
		for id in ids {
			server.herdr.close_workspace(&id).await.unwrap();
		}
		assert!(!first.pane_id.is_empty());
		server.service.refresh().await;
		assert_eq!(prompts_to(&server, &boss).len(), 1);
		assert!(prompts_to(&server, &boss)[0].contains("closed"));
		// The boss's turn on the first line ends: the second follows.
		server.herdr.set_state(&boss, AgentState::Working, None, true);
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		until(|| prompts_to(&server, &boss).len() == 2).await;
		assert!(prompts_to(&server, &boss)[1].contains("closed"));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_end_whose_idle_event_herdr_lost_is_caught_on_the_refresh_tick_one_pushed_after_the_read_isnt_undone_cr_09()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let hunk = use_hunk_daemon().await;
	let mut options = wait_options();
	hunk.apply(&mut options);
	with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		until(|| server.service.hud(None).is_ok_and(|hud| hud.running == 1)).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		// herdr's idle event never arrives.
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, false);
		server.service.refresh().await;
		until(|| server.herdr.of("splitBeside").len() == 1).await;
		let asked = review("main...feature/readme-line", Some(crate::frontier::server::helpers::BRIEF));
		// The review's prompt follows its pane's split: once there, it is there once.
		until(|| count_of(&server, &asked) >= 1).await;
		assert_eq!(count_of(&server, &asked), 1);
	})
	.await;
}

// ---- the worktree goes once nothing holds it ----

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_the_workspace_closes_first_the_worktree_goes_once_the_shells_holding_it_exit_and_a_refresh_meanwhile_neither_finds_the_thread_again_nor_reports_it_closed_f_02_cr_11_f_13()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	let release = j.root().join("release-holder");
	with_server(wait_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let path = started.path;
		let boss = boss_pane(&server.herdr).unwrap();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		// As herdr does on Windows, closing leaves a process in the worktree for a moment (it locks the folder there);
		// a refresh runs in the middle.
		let holder: Arc<Mutex<Option<Holder>>> = Arc::new(Mutex::new(None));
		let (kept, folder, service, released) =
			(Arc::clone(&holder), path.clone(), Arc::clone(&server.service), release.clone());
		server.herdr.state().on_close_workspace = Some(Arc::new(move |_| {
			*locked(&kept) = Some(Holder::spawn(&folder, released.clone()));
			let service: Arc<JocastaService> = Arc::clone(&service);
			async move { service.refresh().await }.boxed()
		}));
		let finishing = ended(&server, &merge_args());
		let finishing = tokio::spawn(finishing);
		until(|| locked(&holder).is_some() && server.herdr.of("closeWorkspace").len() == 1).await;
		// The shell exits once the close has gone through.
		if let Some(held) = locked(&holder).as_ref() {
			held.release();
		}
		let reply = finishing.await.unwrap();
		until(|| locked(&holder).as_mut().is_none_or(|held| !held.running())).await;
		assert!(reply.text.contains("its worktree, workspace and branch are removed"), "{}", reply.text);
		assert!(!path.exists());
		assert_eq!(sh(&project.main_path, &["branch", "--list", "feature/readme-line"]), "");
		server.service.refresh().await;
		server.service.refresh().await;
		assert!(!prompts(&server).iter().any(|(_, text)| text.contains("closed")));
		assert_eq!(status(&server).await, "No threads.");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_removes_the_worktree_only_once_no_process_works_in_it_any_more_0486() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	git_config(&project);
	let release = j.root().join("release-holder");
	with_server(wait_options(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let path = started.path;
		let boss = boss_pane(&server.herdr).unwrap();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		// A process outside the pane's tree (he opened it, or it outlived an earlier pane) works in a subfolder.
		fs::create_dir(path.join("sub")).unwrap();
		let mut holder = Holder::spawn(&path.join("sub"), release);
		let finishing = tokio::spawn(ended(&server, &merge_args()));
		// The workspace closed: the folder stays for as long as the holder runs.
		until(|| server.herdr.of("closeWorkspace").len() == 1).await;
		for _ in 0..10 {
			assert!(holder.running(), "the holder ended before it was observed");
			assert!(path.exists());
			tokio::time::sleep(Duration::from_millis(50)).await;
		}
		holder.release();
		let reply = finishing.await.unwrap();
		assert!(reply.text.contains("its worktree, workspace and branch are removed"), "{}", reply.text);
		assert!(!path.exists());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_whose_notes_hunk_cant_count_says_so_instead_of_hiding_it_f_03() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let broken_dir = temp_dir("e2e-finish-broken-hunk");
	let hunk = use_hunk_daemon().await;
	let options = ServerOptions {
		hunk: Some(Some(broken_hunk(broken_dir.path()))),
		hunk_port: Some(hunk.hunk_port),
		..wait_options()
	};
	with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let boss = boss_pane(&server.herdr).unwrap();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| server.herdr.of("splitBeside").len() == 1).await;
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		let line = "[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, notes not counted, tests unverified, no report";
		until(|| prompts_to(&server, &boss).iter().any(|text| text == line)).await;
	})
	.await;
}

// ---- the feed, the start, the worklog ----

#[tokio::test(flavor = "multi_thread")]
async fn no_control_characters_reach_the_feed_a_reminder_not_yet_due_doesnt_await_him_inbox_done_takes_the_id_as_digits_n_3_n_5_f_11()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		let page = Page::open(&server, "").await;
		page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;

		let filed = call(&server, "note", &json!({ "text": "Tidy\u{1b}[2J the docs\u{7}.", "project": "demo" })).await;
		assert!(!filed.text.contains('\u{1b}'));
		page.wait_for(
			|m| matches!(m, ServerMessage::Today { today } if today.next.iter().any(|n| n.text == "Tidy[2J the docs.")),
		)
		.await;
		let reminded = call(&server, "remind", &json!({ "text": "Later", "at": "23:59" })).await;
		assert!(reminded.is_error.is_none());
		let today = server.service.day().today().unwrap();
		assert!(!today.needs_you.iter().any(|n| n.kind == crate::frontier::protocol::NeedsYouKind::Reminder));
		assert_eq!(server.service.hud(None).unwrap().awaiting, 0);
		let id = Regex::new(r"Reminder (\d+)").unwrap().captures(&reminded.text).unwrap()[1].to_owned();
		assert_eq!(call(&server, "inbox_done", &json!({ "id": id })).await.text, "Done: Later");
		// Outside text in an [event] line is one line in «» with no « or » of its own (SEC-2).
		assert_eq!(quoted("a «b»\n\tc"), "«a \"b\" c»");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_listens_and_answers_the_page_before_the_service_has_started_the_spend_reads_null_until_todays_sessions_are_read()
 {
	let _j = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	// herdr is slow to answer: the service's start waits on it (the fake's snapshot gate, where the TypeScript held
	// `ensureWorkspace`).
	let (release, gate) = tokio::sync::watch::channel(false);
	herdr.state().snapshot_gate = Some(gate);
	let options = ServerOptions {
		herdr: Some(herdr.clone()),
		day: Some(DayOptions { pollers: Some(false), ..DayOptions::default() }),
		port: Some(0),
		sessions_dir: Some(data_dir().join("omp-sessions")),
		whiteboard: Some(None),
		pane_prompt: Some(None),
		refresh_ms: Some(60_000),
		mise: Some(None),
		weekly_reviews: Some(false),
		..relay_test_options()
	};
	let server = start_server(options).await.expect("the server starts");
	let started = Arc::new(AtomicBool::new(false));
	let (waiting, flag) = (server.clone(), Arc::clone(&started));
	tokio::spawn(async move {
		let _ = waiting.ready().await;
		flag.store(true, Ordering::SeqCst);
	});
	let outcome = std::panic::AssertUnwindSafe(async {
		let page = Page::open(&server, "").await;
		let snapshot = page.wait_for(|m| matches!(m, ServerMessage::Snapshot { .. })).await;
		let ServerMessage::Snapshot { hud, .. } = snapshot else { unreachable!() };
		assert_eq!(hud.spend_today, None);
		assert!(!started.load(Ordering::SeqCst));
		release.send_replace(true);
		server.ready().await.unwrap();
		assert_eq!(server.service.hud(None).unwrap().spend_today, Some(0.0));
		let labels: Vec<String> = herdr.state().workspace_list.iter().map(|w| w.label.clone()).collect();
		assert!(labels.contains(&"Jocasta".to_owned()), "{labels:?}");
	})
	.catch_unwind()
	.await;
	release.send_replace(true);
	server.stop().await;
	if let Err(panic) = outcome {
		std::panic::resume_unwind(panic);
	}
}

#[tokio::test]
async fn yesterdays_missing_worklog_is_written_at_the_start_from_sessions_read_in_steps_the_event_loop_running_meanwhile_0302()
 {
	use chrono::{Duration as Days, Local, Timelike, Utc};
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	// A session from yesterday in the project, over 2 MB, so its read takes several steps. The day comes from the
	// service's clock, so a run across midnight can't shift it (SH-20).
	let now = Local::now().with_hour(12).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0)).unwrap();
	let fixed = now.with_nanosecond(0).unwrap();
	let yesterday = (fixed - Days::days(1)).with_hour(10).unwrap();
	let dir = data_dir().join("omp-sessions").join("--demo--");
	fs::create_dir_all(&dir).unwrap();
	let iso = |at: chrono::DateTime<Local>| at.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
	let mut lines = vec![
		json!({ "type": "session", "id": "y", "cwd": path_text(&project.main_path), "timestamp": iso(yesterday) })
			.to_string(),
	];
	for i in 0..1500 {
		let at = yesterday.timestamp_millis() + i * 1000;
		lines.push(
			json!({
				"type": "message", "id": format!("m{i}"),
				"message": { "role": "assistant", "timestamp": at,
					"content": [{ "type": "text", "text": format!("{i} {}", "x".repeat(1500)) }] },
			})
			.to_string(),
		);
	}
	fs::write(dir.join("yesterday.jsonl"), format!("{}\n", lines.join("\n"))).unwrap();
	let day = yesterday.format("%Y-%m-%d").to_string();
	let ticks = Arc::new(AtomicUsize::new(0));
	let counter = Arc::clone(&ticks);
	let ticker = tokio::spawn(async move {
		loop {
			counter.fetch_add(1, Ordering::SeqCst);
			tokio::task::yield_now().await;
		}
	});
	let clock_ms = fixed.timestamp_millis();
	let day_options =
		DayOptions { pollers: Some(false), now: Some(Arc::new(move || clock_ms)), ..DayOptions::default() };
	with_server(ServerOptions { day: Some(day_options), ..ServerOptions::default() }, |server| async move {
		// The window the test can see starts here: under load the write may already be over by the time the server is
		// up, and then there is no "meanwhile" left to watch (the steps' yields are `advance_in_steps`' own test). While
		// it is still being written, the loop has to keep turning until it ends.
		let before = ticks.load(Ordering::SeqCst);
		let pending = futures_util::FutureExt::now_or_never(server.service.day().worklog_writing()).is_none();
		server.service.day().worklog_writing().await;
		if pending {
			assert!(ticks.load(Ordering::SeqCst) > before);
		}
		let written = fs::read_to_string(worklog_path(&day)).unwrap();
		assert!(written.contains("demo"), "{written:?}");
	})
	.await;
	ticker.abort();
}
