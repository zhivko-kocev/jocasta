//! Tests of `review_flow.rs`, on `context::test_context` with a real git repo and worktree, the fake hunk
//! (`fake_hunk.rs`) and a stand-in mise (`stand_in_mise`, the Rust form of `fake-mise-verify.ts`, 0630).
//!
//! Rewritten at part level (`ReviewFlow`'s methods, no HTTP server) from `service-plans.test.ts` (B4, P3),
//! `service-regressions.test.ts` (SEC-NET-6's `check`), `service-tools.test.ts` (the restored panes' catch-up),
//! `service-keys.test.ts` (the check tool, the check on idle, the review's done-when and mise skip lines),
//! `pane-check.test.ts` and `hunk.test.ts` ("a review fails on each way a proof weakens the tests").
//!
//! Waits for the service (a later wave ports each verbatim on the real service):
//! - `service-plans.test.ts`: "B4: the hunk session daemon the service started ends with Stop everything, also when
//!   Keep came first" (the stop; the review's start of the daemon is the pane launcher's), "a done and reviewed thread
//!   survives Stop everything and reopen", "P6, B6: a reviewed thread counts in the close question as on the HUD".
//! - `service-regressions.test.ts`: "SVC-6: an omp started again mid-review runs the review again, and mid-/handoff
//!   runs /handoff again", "C6: with the layer off, a thread pane gets JOCASTA_LAYER=off, … no report steer"
//!   (the steer is `thread_lifecycle`'s).
//! - `service-tools.test.ts`: "herdr restored its panes as shells: …" (the relaunch; its review is
//!   `catch_up_reviews_a_commit_found_in_herdr...` here).
//! - `pane-check.test.ts`: "jocasta pane-check prints the service's text for the pane's key, …" (the CLI).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::frontier::server::checks::{CheckRunner, CheckRunnerOptions};
use crate::frontier::server::context::{BossRef, Shared, TestContext, test_context};
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::herdr::PaneInfo;
use crate::frontier::server::hunk::fake_hunk::{fake_hunk, health_daemon};
use crate::frontier::server::mise::tests::{captured_shim, stand_in};
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::task_file::TaskRecord;
use crate::frontier::server::wording::{HANDOFF_PROMPT, IDLE_CHECK_FRAME};
use crate::shared::env::overlay;
use crate::shared::layer::LAYER_ENV;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::testing::{TempDir, temp_dir};

const BRIEF: &str = "Add a README line.";
const RANGE: &str = "main...feature/readme-line";
const STAND_IN: &str = "frontier::server::review_flow::tests::stand_in_mise";

fn git(dir: &Path, args: &[&str]) -> String {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	crate::shared::env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
	String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Writes the README and commits it.
fn commit_in(path: &Path, text: &str, message: &str) {
	std::fs::write(path.join("README.md"), text).unwrap();
	git(path, &["commit", "-qam", message]);
}

/// A stand-in mise whose project check is a `verify` task (`fake-mise-verify.ts`): `run -c verify` counts itself in
/// `ctl/runs`, waits for as long as `ctl/hold` exists (at most a minute), then exits with `ctl/exit` (read when it
/// starts; 0 when absent); a
/// run that starts while another is under way adds a line to `ctl/overlaps`. `ctl/paranoid`, `ctl/trust` and
/// `ctl/tasks` change its answers to `settings get paranoid`, `trust --show` and `tasks ls --json`. The control folder
/// is `ctl` beside the shim. Does nothing in a normal test run.
#[test]
fn stand_in_mise() {
	stand_in(|args| {
		let ctl = crate::shared::env::var("FAKE_OUT")
			.and_then(|prefix| Path::new(&prefix).parent().map(|dir| dir.join("ctl")))
			.unwrap_or_default();
		let read = |name: &str| std::fs::read_to_string(ctl.join(name)).ok();
		let number = |name: &str| read(name).and_then(|text| text.trim().parse::<u64>().ok()).unwrap_or(0);
		let append = |name: &str, line: &str| {
			use std::io::Write;
			let mut file =
				std::fs::OpenOptions::new().create(true).append(true).open(ctl.join(name)).expect("the control file");
			file.write_all(line.as_bytes()).expect("writing the control file");
		};
		if args == "run -c verify" {
			let exit = number("exit");
			let active = ctl.join("active");
			if active.exists() {
				append("overlaps", "overlap\n");
			}
			std::fs::write(&active, "").expect("the active mark");
			append("runs", "run\n");
			for _ in 0..3_000 {
				if !ctl.join("hold").exists() {
					break;
				}
				std::thread::sleep(Duration::from_millis(20));
			}
			let _ = std::fs::remove_file(&active);
			return (String::new(), "verify: boom\n".to_owned(), i32::try_from(exit).unwrap_or(1));
		}
		let answer = match args.as_str() {
			"settings get paranoid" => read("paranoid").unwrap_or_else(|| "true\n".to_owned()),
			"trust --show" => read("trust").unwrap_or_default(),
			"tasks ls --json" => read("tasks").unwrap_or_else(|| r#"[{"name":"verify"}]"#.to_owned()),
			"env --json" => "{}".to_owned(),
			other if other.starts_with("trust ") && !other.starts_with("trust --") => String::new(),
			other => return (String::new(), format!("fake mise: no answer for {other}\n"), 1),
		};
		(answer, String::new(), 0)
	});
}

/// Puts a test environment variable back when dropped.
struct EnvGuard {
	name: &'static str,
	before: Option<Option<String>>,
}

impl Drop for EnvGuard {
	fn drop(&mut self) {
		overlay::restore(self.name, self.before.take());
	}
}

/// What a test works with: the demo project in `<dir>/code/demo` with its thread `Readme line` in a worktree, a boss
/// pane and a project pane, over a [`TestContext`] with the fake hunk and no mise (`stand_in_mise` when asked).
struct World {
	/// Declared first: dropped before the context releases the environment.
	_layer: Option<EnvGuard>,
	t: TestContext,
	dir: TempDir,
	main: PathBuf,
	wt: PathBuf,
	thread: ThreadRef,
	boss: BossRef,
	hunk: Vec<String>,
	port: u16,
}

/// How a [`World`] differs from the default.
#[derive(Default)]
struct Setup {
	/// The stand-in mise answers the project's check.
	mise: bool,
	/// `JOCASTA_LAYER=off`.
	layer_off: bool,
	/// The pane check's wait, ms.
	wait_ms: Option<i64>,
	done_when: Vec<String>,
}

impl World {
	async fn new() -> Self {
		Self::with(Setup::default()).await
	}

	async fn with(setup: Setup) -> Self {
		let dir = temp_dir("review-flow");
		let code = real_path(dir.path()).join("code");
		let main = code.join("demo");
		std::fs::create_dir_all(&main).unwrap();
		git(&main, &["init", "-q", "-b", "main"]);
		git(&main, &["config", "user.name", "Test"]);
		git(&main, &["config", "user.email", "test@example.com"]);
		std::fs::write(main.join("README.md"), "demo\n").unwrap();
		git(&main, &["add", "."]);
		git(&main, &["commit", "-q", "-m", "init repo"]);
		let wt = code.join("demo.wt").join("readme-line");
		git(&main, &["worktree", "add", "-q", "-b", "feature/readme-line", &wt.to_string_lossy()]);
		let hunk = fake_hunk(dir.path());
		let port = health_daemon().await;
		let mise_argv = vec![captured_shim(dir.path(), "mise", STAND_IN).to_string_lossy().into_owned()];
		std::fs::create_dir_all(dir.path().join("ctl")).unwrap();
		let mut layer = None;
		let t = test_context(|options: &mut ServiceOptions| {
			options.hunk = Some(Some(hunk.clone()));
			options.hunk_port = Some(port);
			options.mise = Some(setup.mise.then(|| mise_argv.clone()));
			options.pane_check_wait_ms = setup.wait_ms;
			options.checks = Some(Arc::new(CheckRunner::new(CheckRunnerOptions {
				dir: dir.path().join("checks"),
				versions: Some(Arc::new(|_, _| futures_util::FutureExt::boxed(async { String::new() }))),
				..CheckRunnerOptions::default()
			})));
			if setup.layer_off {
				let before = overlay::set(LAYER_ENV, Some("off".to_owned()));
				layer = Some(EnvGuard { name: LAYER_ENV, before });
			}
		});
		std::fs::create_dir_all(projects_file().parent().unwrap()).unwrap();
		let roots = format!(
			"version: 1\nroots:\n  - path: {}\n    depth: 2\n    ignore: [\"*.wt\"]\n",
			code.display().to_string().replace('\\', "/")
		);
		std::fs::write(projects_file(), roots).unwrap();
		t.ctx.places.sync().await;
		let project = t.ctx.places.project_at(&main).unwrap().expect("the demo project");
		let mut thread = Thread::new("t-1", "w-1", project, "Readme line", "feature/readme-line");
		thread.path = wt.clone();
		thread.state = AgentState::Idle;
		thread.task = Some(TaskRecord { brief: BRIEF.to_owned(), done_when: setup.done_when, ..TaskRecord::default() });
		let thread = Shared::new(thread);
		t.ctx.threads.add(thread.clone());
		t.herdr.state().pane_list.push(PaneInfo {
			pane_id: "t-1".to_owned(),
			tab_id: "tab-1".to_owned(),
			workspace_id: "w-1".to_owned(),
			cwd: Some(wt.clone()),
			agent_state: AgentState::Idle,
			..PaneInfo::default()
		});
		t.ctx.launched.add("t-1", &wt.to_string_lossy(), Some("thread-key")).unwrap();
		t.ctx.launched.add("p-main", &main.to_string_lossy(), Some("project-key")).unwrap();
		t.ctx.launched.add("boss-1", &dir.path().to_string_lossy(), Some("boss-key")).unwrap();
		let boss = t.with_boss(AgentState::Idle);
		Self { _layer: layer, t, dir, main, wt, thread, boss, hunk, port }
	}

	fn flow(&self) -> &ReviewFlow {
		&self.t.ctx.review
	}

	fn thread(&self) -> Thread {
		self.thread.get()
	}

	/// What `pane` was typed, in order.
	fn typed(&self, pane: &str) -> Vec<String> {
		let prompts = self.t.herdr.of("prompt");
		let to_pane = prompts.iter().filter(|args| args[0] == pane);
		to_pane.map(|args| args[1].as_str().unwrap().to_owned()).collect()
	}

	/// The `[event]` lines the boss was told, in order; the boss is idle again after each.
	fn boss_lines(&self) -> Vec<String> {
		self.typed("boss-1")
	}

	/// Lets the next `[event]` line through to the boss.
	fn boss_idle(&self) {
		self.boss.with(|boss| boss.state = AgentState::Idle);
	}

	/// The review turn as the thread is asked for it.
	fn review_text(&self, brief: Option<&str>) -> String {
		let context = ReviewContext { brief: brief.map(str::to_owned), report: Some(None), ..ReviewContext::default() };
		review_prompt(&self.hunk, RANGE, &context)
	}

	/// The control folder of the stand-in mise.
	fn control(&self, name: &str, value: impl std::fmt::Display) {
		std::fs::write(self.dir.path().join("ctl").join(name), value.to_string()).unwrap();
	}

	/// Makes the stand-in's next runs wait, from their start, until [`World::release`].
	fn hold(&self) {
		self.control("hold", "");
	}

	/// Lets every held run go on.
	fn release(&self) {
		let _ = std::fs::remove_file(self.dir.path().join("ctl").join("hold"));
	}

	/// How many lines a control file has (0 when absent).
	fn lines(&self, name: &str) -> usize {
		std::fs::read_to_string(self.dir.path().join("ctl").join(name)).map_or(0, |text| text.trim().lines().count())
	}

	/// Today's activity lines.
	fn today(&self) -> Vec<String> {
		self.t.ctx.day.dashboard().activity.events.into_iter().map(|event| event.text).collect()
	}

	fn head(&self) -> String {
		git(&self.wt, &["rev-parse", "HEAD"])
	}

	/// A session file of the thread with `text` as its last message and the bash `runs` that all ended ok, written as
	/// the review turn would leave it (a moment after `now`, so it counts for the review asked for before).
	fn session(&self, runs: &[&str], text: &str) {
		let at = chrono::Utc::now().timestamp_millis() + 60_000;
		let mut lines = Vec::new();
		for (i, command) in runs.iter().enumerate() {
			let id = format!("c{i}");
			let call = json!({"type": "message", "message": {"role": "assistant", "timestamp": at,
				"content": [{"type": "toolCall", "id": id, "name": "bash", "arguments": {"command": command}}]}});
			let result = json!({"type": "message", "message": {"role": "toolResult", "toolCallId": id,
				"toolName": "bash", "timestamp": at + 1, "content": [{"type": "text", "text": "output"}]}});
			lines.extend([call.to_string(), result.to_string()]);
		}
		let said = json!({"type": "message", "message": {"role": "assistant", "timestamp": at + 2,
			"content": [{"type": "text", "text": text}]}});
		lines.push(said.to_string());
		let file = self.dir.path().join("session.jsonl");
		std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
		self.thread.with(|t| t.session_path = Some(file));
	}

	/// The first task: a commit, then its review asked for.
	async fn first_task(&self) {
		commit_in(&self.wt, "demo\nA line.\n", "Add a README line");
		let said = self.flow().commits_said(&self.thread, None, &self.head()).await.unwrap();
		self.flow().task_ended(&self.thread, &said.said, said.subject.as_deref()).await.unwrap();
	}
}

fn proof(changes: impl FnOnce(&mut ReviewProof)) -> ReviewProof {
	let mut proof = ReviewProof {
		added: 1,
		deleted: 1,
		files: 1,
		test_files: vec!["a.test.ts".to_owned()],
		..ReviewProof::default()
	};
	changes(&mut proof);
	proof
}

fn names(files: &[&str]) -> Option<Vec<String>> {
	Some(files.iter().map(|file| (*file).to_owned()).collect())
}

#[tokio::test]
async fn a_review_fails_on_each_way_a_proof_weakens_the_tests_0610() {
	let t = test_context(|_| {});
	let failed = |proof: Option<ReviewProof>| {
		let mut thread = t.with_thread("1-1", "Readme line").get();
		thread.task = Some(TaskRecord::default());
		let result = ReviewResult {
			notes: Some(0),
			blocking: Some(0),
			tests: TestsResult::Pass,
			proof,
			checks: Vec::new(),
			at: None,
		};
		thread.review = Some(ThreadReview { pane: None, running: None, result: Some(result) });
		t.ctx.review.failed(&thread)
	};
	assert_eq!(failed(Some(proof(|_| {}))), None);
	assert_eq!(failed(None), None);
	// A proof stored before 0610 has neither new field.
	assert_eq!(failed(Some(proof(|p| p.removed_tests = None))), None);
	assert_eq!(
		failed(Some(proof(|p| p.removed_tests = names(&["a.test.ts"])))).as_deref(),
		Some("removed tests in a.test.ts")
	);
	assert_eq!(
		failed(Some(proof(|p| p.fewer_assertions = names(&["a.test.ts"])))).as_deref(),
		Some("fewer assertions in a.test.ts")
	);
	assert_eq!(
		failed(Some(proof(|p| p.skipped = vec!["a.test.ts".to_owned()]))).as_deref(),
		Some("added skip/only in a.test.ts")
	);
}

#[tokio::test]
async fn a_review_that_isnt_a_pass_says_why_in_order_and_one_without_a_result_says_nothing() {
	let t = test_context(|_| {});
	let mut thread = t.with_thread("1-1", "Readme line").get();
	assert_eq!(t.ctx.review.failed(&thread), None);
	thread.task =
		Some(TaskRecord { done_when: vec!["bun lint".to_owned(), "bun fmt".to_owned()], ..TaskRecord::default() });
	let checks = vec![
		CheckExit { command: "bun test".to_owned(), exit: Some(1) },
		CheckExit { command: "bun lint".to_owned(), exit: Some(2) },
		CheckExit { command: "bun fmt".to_owned(), exit: None },
	];
	let result =
		ReviewResult { notes: None, blocking: Some(2), tests: TestsResult::Fail, proof: None, checks, at: None };
	thread.review = Some(ThreadReview { pane: None, running: None, result: Some(result.clone()) });
	assert_eq!(
		t.ctx.review.failed(&thread).as_deref(),
		Some(
			"tests fail, notes not counted, 2 blocking notes, done-when `bun lint` exit 2, done-when `bun fmt` not run"
		)
	);
	let one =
		ReviewResult { notes: Some(1), blocking: Some(1), tests: TestsResult::None, checks: Vec::new(), ..result };
	thread.review = Some(ThreadReview { pane: None, running: None, result: Some(one) });
	assert_eq!(t.ctx.review.failed(&thread).as_deref(), Some("1 blocking note"));
}

#[tokio::test]
async fn a_report_is_in_once_it_came_after_the_last_review_0539() {
	let t = test_context(|_| {});
	let mut thread = t.with_thread("1-1", "Readme line").get();
	assert!(!t.ctx.review.report_in(&thread));
	let value = crate::frontier::server::report::parse_report(&json!({
		"outcome": "done", "summary": "Done", "files": [], "tests": [], "decisions": [], "questions": [], "followUps": []
	}))
	.unwrap();
	let report = crate::frontier::server::task_file::TaskReport { at: 100, value };
	let mut task = TaskRecord { report: Some(report), ..TaskRecord::default() };
	thread.task = Some(task.clone());
	assert!(t.ctx.review.report_in(&thread));
	task.reviews.push(TaskReview {
		at: 100,
		passed: true,
		checks: Vec::new(),
		said: String::new(),
		notes: None,
		blocking: None,
		tests: None,
		proof: None,
	});
	thread.task = Some(task.clone());
	assert!(!t.ctx.review.report_in(&thread), "a review at the report's time covers it");
	task.reviews[0].at = 99;
	thread.task = Some(task);
	assert!(t.ctx.review.report_in(&thread));
}

#[tokio::test]
async fn b4_a_task_end_opens_hunk_beside_its_pane_once_and_a_later_review_reuses_the_pane() {
	let w = World::new().await;
	w.first_task().await;
	let splits = w.t.herdr.of("splitBeside");
	assert_eq!(splits.len(), 1);
	let mut command = w.hunk.clone();
	command.extend(["diff", "--theme", "tokyo-night", RANGE, "--watch"].map(str::to_owned));
	assert_eq!(splits[0][0], json!("t-1"));
	assert_eq!(splits[0][1]["command"], json!(command));
	assert_eq!(splits[0][1]["cwd"], json!(w.wt.to_string_lossy()));
	assert_eq!(splits[0][1]["label"], json!("review"));
	assert_eq!(splits[0][1]["env"]["HUNK_MCP_PORT"], json!(w.port.to_string()));
	// The thread is asked for the review, with its brief; the phase and what is running are kept.
	assert_eq!(w.typed("t-1"), [w.review_text(Some(BRIEF))]);
	let thread = w.thread();
	assert_eq!(thread.phase, Phase::Review);
	let review = thread.review.expect("a review step");
	let running = review.running.expect("a review running");
	assert_eq!((running.range.as_str(), running.commits.as_str()), (RANGE, "1 new commit «Add a README line»"));
	assert_eq!(running.head.as_deref(), Some(w.head().as_str()));
	let pane = review.pane.expect("the hunk pane");
	assert!(w.t.herdr.state().pane_list.iter().any(|p| p.pane_id == pane));
	// Another task: the hunk pane is still open, so it is reused (`--watch` shows the new commits in it).
	w.thread.with(|t| set_phase(t, Phase::Handoff, "test"));
	w.flow().task_ended(&w.thread, "1 new commit", None).await.unwrap();
	assert_eq!(w.t.herdr.of("splitBeside").len(), 1);
	assert_eq!(w.typed("t-1"), [w.review_text(Some(BRIEF)), w.review_text(Some(BRIEF))]);
	assert_eq!(w.thread().review.and_then(|review| review.pane), Some(pane));
}

#[tokio::test]
async fn a_task_end_without_a_branch_is_announced_done_counts_as_reviewed_and_goes_to_its_handoff_0400() {
	let w = World::new().await;
	commit_in(&w.wt, "demo\nA line.\n", "Add a README line");
	w.thread.with(|t| t.branch = String::new());
	w.flow().task_ended(&w.thread, "1 new commit", Some("Add a README line")).await.unwrap();
	assert!(w.t.herdr.of("splitBeside").is_empty());
	assert_eq!(w.boss_lines(), ["[event] demo/Readme line — done: 1 new commit"]);
	assert_eq!(w.t.ctx.reviewed.of(&w.wt), Some(w.head()));
	assert_eq!(w.typed("t-1"), [HANDOFF_PROMPT]);
	let thread = w.thread();
	assert_eq!(thread.phase, Phase::Handoff);
	assert_eq!(thread.head.as_deref(), Some(w.head().as_str()));
	assert!(thread.handoff_at.is_some());
	let toast = w.t.sent.lock().unwrap().iter().find_map(|message| match message {
		ServerMessage::Toast { text, pane_id, reason: ToastReason::Done, .. } => Some((text.clone(), pane_id.clone())),
		_ => None,
	});
	assert_eq!(toast, Some(("demo/Readme line — done: 1 new commit".to_owned(), Some("t-1".to_owned()))));
}

#[tokio::test]
async fn a_review_line_names_the_commits_the_diff_the_notes_and_the_tests_and_is_said_once_with_its_toast() {
	let w = World::new().await;
	w.first_task().await;
	w.session(&[], "Reviewed.\n\n**TESTS: pass — bun test**");
	w.flow().ready(&w.thread).await.unwrap();
	let line = "[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests unverified, no report";
	assert_eq!(w.boss_lines(), [line]);
	// The named test command never ran in the session's bash calls: its `pass` is unverified (0453).
	let thread = w.thread();
	assert_eq!(thread.shown, ThreadState::Done);
	assert_eq!(thread.phase, Phase::Handoff);
	assert_eq!(w.typed("t-1").last().map(String::as_str), Some(HANDOFF_PROMPT));
	assert_eq!(w.t.ctx.reviewed.of(&w.wt), Some(w.head()));
	let result = thread.review.and_then(|review| review.result).expect("the result");
	assert_eq!((result.notes, result.blocking, result.tests), (Some(2), Some(0), TestsResult::Unknown));
	let review = &thread.task.expect("the task").reviews[0];
	assert!(!review.passed);
	assert_eq!(review.said, "2 notes, tests unverified, no report");
	let toasts: Vec<String> =
		w.t.sent
			.lock()
			.unwrap()
			.iter()
			.filter_map(|message| match message {
				ServerMessage::Toast { text, reason: ToastReason::Done, .. } => Some(text.clone()),
				_ => None,
			})
			.collect();
	assert_eq!(toasts, [line.replace("[event] ", "").replace(['«', '»'], "")]);
}

#[tokio::test]
async fn the_review_runs_the_done_when_first_gives_the_turn_its_failing_gate_and_says_it_beside_the_verdict_0542() {
	let failing = "git rev-parse --verify no-such-ref";
	let w = World::with(Setup { done_when: vec![failing.to_owned()], ..Setup::default() }).await;
	w.first_task().await;
	// The runner checked the task's tree first: the turn gets the failing gate, and the brief to check against.
	let asked = w.typed("t-1").remove(0);
	assert!(asked.contains("they failed; give the reviewer these gates too:\n\nchecks: FAIL (tree "), "{asked}");
	assert!(asked.contains(&format!("  FAIL  done_when  {failing}\n")));
	assert!(asked.contains(&format!("        rerun: {failing}")));
	// No project check ran, so the turn runs the project's tests itself.
	assert!(asked.contains("the way its AGENTS.md or package scripts say"));
	assert!(asked.contains("his brief «Add a README line.», to check the diff against what he asked for"));
	assert!(asked.contains("the fact that the thread handed in no report"));
	// The review turn: its test command passes, the done-when's result is the runner's stored one.
	w.session(&["bun run check"], "Reviewed.\nTESTS: pass — `bun run check`");
	w.flow().ready(&w.thread).await.unwrap();
	assert_eq!(
		w.boss_lines(),
		[format!(
			"[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests pass, done-when `{failing}` exit 128, no report"
		)]
	);
	let thread = w.thread();
	let review = &thread.task.expect("the task").reviews[0];
	assert!(!review.passed);
	assert_eq!(review.said, "2 notes, tests pass, no report");
	assert_eq!(
		review.checks,
		[
			CheckExit { command: "bun run check".to_owned(), exit: Some(0) },
			CheckExit { command: failing.to_owned(), exit: Some(128) },
		]
	);
}

#[tokio::test]
async fn a_review_a_thread_finish_asked_for_after_main_moved_is_said_once_with_the_merges_outcome_0454() {
	let w = World::new().await;
	w.first_task().await;
	w.thread.with(|t| t.merge_after_review = true);
	w.flow().ready(&w.thread).await.unwrap();
	// It didn't pass (tests unverified): one line with why the merge wasn't made, then the handoff.
	let said = "done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests unverified, no report";
	assert_eq!(
		w.boss_lines(),
		[format!(
			"[event] demo/Readme line — {said}; not merged: its new review after main moved didn't pass (tests unverified)"
		)]
	);
	assert!(!w.thread().merge_after_review);
	assert_eq!(w.typed("t-1").last().map(String::as_str), Some(HANDOFF_PROMPT));
	// One that passed says nothing of its own here: the merge goes on in `Finisher::after_review` (its outcome is the second line), not `/handoff`.
	w.thread.with(|t| {
		t.merge_after_review = true;
		set_phase(t, Phase::Review, "test");
	});
	w.boss_idle();
	let before = w.typed("t-1").len();
	w.flow().task_ended(&w.thread, "1 new commit", None).await.unwrap();
	w.session(&["bun test"], "Reviewed.\nTESTS: pass — bun test");
	w.flow().ready(&w.thread).await.unwrap();
	let lines = w.boss_lines();
	assert_eq!(lines.len(), 2, "the merge's outcome is the one new line: {lines:?}");
	let passed = "[event] demo/Readme line — its new review passed (1 new commit, +1/−0 in 1 file, no test files changed, 2 notes, tests pass, no report); merged demo/Readme line into main as ";
	assert!(lines[1].starts_with(passed), "{}", lines[1]);
	assert!(
		lines[1].ends_with(" (1 commit squashed); its worktree, workspace and branch are removed."),
		"{}",
		lines[1]
	);
	assert_eq!(w.typed("t-1").len(), before + 1, "only the review turn was typed: no /handoff");
	assert!(!w.thread().merge_after_review);
}

#[tokio::test]
async fn p3_a_tasks_review_line_counts_the_threads_own_commits_not_mains_it_merged_in() {
	let w = World::new().await;
	w.first_task().await;
	w.flow().ready(&w.thread).await.unwrap();
	let first = w.head();
	assert_eq!(w.t.ctx.reviewed.of(&w.wt), Some(first.clone()));
	// main moves; the thread merges it in, then commits its own work.
	std::fs::write(w.main.join("other.txt"), "other\n").unwrap();
	git(&w.main, &["add", "other.txt"]);
	git(&w.main, &["commit", "-qm", "Other work"]);
	git(&w.wt, &["merge", "--no-edit", "main"]);
	commit_in(&w.wt, "demo\nA line.\nOwn.\n", "Own line");
	let said = w.flow().commits_said(&w.thread, Some(&first), &w.head()).await.unwrap();
	assert_eq!(said, CommitsSaid { said: "1 new commit".to_owned(), subject: Some("Own line".to_owned()) });
	w.thread.with(|t| set_phase(t, Phase::Review, "test"));
	w.flow().task_ended(&w.thread, &said.said, said.subject.as_deref()).await.unwrap();
	assert_eq!(w.typed("t-1").iter().filter(|text| **text == w.review_text(Some(BRIEF))).count(), 2);
	w.boss_idle();
	w.flow().ready(&w.thread).await.unwrap();
	let lines = w.boss_lines();
	assert!(lines[1].starts_with("[event] demo/Readme line — done and reviewed: 1 new commit «Own line»"), "{lines:?}");
}

#[tokio::test]
async fn a_task_that_only_brought_main_in_says_main_merged_in_and_commits_since_trunk_say_new_commits() {
	let w = World::new().await;
	let head = w.head();
	let said = w.flow().commits_said(&w.thread, None, &head).await.unwrap();
	assert_eq!(said, CommitsSaid { said: "new commits".to_owned(), subject: None });
	std::fs::write(w.main.join("other.txt"), "other\n").unwrap();
	git(&w.main, &["add", "other.txt"]);
	git(&w.main, &["commit", "-qm", "Other work"]);
	git(&w.wt, &["merge", "--no-edit", "main"]);
	let said = w.flow().commits_said(&w.thread, Some(&head), &w.head()).await.unwrap();
	assert_eq!(said, CommitsSaid { said: "main merged in".to_owned(), subject: None });
	commit_in(&w.wt, "demo\nOne.\n", "One");
	commit_in(&w.wt, "demo\nOne.\nTwo.\n", "Two");
	let said = w.flow().commits_said(&w.thread, None, &w.head()).await.unwrap();
	assert_eq!(said, CommitsSaid { said: "2 new commits".to_owned(), subject: Some("Two".to_owned()) });
}

#[tokio::test]
async fn catch_up_reviews_a_commit_found_in_herdr_that_no_review_covered_and_only_once_0394() {
	let w = World::new().await;
	// Nothing ahead of trunk: nothing to review.
	w.flow().catch_up(&w.thread).await.unwrap();
	assert!(w.typed("t-1").is_empty());
	commit_in(&w.wt, "demo\nA line.\n", "Add a README line");
	w.flow().catch_up(&w.thread).await.unwrap();
	// Its unreviewed commit gets its review, said as the commits since trunk, with the thread shown done.
	assert_eq!(w.typed("t-1"), [w.review_text(Some(BRIEF))]);
	let thread = w.thread();
	assert_eq!(thread.shown, ThreadState::Done);
	assert_eq!(thread.head.as_deref(), Some(w.head().as_str()));
	assert_eq!(
		thread.review.and_then(|review| review.running).map(|running| running.commits).as_deref(),
		Some("1 new commit «Add a README line»")
	);
	// The review turn ends: /handoff, and the commit counts as reviewed, so a second start leaves it alone.
	w.flow().ready(&w.thread).await.unwrap();
	assert_eq!(w.typed("t-1").last().map(String::as_str), Some(HANDOFF_PROMPT));
	let typed = w.typed("t-1").len();
	w.flow().catch_up(&w.thread).await.unwrap();
	assert_eq!(w.typed("t-1").len(), typed);
}

#[tokio::test]
async fn handoff_asks_for_it_and_notes_head_first_so_the_reviews_commits_arent_the_next_tasks_0400() {
	let w = World::new().await;
	w.thread.with(|t| set_phase(t, Phase::Review, "test"));
	commit_in(&w.wt, "demo\nA line.\n", "Add a README line");
	w.flow().handoff(&w.thread).await.unwrap();
	assert_eq!(w.typed("t-1"), [HANDOFF_PROMPT]);
	let thread = w.thread();
	assert_eq!(thread.phase, Phase::Handoff);
	assert_eq!(thread.head.as_deref(), Some(w.head().as_str()));
	assert!(thread.handoff_at.is_some_and(|at| at > 0));
}

#[tokio::test]
async fn sec_net_6_the_check_tool_answers_only_a_threads_pane_and_says_what_it_ran_0542() {
	let w = World::with(Setup { done_when: vec!["git diff --quiet".to_owned()], ..Setup::default() }).await;
	let refused =
		|text: &str| format!("check: only a thread's pane runs its checks, and this pane isn't a thread's{text}.");
	for (pane, unkeyed) in [(None, ""), (Some("no-such-pane"), "; asked from outside")] {
		let error = w.flow().check_tool(pane, unkeyed).await.unwrap_err();
		assert!(error.is_user());
		assert_eq!(error.to_string(), refused(unkeyed));
	}
	// A clean tree passes in one line.
	let said = w.flow().check_tool(Some("t-1"), "").await.unwrap();
	let tree = said.strip_prefix("checks: PASS (tree ").and_then(|rest| rest.strip_suffix(')')).expect(&said);
	assert!(tree.len() == 6 && tree.chars().all(|c| c.is_ascii_hexdigit()), "{said}");
	// A failing one answers the gates, again from the stored result.
	std::fs::write(w.wt.join("README.md"), "demo\nchanged\n").unwrap();
	let gates = w.flow().check_tool(Some("t-1"), "").await.unwrap();
	assert!(gates.starts_with("checks: FAIL (tree "), "{gates}");
	assert!(gates.contains("  FAIL  done_when  git diff --quiet\n"));
	assert_eq!(w.flow().check_tool(Some("t-1"), "").await.unwrap(), gates);
	// A task with no checks says so.
	w.thread.with(|t| t.task = Some(TaskRecord::default()));
	assert_eq!(
		w.flow().check_tool(Some("t-1"), "").await.unwrap(),
		"No checks: the project has no trusted mise verify or test task, and the task has no done_when."
	);
}

#[tokio::test]
async fn the_check_tool_and_the_review_say_when_the_projects_mise_check_didnt_run_never_a_silent_pass_0559() {
	let w = World::with(Setup { mise: true, done_when: vec!["git --version".to_owned()], ..Setup::default() }).await;
	// mise lists the project's config, but its paranoid trust is off: Jocasta may not load it.
	w.control("paranoid", "false\n");
	w.control("trust", format!("{}\\mise.toml: trusted\n", w.main.display()));
	let skipped = "the project's check didn't run: mise's paranoid config trust is off";
	let said = w.flow().check_tool(Some("t-1"), "").await.unwrap();
	let (head, note) = said.split_once('\n').expect(&said);
	assert!(head.starts_with("checks: PASS (tree "), "{said}");
	assert_eq!(note, format!("Note: {skipped}."));
	w.thread.with(|t| t.task = Some(TaskRecord::default()));
	assert_eq!(
		w.flow().check_tool(Some("t-1"), "").await.unwrap(),
		format!("No checks: {skipped}, and the task has no done_when.")
	);
	w.thread
		.with(|t| t.task = Some(TaskRecord { done_when: vec!["git --version".to_owned()], ..TaskRecord::default() }));
	w.first_task().await;
	w.session(&["bun test"], "Reviewed.\nTESTS: pass — `bun test`");
	w.flow().ready(&w.thread).await.unwrap();
	let line = w.boss_lines().pop().expect("the review line");
	assert!(line.contains(&format!(", {skipped}, no report")), "{line}");
	let thread = w.thread();
	assert!(
		thread.task.expect("the task").reviews[0].said.contains(&format!("2 notes, tests pass, {skipped}, no report"))
	);
}

#[tokio::test]
async fn a_turn_ending_with_failing_edits_gets_the_gates_once_under_the_frame_and_a_pass_says_nothing_0513_0542() {
	let clean = "git diff --quiet";
	let w = World::with(Setup { done_when: vec![clean.to_owned()], ..Setup::default() }).await;
	std::fs::write(w.wt.join("README.md"), "demo\nchanged\n").unwrap();
	w.flow().start_idle_check(&w.thread);
	w.flow().settled().await;
	let typed = w.typed("t-1");
	assert_eq!(typed.len(), 1);
	assert!(typed[0].starts_with(&format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL")), "{}", typed[0]);
	assert!(typed[0].contains(&format!("  FAIL  done_when  {clean}\n")));
	assert!(typed[0].contains(&format!("        rerun: {clean}")));
	// The check tool answers the same tree from the stored result, without the frame.
	assert_eq!(w.flow().check_tool(Some("t-1"), "").await.unwrap(), typed[0][IDLE_CHECK_FRAME.len() + 2..]);
	// The same tree is never checked or answered twice.
	w.flow().start_idle_check(&w.thread);
	w.flow().settled().await;
	assert_eq!(w.typed("t-1").len(), 1);
	// A tree with no edits since the last commit costs nothing; a pass says nothing.
	commit_in(&w.wt, "demo\nchanged\n", "Change it");
	w.flow().start_idle_check(&w.thread);
	w.flow().settled().await;
	assert_eq!(w.typed("t-1").len(), 1);
	assert_eq!(w.thread().idle_tree.as_deref().map(str::len), Some(40));
}

#[tokio::test]
async fn the_check_on_idle_leaves_a_thread_alone_that_is_working_waits_on_a_plan_or_left_the_task_phase() {
	let clean = "git diff --quiet";
	let w = World::with(Setup { done_when: vec![clean.to_owned()], ..Setup::default() }).await;
	std::fs::write(w.wt.join("README.md"), "demo\nchanged\n").unwrap();
	let idle = async || {
		w.flow().start_idle_check(&w.thread);
		w.flow().settled().await;
		w.typed("t-1").len()
	};
	w.thread.with(|t| {
		t.plan = Some(crate::frontier::server::thread::ThreadPlan {
			awaiting: Some("A plan".to_owned()),
			..Default::default()
		})
	});
	assert_eq!(idle().await, 0);
	w.thread.with(|t| t.plan = None);
	w.thread.with(|t| set_phase(t, Phase::Review, "test"));
	assert_eq!(idle().await, 0);
	w.thread.with(|t| set_phase(t, Phase::Task, "test"));
	// Working when its check ends: the gates wait for its next turn end.
	w.thread.with(|t| t.state = AgentState::Working);
	assert_eq!(idle().await, 0);
	w.thread.with(|t| t.idle_tree = None);
	w.thread.with(|t| t.state = AgentState::Idle);
	assert_eq!(idle().await, 1);
}

#[tokio::test]
async fn with_the_layer_off_the_check_on_idle_runs_nothing_0576() {
	let w = World::with(Setup { layer_off: true, done_when: vec!["git diff --quiet".to_owned()], ..Setup::default() })
		.await;
	std::fs::write(w.wt.join("README.md"), "demo\nchanged\n").unwrap();
	w.flow().start_idle_check(&w.thread);
	w.flow().settled().await;
	assert!(w.typed("t-1").is_empty());
	assert_eq!(w.thread().idle_tree, None);
}

/// A project pane's check as its extension asks for it; "" for nothing.
async fn ask(w: &World, key: &str) -> String {
	w.flow().pane_check(key, None).await.unwrap()
}

/// Edits the project's README, so its tree differs from the last one.
fn edit(w: &World, text: &str) {
	std::fs::write(w.main.join("README.md"), format!("demo\n{text}\n")).unwrap();
}

/// Waits (up to 20 s) until `done` holds.
async fn until(mut done: impl FnMut() -> bool) {
	let start = Instant::now();
	while !done() {
		assert!(start.elapsed() < Duration::from_secs(20), "timed out");
		tokio::time::sleep(Duration::from_millis(20)).await;
	}
}

fn failed_late(line: &str) -> bool {
	line.strip_prefix("demo — checks failed on tree ").is_some_and(|rest| {
		rest.len() > 6
			&& rest[..6].chars().all(|c| c.is_ascii_hexdigit())
			&& &rest[6..] == " after its turn ended; its agent gets them at its next turn end"
	})
}

#[tokio::test]
async fn a_project_panes_turn_end_with_edits_not_yet_checked_gets_the_failing_check_in_the_threads_frame_once_per_tree_0617()
 {
	let w = World::with(Setup { mise: true, ..Setup::default() }).await;
	// No edits since HEAD: nothing to check.
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 0);
	w.control("exit", 1);
	edit(&w, "one");
	let text = ask(&w, "project-key").await;
	assert!(text.starts_with(&format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL")), "{text}");
	assert_eq!(w.lines("runs"), 1);
	// The event is called again after every settle, the continuation's included: the same tree is never answered twice.
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 1);
	// A pass says nothing; the tree is not checked again.
	w.control("exit", 0);
	edit(&w, "two");
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 2);
	// Committed: the tree is HEAD's.
	commit_in(&w.main, "demo\ntwo\n", "Add two");
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 2);
}

#[tokio::test]
async fn a_threads_key_the_bosss_key_and_an_unknown_key_get_nothing_and_run_nothing_0617() {
	let w = World::with(Setup { mise: true, ..Setup::default() }).await;
	w.control("exit", 1);
	edit(&w, "one");
	assert_eq!(ask(&w, "thread-key").await, "");
	assert_eq!(ask(&w, "boss-key").await, "");
	assert_eq!(ask(&w, "no such key").await, "");
	assert_eq!(w.lines("runs"), 0);
	// The project pane itself still gets it.
	assert!(ask(&w, "project-key").await.starts_with(IDLE_CHECK_FRAME));
}

#[tokio::test]
async fn with_the_layer_off_a_project_panes_check_runs_nothing_and_answers_nothing_0576_0617() {
	let w = World::with(Setup { mise: true, layer_off: true, ..Setup::default() }).await;
	w.control("exit", 1);
	edit(&w, "one");
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 0);
}

#[tokio::test]
async fn a_check_that_outlasts_the_wait_lets_the_turn_end_its_failure_reaches_today_and_is_owed_once_0617() {
	let w = World::with(Setup { mise: true, wait_ms: Some(200), ..Setup::default() }).await;
	w.control("exit", 1);
	w.hold();
	edit(&w, "one");
	assert_eq!(ask(&w, "project-key").await, "");
	until(|| w.lines("runs") == 1).await;
	// Called again while the check runs: the same tree, no second run, nothing owed yet.
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 1);
	w.release();
	until(|| w.today().iter().any(|line| failed_late(line))).await;
	assert_eq!(w.today().iter().filter(|line| failed_late(line)).count(), 1);
	assert!(ask(&w, "project-key").await.starts_with(&format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL")));
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 1);
	// Slow again, and the tree changes before it ends: the owed failure is for a tree no longer there.
	w.hold();
	edit(&w, "two");
	assert_eq!(ask(&w, "project-key").await, "");
	until(|| w.lines("runs") == 2).await;
	w.control("exit", 0);
	edit(&w, "three");
	w.release();
	until(|| w.today().iter().filter(|line| failed_late(line)).count() == 2).await;
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(ask(&w, "project-key").await, "");
	w.flow().settled().await;
}

#[tokio::test]
async fn a_turn_end_on_a_new_tree_while_the_panes_last_check_still_runs_waits_for_it_0617() {
	let w = World::with(Setup { mise: true, wait_ms: Some(200), ..Setup::default() }).await;
	w.control("exit", 1);
	w.hold();
	edit(&w, "one");
	assert_eq!(ask(&w, "project-key").await, "");
	until(|| w.lines("runs") == 1).await;
	edit(&w, "two");
	assert_eq!(ask(&w, "project-key").await, "");
	assert_eq!(w.lines("runs"), 1);
	w.release();
	until(|| w.lines("runs") == 2).await;
	until(|| w.today().iter().filter(|line| line.contains("checks failed")).count() == 2).await;
	assert_eq!(w.lines("overlaps"), 0);
}

#[tokio::test]
async fn a_role_that_stops_asking_mid_wait_ends_the_services_wait_and_the_failure_is_owed_to_the_next_turn_end_0617() {
	let w = World::with(Setup { mise: true, wait_ms: Some(60_000), ..Setup::default() }).await;
	w.control("exit", 1);
	w.hold();
	edit(&w, "one");
	let (abort, signal) = tokio::sync::watch::channel(None::<String>);
	let flow = &w.t.ctx.review;
	let asked = flow.pane_check("project-key", Some(signal));
	let started = Instant::now();
	let ((), answer) = tokio::join!(
		async {
			until(|| w.lines("runs") == 1).await;
			abort.send(Some("the role stopped asking".to_owned())).unwrap();
		},
		asked
	);
	// Within the 60 s wait: the service saw the role leave, so the answer is nothing at once.
	assert_eq!(answer.unwrap(), "");
	assert!(started.elapsed() < Duration::from_secs(30));
	w.release();
	// Had it not, the failure would go to the gone request and today would say nothing.
	until(|| w.today().iter().any(|line| line.contains("checks failed"))).await;
	assert!(ask(&w, "project-key").await.starts_with(&format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL")));
	assert_eq!(w.lines("runs"), 1);
}

#[tokio::test]
async fn stopping_the_service_ends_a_pane_checks_wait_at_once_and_writes_no_today_line_0617() {
	let w = World::with(Setup { mise: true, wait_ms: Some(18_000), ..Setup::default() }).await;
	w.control("exit", 1);
	w.hold();
	edit(&w, "one");
	let stopped = std::cell::Cell::new(None);
	let (answer, ()) = tokio::join!(w.flow().pane_check("project-key", None), async {
		until(|| w.lines("runs") == 1).await;
		stopped.set(Some(Instant::now()));
		w.t.ctx.set_stopping();
		w.flow().end_pane_waits();
		w.t.ctx.checks.stop().await;
	});
	assert_eq!(answer.unwrap(), "");
	// At once: well inside the 18 s wait, counted from the stop (the check's own start can be slow under load).
	assert!(stopped.get().expect("the stop").elapsed() < Duration::from_secs(10));
	w.flow().settled().await;
	assert!(!w.today().iter().any(|line| line.contains("checks failed")));
	w.release();
}
