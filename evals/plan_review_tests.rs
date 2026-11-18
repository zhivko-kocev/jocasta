//! Ported from `test/evals/plan-review.test.ts`. The generated extension is JavaScript omp loads, so these cases run it
//! on Bun (a subprocess) with a stand-in for omp's extension API, as the TypeScript test imported it. The fake herdr of
//! "a dropped herdr read never ends the turn" is a `TrialPanes` that starts the stand-in `sleep`; the fake omp of "a
//! plan-review trial" is the stand-in omp of `evals/stand_in.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use jocasta::frontier::server::herdr::{TabPane, TabSpec};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};
use serde_json::{Value, json};

use super::plan_review::{PlanReviewChoice, PlanReviewSummary, plan_review_extension, read_plan_review_log};
use super::runner::{EvalCase, TrialOptions, plan_review_overlay, run_trial};
use super::stand_in::{exe, launcher};
use super::terminal::{
	TRIAL_WORKSPACE, TerminalDeps, TerminalEnd, TerminalRunner, TerminalTurn, TrialPanes, env_delta, herdr_terminal,
};
use super::variants::load_variants;
use jocasta::shared::testing::temp_dir;

/// Loads the extension in `argv[2]` with a stand-in `pi`, replays the steps in `argv[3]` (`["fire", event, payload]`,
/// `["idle", bool]`, `["mark"]`) and prints the calls it saw, as a snapshot at each mark and at the end.
const DRIVER: &str = r#"import { pathToFileURL } from "node:url";
const [file, script] = process.argv.slice(2);
const handlers = new Map();
const calls = { resolved: [], shutdowns: 0 };
let idle = true;
const ctx = {
	resolvePlanReview: (...args) => {
		calls.resolved.push(args);
		return true;
	},
	isIdle: () => idle,
	hasPendingMessages: () => false,
	shutdown: () => {
		calls.shutdowns++;
	},
};
const module = await import(pathToFileURL(file).href);
module.default({ on: (event, handler) => handlers.set(event, [...(handlers.get(event) ?? []), handler]) });
const marks = [];
for (const step of JSON.parse(script)) {
	if (step[0] === "idle") idle = step[1];
	else if (step[0] === "mark") marks.push(JSON.parse(JSON.stringify(calls)));
	else for (const handler of handlers.get(step[1]) ?? []) handler(step[2] ?? {}, ctx);
}
console.log(JSON.stringify({ calls, marks }, (_key, value) => (value === undefined ? "<undefined>" : value)));
"#;

struct Seen {
	calls: Value,
	marks: Vec<Value>,
}

impl Seen {
	/// The `resolvePlanReview` calls with their first `n` arguments.
	fn resolved(calls: &Value, n: usize) -> Value {
		Value::Array(
			calls["resolved"]
				.as_array()
				.unwrap()
				.iter()
				.map(|args| Value::Array(args.as_array().unwrap().iter().take(n).cloned().collect()))
				.collect(),
		)
	}
}

fn run_extension(dir: &Path, file: &Path, steps: &Value) -> Seen {
	let driver = dir.join("driver.mjs");
	std::fs::write(&driver, DRIVER).unwrap();
	let bun = on_path("bun", None).expect("bun is on PATH");
	let run = run_program_sync(
		&[bun.as_os_str(), driver.as_os_str(), file.as_os_str(), steps.to_string().as_ref()],
		RunOptions { timeout_ms: Some(60_000), ..RunOptions::default() },
	)
	.unwrap();
	assert_eq!(run.code, Some(0), "{}", run.stderr);
	let mut seen: Value = serde_json::from_str(run.stdout.trim()).unwrap();
	Seen { calls: seen["calls"].take(), marks: seen["marks"].as_array().cloned().unwrap_or_default() }
}

fn fire(event: &str, payload: Value) -> Value {
	json!(["fire", event, payload])
}

/// omp's events around an approval: the planning run, the review, then the execution run the approval starts.
fn approve_and_execute(choice: &str) -> Vec<Value> {
	vec![
		fire("agent_start", json!({})),
		fire("message_end", json!({ "message": { "role": "assistant" } })),
		fire("plan_review_requested", json!({ "reviewId": "r1" })),
		fire("plan_review_resolved", json!({ "reviewId": "r1", "choice": choice })),
		// The planning run ends after the approval, before the execution run: no shutdown yet.
		fire("agent_end", json!({})),
		fire("message_end", json!({ "message": { "role": "user" } })),
		fire("agent_start", json!({})),
		fire("message_end", json!({ "message": { "role": "assistant" } })),
		fire("agent_end", json!({})),
	]
}

fn write_extension(
	dir: &Path,
	name: &str,
	choice: PlanReviewChoice,
	role: Option<&str>,
) -> (std::path::PathBuf, String) {
	let log = dir.join(format!("{name}.jsonl")).display().to_string();
	let file = dir.join(format!("{name}.js"));
	std::fs::write(&file, plan_review_extension(choice, &log, role)).unwrap();
	(file, log)
}

#[test]
fn the_extension_answers_the_review_with_the_arms_choice_and_shuts_omp_down_after_the_execution_not_before() {
	let dir = temp_dir("jocasta-review-");
	let (file, log) = write_extension(dir.path(), "plan-review", PlanReviewChoice::Execute, None);
	let steps = json!([
		fire("agent_start", json!({})),
		fire("plan_review_requested", json!({ "reviewId": "r1" })),
		["mark"],
		fire("plan_review_resolved", json!({ "reviewId": "r1", "choice": "execute" })),
		fire("agent_end", json!({})),
		["mark"],
		fire("message_end", json!({ "message": { "role": "user" } })),
		fire("agent_start", json!({})),
		fire("message_end", json!({ "message": { "role": "assistant" } })),
		fire("agent_end", json!({})),
	]);
	let seen = run_extension(dir.path(), &file, &steps);
	assert_eq!(Seen::resolved(&seen.marks[0], 2), json!([["r1", "execute"]]));
	assert_eq!(seen.marks[1]["shutdowns"], 0);
	assert_eq!(seen.calls["shutdowns"], 1);
	assert_eq!(
		read_plan_review_log(&std::fs::read_to_string(log).unwrap(), PlanReviewChoice::Execute, None),
		PlanReviewSummary {
			requested: true,
			choice: PlanReviewChoice::Execute,
			answered: true,
			role: None,
			resolved: Some("execute".to_owned()),
			shutdown: Some("the approved plan's execution ended".to_owned()),
		}
	);
}

#[test]
fn keep_context_answers_keep_a_planning_turn_that_ends_without_a_proposal_shuts_down_too_busy_omp_waits() {
	let dir = temp_dir("jocasta-review-");
	let (keep_file, _) = write_extension(dir.path(), "keep", PlanReviewChoice::Keep, None);
	let keep = run_extension(dir.path(), &keep_file, &Value::Array(approve_and_execute("keep")));
	assert_eq!(Seen::resolved(&keep.calls, 2), json!([["r1", "keep"]]));
	assert_eq!(keep.calls["shutdowns"], 1);

	let (none_file, none_log) = write_extension(dir.path(), "none", PlanReviewChoice::Execute, None);
	let none = run_extension(
		dir.path(),
		&none_file,
		&json!([["idle", false], fire("agent_end", json!({})), ["mark"], ["idle", true], fire("agent_end", json!({}))]),
	);
	assert_eq!(none.marks[0]["shutdowns"], 0);
	assert_eq!(none.calls["shutdowns"], 1);
	let summary = read_plan_review_log(&std::fs::read_to_string(none_log).unwrap(), PlanReviewChoice::Execute, None);
	assert!(!summary.requested);
	assert!(!summary.answered);
	assert_eq!(summary.shutdown.as_deref(), Some("turn ended without a plan proposal"));
	assert_eq!(
		read_plan_review_log("", PlanReviewChoice::Keep, None),
		PlanReviewSummary {
			requested: false,
			choice: PlanReviewChoice::Keep,
			answered: false,
			role: None,
			resolved: None,
			shutdown: None,
		}
	);
}

#[test]
fn a_role_is_passed_through_as_resolve_plan_reviews_role_and_logged_0608() {
	let dir = temp_dir("jocasta-review-");
	let (file, log) = write_extension(dir.path(), "smol", PlanReviewChoice::Keep, Some("smol"));
	let seen = run_extension(dir.path(), &file, &json!([fire("plan_review_requested", json!({ "reviewId": "r1" }))]));
	assert_eq!(seen.calls["resolved"], json!([["r1", "keep", { "role": "smol" }]]));
	let summary = read_plan_review_log(&std::fs::read_to_string(log).unwrap(), PlanReviewChoice::Keep, Some("smol"));
	assert_eq!(summary.choice, PlanReviewChoice::Keep);
	assert_eq!(summary.role.as_deref(), Some("smol"));
	assert!(summary.answered);
	// Without a role the third argument stays undefined, as before.
	let (plain, _) = write_extension(dir.path(), "plain", PlanReviewChoice::Keep, None);
	let seen = run_extension(dir.path(), &plain, &json!([fire("plan_review_requested", json!({ "reviewId": "r2" }))]));
	assert_eq!(seen.calls["resolved"], json!([["r2", "keep", "<undefined>"]]));
}

#[test]
fn a_panes_environment_carries_what_the_trial_changed_and_empties_what_it_dropped() {
	let pairs = |entries: &[(&str, &str)]| -> HashMap<String, String> {
		entries.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
	};
	assert_eq!(
		env_delta(&pairs(&[("A", "1"), ("B", "changed"), ("D", "new")]), &pairs(&[("A", "1"), ("B", "2"), ("C", "3")])),
		[("B".to_owned(), "changed".to_owned()), ("C".to_owned(), String::new()), ("D".to_owned(), "new".to_owned())]
	);
}

/// What the fake terminal was started with: the arguments and the overlay they name.
type Launched = (Vec<String>, Value);

/// What the fake herdr saw, shared with the test.
#[derive(Default)]
struct Seen2 {
	calls: Mutex<Vec<String>>,
	reads: AtomicUsize,
	pid: Mutex<Option<u32>>,
}

struct FakePanes {
	seen: Arc<Seen2>,
}

impl TrialPanes for FakePanes {
	fn ensure_workspace<'a>(&'a self, label: &'a str, cwd: &'a Path) -> BoxFuture<'a, Result<String>> {
		self.seen.calls.lock().unwrap().push(format!("workspace {label} in {}", cwd.display()));
		Box::pin(async { Ok("w1".to_owned()) })
	}

	fn create_tab<'a>(&'a self, _workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, Result<TabPane>> {
		let child = std::process::Command::new(&tab.command[0])
			.args(&tab.command[1..])
			.current_dir(&tab.cwd)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();
		*self.seen.pid.lock().unwrap() = Some(child.id());
		// The pane's process is reaped as herdr would: a zombie would look alive.
		std::thread::spawn(move || {
			let mut child = child;
			let _ = child.wait();
		});
		Box::pin(async { Ok(TabPane { tab_id: "t1".to_owned(), pane_id: "p1".to_owned() }) })
	}

	fn pane_root<'a>(&'a self, _pane_id: &'a str) -> BoxFuture<'a, Result<Option<u32>>> {
		let reads = self.seen.reads.fetch_add(1, Ordering::SeqCst) + 1;
		let pid = *self.seen.pid.lock().unwrap();
		Box::pin(async move {
			// herdr 0.9.5 sometimes closes the connection without answering (0517).
			if reads == 1 {
				return Err(Error::other("herdr closed the connection before answering pane.process_info"));
			}
			Ok(pid)
		})
	}

	fn close(&self) {
		self.seen.calls.lock().unwrap().push("close".to_owned());
	}
}

#[tokio::test]
async fn a_dropped_herdr_read_never_ends_the_turn_the_panes_process_is_watched_locally_until_it_exits_evl_6_evl_17() {
	let work = temp_dir("jocasta-review-");
	let seen = Arc::new(Seen2::default());
	let connect_seen = seen.clone();
	let close_seen = seen.clone();
	let deps = TerminalDeps {
		workspace_cwd: Some(work.path().to_path_buf()),
		connect: Some(Arc::new(move |_session| {
			let panes = Arc::new(FakePanes { seen: connect_seen.clone() }) as Arc<dyn TrialPanes>;
			Box::pin(async move { Ok(panes) })
		})),
		close_tab: Some(Arc::new(move |_session, tab_id| {
			close_seen.calls.lock().unwrap().push(format!("tab.close {tab_id}"));
			Box::pin(async {})
		})),
	};
	let terminal = herdr_terminal("bench-pty-test", deps).unwrap();
	let ended = terminal(TerminalTurn {
		command: exe(),
		args: vec!["stand-in".to_owned(), "sleep".to_owned(), "1500".to_owned()],
		cwd: work.path().to_path_buf(),
		env: HashMap::new(),
		timeout_ms: Some(60_000),
	})
	.await
	.unwrap();
	assert!(!ended.timed_out);
	// The turn lasted until the process exited, past the dropped read.
	assert!(ended.wall_ms >= 1_400, "{}", ended.wall_ms);
	assert_eq!(seen.reads.load(Ordering::SeqCst), 2);
	assert_eq!(
		*seen.calls.lock().unwrap(),
		[
			format!("workspace {TRIAL_WORKSPACE} in {}", work.path().display()),
			"tab.close t1".to_owned(),
			"close".to_owned()
		]
	);
	let Err(refused) = herdr_terminal("jocasta", TerminalDeps::default()) else { panic!("jocasta is refused") };
	let refused = refused.to_string();
	assert!(refused.contains("the trials' terminal session can't be jocasta"), "{refused}");
}

#[tokio::test]
async fn a_plan_review_trial_the_first_turn_in_the_terminal_with_the_extension_plan_mode_on_the_review_turns_in_print_mode()
 {
	let _jocasta = jocasta::shared::testing::temp_jocasta();
	let root_dir = temp_dir("jocasta-review-");
	let root = root_dir.path().to_path_buf();
	let fixture = root.join("fixture");
	std::fs::create_dir(&fixture).unwrap();
	std::fs::write(fixture.join("lib.txt"), "x\n").unwrap();
	let variants = root.join("variants");
	std::fs::create_dir(&variants).unwrap();
	std::fs::write(
		variants.join("review.yaml"),
		"model: anthropic/claude-opus-5-5\nplan_review: execute\nfollowups: [\"Check it\"]\nsettings:\n  providers:\n    cacheWarming: streaming\n",
	)
	.unwrap();
	let eval_case = EvalCase {
		id: "c1".to_owned(),
		dir: root.clone(),
		prompt: "Fix it".to_owned(),
		fixture: Some(fixture),
		..EvalCase::default()
	};
	let out = root.join("out");
	std::fs::create_dir_all(out.join("transcripts")).unwrap();
	let seen: Arc<Mutex<Vec<Launched>>> = Arc::default();
	let (recorded, driver_dir) = (seen.clone(), root.clone());
	let terminal: TerminalRunner = Arc::new(move |turn: TerminalTurn| {
		let (recorded, dir) = (recorded.clone(), driver_dir.clone());
		Box::pin(async move {
			let after = |flag: &str| {
				let at = turn.args.iter().position(|arg| arg == flag).unwrap();
				turn.args[at + 1].clone()
			};
			let overlay: Value = serde_json::from_str(&std::fs::read_to_string(after("--config")).unwrap()).unwrap();
			recorded.lock().unwrap().push((turn.args.clone(), overlay));
			// omp interactive with the extension: its events, and the session file it writes.
			run_extension(&dir, Path::new(&after("-e")), &Value::Array(approve_and_execute("execute")));
			let sessions = PathBuf::from(after("--session-dir"));
			std::fs::create_dir_all(&sessions).unwrap();
			let entries = [
				json!({ "type": "session", "id": "s1", "timestamp": "2026-11-09T10:00:00.000Z" }),
				json!({
					"type": "message",
					"timestamp": "2026-11-09T10:00:01.000Z",
					"message": {
						"role": "assistant",
						"model": "claude-opus-5-5",
						"content": [{ "type": "toolCall", "id": "c", "name": "write", "arguments": { "path": "lib.txt" } }],
						"usage": { "input": 10, "output": 2, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 12, "cost": { "total": 0.01 } },
					},
				}),
			];
			let text: String = entries.iter().map(|entry| format!("{entry}\n")).collect();
			std::fs::write(sessions.join("s1.jsonl"), text).unwrap();
			Ok(TerminalEnd { timed_out: false, wall_ms: 5 })
		})
	});
	let trial = TrialOptions {
		out_dir: out.clone(),
		launcher: launcher("omp"),
		variant: Some(load_variants(&variants, &["review".to_owned()]).unwrap()),
		run_in_terminal: Some(terminal),
		work_dir: Some(root.clone()),
		..TrialOptions::default()
	};
	let row = run_trial(&eval_case, 0, "omp/stub", trial).await.unwrap();
	let seen = seen.lock().unwrap();
	let (args, overlay) = &seen[0];
	assert!(!args.contains(&"-p".to_owned()));
	assert_eq!(args.last().unwrap(), "Fix it");
	let at = |flag: &str| args[args.iter().position(|arg| arg == flag).unwrap() + 1].clone();
	assert_eq!(Path::new(&at("-e")).parent(), Path::new(&at("--config")).parent());
	let mut expected = Value::Object(plan_review_overlay());
	expected["providers"] = json!({ "cacheWarming": "streaming" });
	expected["memory"] = json!({ "backend": "off" });
	assert_eq!(overlay, &expected);
	assert_eq!(
		row.plan_review,
		Some(PlanReviewSummary {
			requested: true,
			choice: PlanReviewChoice::Execute,
			answered: true,
			role: None,
			resolved: Some("execute".to_owned()),
			shutdown: Some("the approved plan's execution ended".to_owned()),
		})
	);
	assert_eq!(row.grade["omp exited 0"], 1);
	assert_eq!(row.prompts, 2);
	// The first turn's usage from its session file, plus the review turn the fake omp appended to it (-c, --session-dir).
	assert_eq!(row.turns, 2);
	assert_eq!(row.usage.input, 1010.0);
	assert_eq!(row.findings.map(|found| (found.total, found.blocking)), Some((0, 0)));
	assert!(std::fs::read_to_string(out.join("transcripts").join("c1-0.jsonl")).unwrap().contains("\"id\":\"s1\""));
}
