//! Ported from `test/evals/tasks.test.ts`: the cases of the trial order, `tasks.jsonl`, `prepareTask`, the leak scan, the
//! infra classifier and the first `resume` case, which need only `tasks.rs`. The cases that call `runBench` or
//! `runValidation` (the second `resume` case onward) live with `evals/bench.rs`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::runner::{Check, ResultRow};
use super::stand_in::exe;
use super::tasks::{
	BenchTask, InfraKind, InfraOutcome, MAX_ATTEMPTS, PYTHON_SETUP_PREFIX, PreparedTask, TASK_PROMPT_PREFIX,
	TaskLanguage, TaskSet, Trial, classify_infra, load_tasks, pending_trials, pick_pilot, plan_trials, prepare_task,
	read_resume_state, scan_leaks, shuffle, task_case, trial_key,
};
use super::variants::Harness;
use crate::lib::git::git as run_git;
use jocasta::shared::testing::temp_dir;

fn git(cwd: &Path, args: &[&str]) -> String {
	let mut all = vec!["-c", "core.autocrlf=false"];
	all.extend(args);
	run_git(cwd, &all, None).unwrap()
}

pub(super) fn write_files(dir: &Path, files: &[(&str, &str)]) {
	for (name, content) in files {
		let file = dir.join(name);
		fs::create_dir_all(file.parent().unwrap()).unwrap();
		fs::write(file, content).unwrap();
	}
}

const CHECK: &str = "check\n";

/// A local upstream: the base commit (the bug), the fix's merge commit (gold change, a changed test and a new one) and a
/// later commit that must never reach the fixture. The task fetches it by path, so nothing touches the network.
pub(super) fn local_task(source: &Path) -> BenchTask {
	git(source, &["init", "-q", "-b", "main"]);
	git(source, &["config", "user.name", "Acme Dev"]);
	git(source, &["config", "user.email", "dev@example.org"]);
	write_files(source, &[("lib.txt", "bug\n"), ("tests/existing.txt", "old\n"), ("README.md", "# widget\n")]);
	git(source, &["add", "-A"]);
	git(source, &["commit", "-q", "-m", "base"]);
	let base = git(source, &["rev-parse", "HEAD"]).trim().to_owned();
	git(source, &["tag", "v1.0.0"]);
	write_files(source, &[("lib.txt", "fixed\n"), ("tests/existing.txt", "new\n"), ("tests/check.ts", CHECK)]);
	git(source, &["add", "-A"]);
	git(source, &["commit", "-q", "-m", "fix (#7)"]);
	let merge = git(source, &["rev-parse", "HEAD"]).trim().to_owned();
	write_files(source, &[("later.txt", "after the fix\n")]);
	git(source, &["add", "-A"]);
	git(source, &["commit", "-q", "-m", "later"]);
	git(source, &["tag", "v1.1.0"]);
	BenchTask {
		instance_id: "acme__widget-7".to_owned(),
		set: TaskSet::Main,
		repo: "acme/widget".to_owned(),
		repo_url: source.display().to_string(),
		base_commit: base.clone(),
		problem_statement: "lib.txt says bug\n\nIt should say it is fixed.".to_owned(),
		gold_patch: git(source, &["diff", &base, &merge, "--", "lib.txt"]),
		test_patch: git(source, &["diff", &base, &merge, "--", "tests"]),
		hidden_test_files: vec!["tests/check.ts".to_owned(), "tests/existing.txt".to_owned()],
		setup_command: "git --version".to_owned(),
		test_command: format!("\"{}\" stand-in file-is lib.txt fixed", exe()),
		language: TaskLanguage::JavaScript,
		pr_url: "https://github.com/acme/widget/pull/7".to_owned(),
		issue_urls: vec!["https://github.com/acme/widget/issues/3".to_owned()],
		merge_commit: merge,
	}
}

pub(super) fn stub_task(id: &str, language: TaskLanguage) -> BenchTask {
	BenchTask {
		instance_id: id.to_owned(),
		set: TaskSet::Main,
		repo: "acme/widget".to_owned(),
		repo_url: "https://github.com/acme/widget".to_owned(),
		base_commit: "a".repeat(40),
		problem_statement: format!("Issue of {id}"),
		gold_patch: "diff --git a/lib.txt b/lib.txt\n".to_owned(),
		test_patch: "diff --git a/test.txt b/test.txt\n".to_owned(),
		hidden_test_files: Vec::new(),
		setup_command: "pip install -e .".to_owned(),
		test_command: "pytest".to_owned(),
		language,
		pr_url: "https://github.com/acme/widget/pull/12".to_owned(),
		issue_urls: vec!["https://github.com/acme/widget/issues/11".to_owned()],
		merge_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
	}
}

fn grade(entries: &[(&str, u8)]) -> Map<String, Value> {
	entries.iter().map(|(name, score)| ((*name).to_owned(), json!(score))).collect()
}

pub(super) fn fake_row(change: impl FnOnce(&mut ResultRow)) -> ResultRow {
	let usage = json!({ "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 2, "cost": 0 });
	let mut row: ResultRow = serde_json::from_value(json!({
		"case": "acme__widget-7",
		"rep": 0,
		"variant": "a0",
		"harness": "omp",
		"harnessVersion": "omp/0.0.0-stub",
		"startedAt": "2026-10-27T10:00:00.000Z",
		"model": "stub-model",
		"usage": usage,
		"totalUsage": usage,
		"requests": 2,
		"turns": 2,
		"toolCalls": 1,
		"prompts": 1,
		"wallMs": 10,
		"linesAdded": 1,
		"linesRemoved": 1,
		"grade": { "omp exited 0": 1, "hidden: command: test": 1 },
		"hiddenPassed": true,
		"findings": null,
		"passed": true,
	}))
	.unwrap();
	change(&mut row);
	row
}

fn outcome(row: Option<ResultRow>) -> InfraOutcome {
	InfraOutcome {
		error: None,
		row,
		stderr: String::new(),
		transcript: String::new(),
		prompt: String::new(),
		account_full: false,
	}
}

fn ids(tasks: &[BenchTask]) -> Vec<&str> {
	tasks.iter().map(|task| task.instance_id.as_str()).collect()
}

fn arms(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_seeded_shuffle_is_deterministic_and_a_permutation() {
	let items: Vec<u32> = (0..50).collect();
	let once = shuffle(&items, 482);
	assert_eq!(shuffle(&items, 482), once);
	let mut sorted = once.clone();
	sorted.sort_unstable();
	assert_eq!(sorted, items);
	assert_ne!(once, items);
	assert_ne!(shuffle(&items, 483), once);
}

#[test]
fn trials_are_every_task_x_arm_x_rep_once_interleaved_indexed_in_order() {
	let tasks: Vec<BenchTask> = ["t1", "t2", "t3"].iter().map(|id| stub_task(id, TaskLanguage::Go)).collect();
	let order = plan_trials(&tasks, &arms(&["a0", "a1"]), 3, 482);
	assert_eq!(order.len(), 18);
	assert_eq!(order.iter().map(trial_key).collect::<HashSet<_>>().len(), 18);
	assert_eq!(order.iter().map(|trial| trial.index).collect::<Vec<_>>(), (0..18).collect::<Vec<_>>());
	assert_eq!(plan_trials(&tasks, &arms(&["a0", "a1"]), 3, 482), order);
	// Not grouped by arm: the first six trials aren't all one arm.
	assert_eq!(order[..6].iter().map(|trial| trial.arm.as_str()).collect::<HashSet<_>>().len(), 2);
}

#[test]
fn the_pilot_picks_2_python_2_type_script_java_script_and_1_go_task_the_same_for_a_seed() {
	let mut tasks: Vec<BenchTask> =
		["p1", "p2", "p3", "p4"].iter().map(|id| stub_task(id, TaskLanguage::Python)).collect();
	tasks.push(stub_task("t1", TaskLanguage::TypeScript));
	tasks.push(stub_task("j1", TaskLanguage::JavaScript));
	tasks.push(stub_task("j2", TaskLanguage::JavaScript));
	tasks.extend(["g1", "g2", "g3"].iter().map(|id| stub_task(id, TaskLanguage::Go)));
	let pilot = pick_pilot(&tasks, 482).unwrap();
	let languages: Vec<TaskLanguage> = pilot.iter().map(|task| task.language).collect();
	assert_eq!(languages[..2], [TaskLanguage::Python, TaskLanguage::Python]);
	assert!(
		languages[2..4].iter().all(|language| matches!(language, TaskLanguage::TypeScript | TaskLanguage::JavaScript))
	);
	assert_eq!(languages[4], TaskLanguage::Go);
	assert_eq!(languages.len(), 5);
	let mut reversed = tasks.clone();
	reversed.reverse();
	assert_eq!(ids(&pick_pilot(&reversed, 482).unwrap()), ids(&pilot));
	assert_eq!(plan_trials(&pilot, &arms(&["a0", "a1", "a2", "a3", "a4"]), 1, 482).len(), 25);
	let error = pick_pilot(&tasks[..7], 482).unwrap_err().to_string();
	assert!(error.contains("the pilot needs 1 Go"), "{error}");
}

#[test]
fn loads_the_tasks_and_names_a_bad_line() {
	let dir = temp_dir("jocasta-bench");
	let file = dir.path().join("tasks.jsonl");
	let task = |change: &dyn Fn(&mut Value)| {
		let mut value = serde_json::to_value(stub_task("acme__widget-1", TaskLanguage::Go)).unwrap();
		change(&mut value);
		format!("{value}\n")
	};
	fs::write(&file, format!("{}\n", task(&|_| {}))).unwrap();
	assert_eq!(ids(&load_tasks(&file).unwrap()), ["acme__widget-1"]);
	fs::write(&file, task(&|value| value["test_command"] = json!(""))).unwrap();
	let error = load_tasks(&file).unwrap_err().to_string();
	assert!(error.contains("tasks.jsonl:1: `test_command`"), "{error}");
	fs::write(&file, task(&|value| value["language"] = json!("Rust"))).unwrap();
	let error = load_tasks(&file).unwrap_err().to_string();
	assert!(error.contains("`language`"), "{error}");
}

#[test]
fn a_task_becomes_a_case_the_fixed_prompt_hidden_test_command_a_venv_for_python_a_fresh_one_for_grading() {
	let prepared = PreparedTask {
		dir: PathBuf::from("/cache/x"),
		fixture: PathBuf::from("/cache/x/fixture"),
		hidden: PathBuf::from("/cache/x/hidden"),
		gold_patch: PathBuf::from("/cache/x/gold.patch"),
		test_patch: PathBuf::from("/cache/x/test.patch"),
		tag: None,
	};
	let python = task_case(&stub_task("acme__py-1", TaskLanguage::Python), &prepared);
	assert_eq!(TASK_PROMPT_PREFIX, "Fix the issue below in the repository in the current folder.\n\n");
	assert_eq!(python.prompt, format!("{TASK_PROMPT_PREFIX}Issue of acme__py-1"));
	let setup = python.setup.clone().unwrap();
	assert_eq!(
		setup,
		"(if not exist ..\\venv\\Scripts\\python.exe python -m venv ..\\venv) && python -m pip install -q --upgrade pip && pip install -e ."
	);
	assert!(setup.starts_with(PYTHON_SETUP_PREFIX));
	assert_eq!(python.path_prefix, Some(vec!["venv/Scripts".to_owned()]));
	// 0574: grading's setup makes a new venv beside it, never the one the agent worked in.
	assert_eq!(
		python.grade_setup.as_deref(),
		Some(
			"(if exist ..\\gvenv rmdir /s /q ..\\gvenv) && python -m venv ..\\gvenv && python -m pip install -q --upgrade pip && pip install -e ."
		)
	);
	assert_eq!(python.grade_path_prefix, Some(vec!["gvenv/Scripts".to_owned()]));
	assert!(python.checks.is_empty());
	assert_eq!(python.hidden_checks, [Check { command: Some("pytest".to_owned()), ..Check::default() }]);
	assert_eq!(python.fixture, Some(PathBuf::from("/cache/x/fixture")));
	assert_eq!(python.hidden, Some(PathBuf::from("/cache/x/hidden")));
	assert_eq!(python.id, "acme__py-1");
	let go = task_case(
		&BenchTask { setup_command: "go mod download".to_owned(), ..stub_task("acme__go-1", TaskLanguage::Go) },
		&prepared,
	);
	assert_eq!(go.setup.as_deref(), Some("go mod download"));
	assert_eq!(go.path_prefix, None);
	assert_eq!(go.grade_setup, None);
	assert_eq!(go.tags, ["Go", "main"]);
}

#[test]
fn fetches_the_base_commit_alone_writes_its_tree_and_the_hidden_test_files_once() {
	let source = temp_dir("jocasta-bench");
	let task = local_task(source.path());
	let cache_dir = temp_dir("jocasta-bench");
	let cache = cache_dir.path();
	let prepared = prepare_task(&task, cache).unwrap();
	assert_eq!(prepared.fixture, cache.join("acme__widget-7").join("fixture"));
	let text = |path: PathBuf| fs::read_to_string(path).unwrap();
	assert_eq!(text(prepared.fixture.join("lib.txt")), "bug\n");
	assert_eq!(text(prepared.fixture.join("tests").join("existing.txt")), "old\n");
	assert!(!prepared.fixture.join("tests").join("check.ts").exists());
	assert!(!prepared.fixture.join("later.txt").exists());
	assert!(!prepared.fixture.join(".git").exists());
	assert_eq!(text(prepared.hidden.join("tests").join("check.ts")), CHECK);
	assert_eq!(text(prepared.hidden.join("tests").join("existing.txt")), "new\n");
	assert!(!prepared.hidden.join("lib.txt").exists());
	assert_eq!(text(prepared.gold_patch.clone()), task.gold_patch);
	assert_eq!(text(prepared.test_patch.clone()), task.test_patch);
	assert!(cache.join("repos").join("acme__widget.git").exists());
	// Upstream's nearest tag at the base commit, not the later one.
	assert_eq!(prepared.tag.as_deref(), Some("v1.0.0"));
	assert!(!cache.join("acme__widget-7").join("scratch").exists());
	// The marker matches, so a second call leaves the cache alone, even with the upstream gone.
	fs::write(prepared.fixture.join("sentinel.txt"), "kept\n").unwrap();
	let missing = BenchTask { repo_url: cache.join("missing").display().to_string(), ..task.clone() };
	assert_eq!(prepare_task(&missing, cache).unwrap(), prepared);
	assert!(prepared.fixture.join("sentinel.txt").exists());
	// Changed patches prepare again, from the bare repo's copy of the commit, and keep the tag.
	let changed = BenchTask { gold_patch: format!("{}\n", task.gold_patch), ..missing };
	let again = prepare_task(&changed, cache).unwrap();
	assert!(!prepared.fixture.join("sentinel.txt").exists());
	assert_eq!(again.tag.as_deref(), Some("v1.0.0"));
}

#[test]
fn a_hidden_test_file_the_patch_deletes_fails_preparation() {
	let source = temp_dir("jocasta-bench");
	let task = local_task(source.path());
	let task = BenchTask { hidden_test_files: vec!["tests/gone.txt".to_owned()], ..task };
	let error = prepare_task(&task, temp_dir("jocasta-bench").path()).unwrap_err().to_string();
	assert!(error.contains("deletes tests/gone.txt"), "{error}");
}

#[test]
fn finds_the_pr_issues_merge_commit_and_its_short_hash_outside_the_prompt() {
	let task = stub_task("acme__widget-12", TaskLanguage::Go);
	let prompt = format!("{TASK_PROMPT_PREFIX}see https://github.com/acme/widget/issues/11");
	assert_eq!(scan_leaks(&task, &json!({ "prompt": prompt }).to_string(), Some(&prompt)), Vec::<String>::new());
	assert_eq!(
		scan_leaks(&task, "fetched https://github.com/acme/widget/pull/12/files", None),
		["https://github.com/acme/widget/pull/12", "github.com/acme/widget/pull/12"]
	);
	assert_eq!(
		scan_leaks(&task, "see https://github.com/acme/widget/pull/123 and /issues/110", None),
		Vec::<String>::new()
	);
	assert_eq!(
		scan_leaks(&task, "HTTPS://GITHUB.COM/acme/widget/issues/11", None),
		["https://github.com/acme/widget/issues/11"]
	);
	assert_eq!(scan_leaks(&task, "git show 0123456789ab", None), ["0123456789"]);
	assert_eq!(
		scan_leaks(&task, &format!("/commit/{}", task.merge_commit), None),
		vec![format!("/commit/{}", task.merge_commit), "0123456789".to_owned()]
	);
	assert_eq!(scan_leaks(&task, "ff0123456789", None), Vec::<String>::new());
}

fn failed() -> ResultRow {
	fake_row(|row| {
		row.grade = grade(&[("omp exited 0", 0)]);
		row.turns = 0;
		row.tool_calls = 0;
		row.hidden_passed = Some(false);
	})
}

fn line(event: &Value) -> String {
	format!("{event}\n")
}

fn kind(outcome: &InfraOutcome) -> Option<InfraKind> {
	classify_infra(outcome).map(|infra| infra.kind)
}

fn with_error(error: &str) -> InfraOutcome {
	InfraOutcome { error: Some(error.to_owned()), ..outcome(None) }
}

fn with_stderr(row: &ResultRow, stderr: &str) -> InfraOutcome {
	InfraOutcome { stderr: stderr.to_owned(), ..outcome(Some(row.clone())) }
}

fn with_transcript(row: &ResultRow, transcript: &str, account_full: bool) -> InfraOutcome {
	InfraOutcome { transcript: transcript.to_owned(), account_full, ..outcome(Some(row.clone())) }
}

#[test]
fn setup_and_runner_errors() {
	assert_eq!(kind(&with_error("acme__widget-7: setup `npm install` failed: ENOTFOUND")), Some(InfraKind::Setup));
	// A setup cut at its limit (0574, amended) is infra too, retried as a setup error.
	assert_eq!(
		kind(&with_error("acme__widget-7: setup `npm install` killed at its time limit")),
		Some(InfraKind::Setup)
	);
	assert_eq!(kind(&with_error("git clone failed")), Some(InfraKind::Error));
}

#[test]
fn sign_in_failures_in_stderr_or_the_harnesss_own_error_end() {
	let failed = failed();
	assert_eq!(kind(&with_stderr(&failed, "Error: 401 Unauthorized")), Some(InfraKind::Auth));
	assert_eq!(kind(&with_stderr(&failed, "OAuth token refresh failed: invalid_grant")), Some(InfraKind::Auth));
	let claude = fake_row(|row| {
		row.harness = Harness::Claude;
		row.grade = grade(&[("claude exited 0", 0)]);
		row.turns = 0;
		row.tool_calls = 0;
	});
	let transcript =
		line(&json!({ "type": "result", "is_error": true, "result": "Invalid API key · Please run /login" }));
	assert_eq!(kind(&with_transcript(&claude, &transcript, false)), Some(InfraKind::Auth));
	// A trial that ran and exited 0 counts, whatever its transcript says about OAuth.
	assert_eq!(kind(&with_transcript(&fake_row(|_| {}), "fixed the OAuth refresh token bug (401)", false)), None);
}

#[test]
fn rate_limits_without_a_tool_call_and_no_model_response() {
	let failed = failed();
	assert_eq!(kind(&with_stderr(&failed, "429 Too Many Requests: rate_limit_error")), Some(InfraKind::RateLimit));
	assert_eq!(kind(&with_stderr(&failed, "API Error: 529 overloaded_error")), Some(InfraKind::RateLimit));
	assert_eq!(kind(&with_stderr(&fake_row(|row| row.tool_calls = 3), "overloaded")), None);
	assert_eq!(kind(&with_stderr(&failed, "segmentation fault")), Some(InfraKind::NoResponse));
	// The issue's own text never makes a rate limit.
	let prompt = format!("{TASK_PROMPT_PREFIX}undici retries 429 responses forever");
	let mut quoted = outcome(Some(fake_row(|row| row.tool_calls = 0)));
	quoted.transcript = json!({ "prompt": prompt }).to_string();
	quoted.prompt = prompt;
	assert_eq!(kind(&quoted), None);
	let exited = fake_row(|row| {
		row.grade = grade(&[("omp exited 0", 0)]);
		row.hidden_passed = Some(false);
	});
	assert_eq!(kind(&outcome(Some(exited))), None);
}

#[test]
fn a_trial_that_ended_on_a_rate_limit_while_an_account_was_full_is_infra_even_after_tool_calls_0543() {
	let worked = fake_row(|row| {
		row.tool_calls = 5;
		row.grade = grade(&[("omp exited 0", 0)]);
		row.hidden_passed = Some(false);
	});
	let end = line(&json!({
		"type": "message_end",
		"message": { "role": "assistant", "stopReason": "error", "errorMessage": "429 rate_limit_error" },
	}));
	assert_eq!(kind(&with_transcript(&worked, &end, true)), Some(InfraKind::RateLimit));
	let mut limit = with_stderr(&worked, "usage limit reached");
	limit.account_full = true;
	assert_eq!(kind(&limit), Some(InfraKind::RateLimit));
	// No full account: the trial counts as it ended.
	assert_eq!(kind(&with_transcript(&worked, &end, false)), None);
	// A6's session files carry the same end as a `message` entry; Claude Code's as a rejected rate_limit_event.
	let session = line(&json!({
		"type": "message",
		"message": { "role": "assistant", "stopReason": "error", "errorMessage": "overloaded" },
	}));
	assert_eq!(kind(&with_transcript(&worked, &session, true)), Some(InfraKind::RateLimit));
	let a4 = fake_row(|row| {
		row.harness = Harness::Claude;
		row.tool_calls = 5;
		row.grade = grade(&[("claude exited 0", 0)]);
		row.hidden_passed = Some(false);
	});
	let rejected = line(&json!({ "type": "rate_limit_event", "rate_limit_info": { "status": "rejected" } }));
	assert_eq!(kind(&with_transcript(&a4, &rejected, true)), Some(InfraKind::RateLimit));
}

#[test]
fn the_pilots_counted_shapes_stay_counted_with_a_full_account_passes_and_fails_alike_evl_2() {
	let passed = fake_row(|row| {
		row.tool_calls = 12;
		row.turns = 14;
	});
	let failed_task = fake_row(|row| {
		row.tool_calls = 12;
		row.turns = 14;
		row.grade = grade(&[("omp exited 0", 1), ("hidden: test", 0)]);
	});
	let usage = json!({ "input": 3, "output": 812, "cacheRead": 13330, "cacheWrite": 429, "totalTokens": 14574 });
	let agent_end = line(&json!({
		"type": "agent_end",
		"messages": [
			{ "role": "toolResult", "content": [{ "type": "text", "text": "427,428,429,430\n*429: mockFileContents(() =>\n401: );" }] },
			{ "role": "assistant", "stopReason": "stop", "usage": usage },
		],
	}));
	let omp_end = format!(
		"{}{agent_end}",
		line(
			&json!({ "type": "message_end", "message": { "role": "assistant", "stopReason": "stop", "usage": usage } })
		)
	);
	let a4 = fake_row(|row| {
		row.harness = Harness::Claude;
		row.tool_calls = 9;
		row.turns = 9;
		row.grade = grade(&[("claude exited 0", 1)]);
	});
	let claude_end = [
		line(&json!({ "type": "rate_limit_event", "rate_limit_info": { "status": "allowed", "rateLimitType": "five_hour" } })),
		line(&json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "content": "  429: return retry(429)" }] } })),
		line(&json!({ "type": "result", "subtype": "success", "is_error": false, "num_turns": 9, "result": "Fixed the 401 path." })),
	]
	.concat();
	for row in [&passed, &failed_task] {
		let mut counted = with_transcript(row, &omp_end, true);
		counted.stderr = String::new();
		assert_eq!(kind(&counted), None);
	}
	assert_eq!(kind(&with_transcript(&a4, &claude_end, true)), None);
	// The same runs ended by the trial's limit: a timeout, not a sign-in failure or a rate limit.
	let timed_out = ResultRow { grade: grade(&[("omp exited 0", 0), ("timeout", 0)]), ..failed_task };
	assert_eq!(kind(&with_transcript(&timed_out, &omp_end, true)), None);
	let killed_a4 = ResultRow { grade: grade(&[("claude exited 0", 0), ("timeout", 0)]), ..a4 };
	let first_two = claude_end.split('\n').take(2).collect::<Vec<_>>().join("\n");
	assert_eq!(kind(&with_transcript(&killed_a4, &first_two, true)), None);
}

#[test]
fn a_trial_killed_at_the_limit_before_its_first_response_is_graded_not_no_response_evl_3() {
	let killed = fake_row(|row| {
		row.grade = grade(&[("omp exited 0", 0), ("timeout", 0)]);
		row.turns = 0;
		row.tool_calls = 0;
		row.hidden_passed = Some(false);
	});
	assert_eq!(kind(&outcome(Some(killed))), None);
}

#[test]
fn done_trials_are_skipped_infra_errors_retry_up_to_the_limit() {
	let out = temp_dir("jocasta-bench");
	let tasks = [stub_task("t1", TaskLanguage::Go), stub_task("t2", TaskLanguage::Go)];
	let order = plan_trials(&tasks, &arms(&["a0"]), 2, 482);
	let (first, second, third) = (order[0].clone(), order[1].clone(), order[2].clone());
	let row = |trial: &Trial, extra: Value| {
		let mut value = json!({ "task": trial.task, "arm": trial.arm, "rep": trial.rep, "index": trial.index });
		value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
		value.to_string()
	};
	fs::write(out.path().join("results.jsonl"), format!("{}\n", row(&first, json!({ "attempt": 1 })))).unwrap();
	let errors: Vec<String> =
		[&second, &third, &third, &third, &first].iter().map(|trial| row(trial, json!({ "kind": "setup" }))).collect();
	fs::write(out.path().join("errors.jsonl"), format!("{}\n", errors.join("\n"))).unwrap();
	let state = read_resume_state(out.path()).unwrap();
	assert_eq!(state.done, HashSet::from([trial_key(&first)]));
	assert_eq!(state.failures.get(&trial_key(&third)), Some(&MAX_ATTEMPTS));
	let pending = pending_trials(&order, &state, MAX_ATTEMPTS);
	let expected: Vec<&Trial> = order.iter().filter(|trial| **trial != first && **trial != third).collect();
	assert_eq!(pending.pending.iter().collect::<Vec<_>>(), expected);
	assert_eq!(pending.exhausted, [third]);
	let empty = read_resume_state(temp_dir("jocasta-bench").path()).unwrap();
	assert_eq!(pending_trials(&order, &empty, MAX_ATTEMPTS).pending, order);
}
