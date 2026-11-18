//! Ported from `test/evals/runner.test.ts`. The TypeScript fixtures (`fixtures/{omp,claude,sleeper}.ts` and the `bun -e`
//! one-liners its tests used as shell commands) are the stand-ins of `evals/stand_in.rs`: `jocasta-bench stand-in <mode>`.
//! "bun run evals needs JOCASTA_EVALS_REAL, refuses CI, and needs omp on PATH" is `run.ts`'s case and lives with
//! `evals/run.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::cases::load_cases;
use super::harness::{HarnessResponse, Turn, Usage, assistant_text, read_claude_response, read_response};
use super::runner::{
	Check, EvalCase, MEASUREMENT_NOTES, ResultRow, RunOptions, TrialOptions, count_findings, grade_check,
	make_trial_repo, process_env, run_evals, trial_args,
};
use super::stand_in::{exe, launcher};
use super::tasks_tests::write_files;
use super::variants::{Harness, Variant, default_variant};
use crate::lib::git::git;
use jocasta::shared::jsonl::read_jsonl_file;
use jocasta::shared::proctree::pid_alive;
use jocasta::shared::testing::{TempJocasta, temp_dir, temp_jocasta};

/// A fresh folder under the test sandbox, left for the sandbox to remove with the process.
pub(super) fn temp() -> PathBuf {
	let dir = temp_dir("jocasta-evals");
	let path = dir.path().to_path_buf();
	std::mem::forget(dir);
	path
}

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut all = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	all.extend(args);
	git(dir, &all, None).unwrap()
}

/// The stand-in omp's log: one JSON line per call (`useOmpLog`).
fn omp_log(jocasta: &mut TempJocasta) -> PathBuf {
	let log = jocasta.root().join("omp-calls.jsonl");
	jocasta.set_var("JOCASTA_TEST_OMP_LOG", Some(&log.display().to_string()));
	log
}

fn calls(log: &Path) -> Vec<Value> {
	read_jsonl_file(log).unwrap()
}

fn omp() -> Vec<String> {
	launcher("omp")
}

fn options(launcher: Vec<String>) -> TrialOptions {
	TrialOptions { launcher, ..TrialOptions::default() }
}

async fn run_with(cases: &Path, out: &Path, reps: u64, trial: TrialOptions) -> Vec<ResultRow> {
	run_evals(RunOptions {
		trial,
		cases_dir: vec![cases.to_path_buf()],
		out_dir: out.to_path_buf(),
		reps,
		on_row: None,
		pace: None,
	})
	.await
	.unwrap()
}

async fn run_err(cases: &Path, out: &Path, trial: TrialOptions) -> String {
	run_evals(RunOptions {
		trial,
		cases_dir: vec![cases.to_path_buf()],
		out_dir: out.to_path_buf(),
		reps: 1,
		on_row: None,
		pace: None,
	})
	.await
	.expect_err("the run fails")
	.to_string()
}

fn to_json(rows: &[ResultRow]) -> Vec<Value> {
	rows.iter().map(|row| serde_json::to_value(row).unwrap()).collect()
}

fn argv(call: &Value) -> Vec<String> {
	call["argv"].as_array().unwrap().iter().map(|arg| arg.as_str().unwrap().to_owned()).collect()
}

/// The grandchild's pid from `file`, and whether it is gone (given a moment: the platform reaps asynchronously).
fn gone(pid_file: &Path) -> bool {
	let pid: u32 = fs::read_to_string(pid_file).unwrap().trim().parse().unwrap();
	let until = Instant::now() + Duration::from_secs(5);
	while pid_alive(pid) && Instant::now() < until {
		std::thread::sleep(Duration::from_millis(100));
	}
	!pid_alive(pid)
}

fn quoted_exe() -> String {
	format!("\"{}\"", exe())
}

fn stand_in_command(rest: &str) -> String {
	format!("{} stand-in {rest}", quoted_exe())
}

fn json_text(text: &str) -> String {
	serde_json::to_string(text).unwrap()
}

#[tokio::test]
async fn each_trial_runs_omp_in_a_fresh_fixture_repo_and_writes_results_transcripts_and_the_report() {
	let mut jocasta = temp_jocasta();
	let log = omp_log(&mut jocasta);
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[
			(
				"greet/case.yaml",
				"prompt: 'Write greet.txt: hello acme'\ntags: [write]\nchecks:\n  - file: greet.txt\n    contains: hello acme\n  - file: seed.txt\n",
			),
			("greet/fixture/seed.txt", "seed\n"),
			(
				"wrong/case.yaml",
				"prompt: 'Write wrong.txt: something else'\nchecks:\n  - file: wrong.txt\n    contains: expected text\n",
			),
		],
	);
	let out = temp().join("run");
	let rows = run_with(cases.path(), &out, 2, options(omp())).await;

	let written: Vec<Value> = read_jsonl_file(&out.join("results.jsonl")).unwrap();
	assert_eq!(written, to_json(&rows));
	let names: Vec<String> =
		written.iter().map(|row| format!("{}-{}", row["case"].as_str().unwrap(), row["rep"])).collect();
	assert_eq!(names, ["greet-0", "greet-1", "wrong-0", "wrong-1"]);
	for row in &written {
		let mut keys: Vec<&String> = row.as_object().unwrap().keys().collect();
		keys.sort();
		assert_eq!(
			keys,
			[
				"case",
				"findings",
				"grade",
				"harness",
				"harnessVersion",
				"hiddenPassed",
				"linesAdded",
				"linesRemoved",
				"model",
				"passed",
				"prompts",
				"rep",
				"requests",
				"startedAt",
				"toolCalls",
				"totalUsage",
				"turns",
				"usage",
				"variant",
				"wallMs"
			]
		);
		assert_eq!(
			[&row["harness"], &row["harnessVersion"], &row["prompts"], &row["hiddenPassed"], &row["findings"]],
			[&json!("omp"), &json!("omp/0.0.0-stub"), &json!(1), &Value::Null, &Value::Null]
		);
		assert_eq!(row["model"], "stub-model");
		assert_eq!(row["variant"], "default");
		assert_eq!([&row["turns"], &row["toolCalls"], &row["requests"]], [&json!(2), &json!(1), &json!(4)]);
		assert!(row["wallMs"].as_f64().unwrap() > 0.0);
		// The written file is one new line; the overlay lives outside the repo.
		assert_eq!([&row["linesAdded"], &row["linesRemoved"]], [&json!(1), &json!(0)]);
		assert!(
			out.join("transcripts").join(format!("{}-{}.jsonl", row["case"].as_str().unwrap(), row["rep"])).exists()
		);
	}
	let (greet, wrong) = (&written[0], &written[2]);
	assert_eq!(greet["passed"], true);
	assert_eq!(
		greet["grade"],
		json!({ "omp exited 0": 1, "file: greet.txt contains \"hello acme\"": 1, "file: seed.txt": 1 })
	);
	assert_eq!(
		greet["usage"],
		json!({ "input": 1100, "output": 20, "cacheRead": 800, "cacheWrite": 0, "totalTokens": 1920, "cost": 1 })
	);
	assert_eq!(
		greet["totalUsage"],
		json!({ "input": 1200, "output": 30, "cacheRead": 1200, "cacheWrite": 0, "totalTokens": 2430, "cost": 1.25 })
	);
	assert_eq!(wrong["passed"], false);
	assert_eq!(wrong["grade"]["file: wrong.txt contains \"expected text\""], 0);

	// omp ran in the trial repo on its own branch, headless, with the memory-off overlay and nothing else.
	let all = calls(&log);
	let first = &all[0];
	let args = argv(first);
	assert_eq!(args[0], "--config");
	assert!(args[1].ends_with("eval-run.yml"));
	assert_eq!(args[2..], ["-p", "Write greet.txt: hello acme", "--mode", "json"]);
	assert_eq!(first["overlay"], json!({ "memory": { "backend": "off" } }));
	assert!(first["cwd"].as_str().unwrap().ends_with("repo"));
	// No branch names its case: a benchmark task's id ends in its fix's PR number, and git output shows the branch (0508).
	for call in &all {
		let branch = call["branch"].as_str().unwrap();
		assert!(branch.starts_with("eval/"));
		assert!(!branch.contains("greet") && !branch.contains("wrong"));
	}

	let report = fs::read_to_string(out.join("report.html")).unwrap();
	assert!(
		report
			.contains("omp/0.0.0-stub, variant default: 2 cases × 2 reps. Passed 2 of 4 trials: 50.0% ± 50.0 points.")
	);
	assert!(report.contains("href=\"transcripts/wrong-1.jsonl\""));
	assert!(report.contains("<th>Total tokens</th><th>Requests</th><th>Turns</th><th>Tool calls</th>"));
	for note in MEASUREMENT_NOTES {
		assert!(report.contains(note));
	}
}

#[tokio::test]
async fn with_a_log_folder_lines_log_holds_the_per_file_counts_the_row_sums_0508() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[
			("greet/case.yaml", "prompt: 'Write greet.txt: hello acme'\nchecks:\n  - file: greet.txt\n"),
			("greet/fixture/seed.txt", "seed\n"),
		],
	);
	let logs = temp().join("logs");
	let trial = TrialOptions { log_dir: Some(logs.clone()), ..options(omp()) };
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert_eq!(fs::read_to_string(logs.join("lines.log")).unwrap().trim(), "1\t0\tgreet.txt");
	assert_eq!([rows[0].lines_added, rows[0].lines_removed], [1, 0]);
}

#[tokio::test]
async fn a_variants_flags_go_to_omp_and_its_settings_layer_over_the_memory_off_overlay() {
	let mut jocasta = temp_jocasta();
	let log = omp_log(&mut jocasta);
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[(
			"branch/case.yaml",
			"prompt: Say done\nsetup: echo made > made.txt\nchecks:\n  - file: made.txt\n    contains: made\n  - git_clean: true\n",
		)],
	);
	let prompt = temp().join("acme.md");
	fs::write(&prompt, "Be brief.\n").unwrap();
	let settings = json!({ "memory": { "backend": "local", "extra": 1 }, "modelRoles": { "default": "acme/model-a" } });
	let variant = Variant {
		label: "lean+acme".to_owned(),
		append_system_prompt: Some(prompt.display().to_string()),
		extensions: vec!["/ext/a.ts".to_owned()],
		tools: Some(vec!["read".to_owned(), "bash".to_owned()]),
		settings: settings.as_object().cloned(),
		..default_variant()
	};
	let trial = TrialOptions { variant: Some(variant), ..options(omp()) };
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert_eq!(
		rows.iter()
			.map(|row| (row.variant.clone(), row.passed, row.lines_added, row.lines_removed))
			.collect::<Vec<_>>(),
		[("lean+acme".to_owned(), true, 0, 0)]
	);
	let call = &calls(&log)[0];
	let args = argv(call);
	assert_eq!(args[0], "--append-system-prompt");
	// The trial's copy, its leading comment stripped (0577).
	assert!(args[1].ends_with("system-prompt.md"));
	assert_eq!(args[2..6], ["-e", "/ext/a.ts", "--tools", "read,bash"]);
	assert_eq!(args[6], "--config");
	assert!(args[7].ends_with("eval-run.yml"));
	assert_eq!(args[8..], ["-p", "Say done", "--mode", "json"]);
	// The variant wins where both set a key.
	assert_eq!(call["overlay"], settings);
}

#[tokio::test]
async fn a_prewalk_trials_row_records_whether_prewalks_switch_happened_read_from_the_session_files_0608() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	write_files(cases.path(), &[("p/case.yaml", "prompt: Say done\nchecks:\n  - git_clean: true\n")]);
	let run = |variant: Variant| {
		let trial = TrialOptions { variant: Some(variant), ..options(omp()) };
		let out = temp().join("run");
		let cases = cases.path().to_path_buf();
		async move { run_with(&cases, &out, 1, trial).await }
	};
	let switched =
		run(Variant { label: "f-prewalk-clean".to_owned(), args: vec!["--prewalk".to_owned()], ..default_variant() })
			.await;
	assert_eq!(switched[0].prewalk_switched, Some(true));
	let plain = run(default_variant()).await;
	assert!(to_json(&plain)[0].get("prewalkSwitched").is_none());
}

#[tokio::test]
async fn a_failing_omp_fails_the_trial() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	write_files(cases.path(), &[("fail/case.yaml", "prompt: Fail now\nchecks:\n  - git_clean: true\n")]);
	let rows = run_with(cases.path(), &temp().join("run"), 1, options(omp())).await;
	assert_eq!(Value::Object(rows[0].grade.clone()), json!({ "omp exited 0": 0, "git_clean: true": 1 }));
	assert!(!rows[0].passed);
}

#[tokio::test]
async fn a_trial_past_its_limit_is_killed_with_its_process_tree_and_graded_failed_with_a_timeout() {
	let mut jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	write_files(cases.path(), &[("stuck/case.yaml", "prompt: Never finish\nchecks:\n  - git_clean: true\n")]);
	let pid_file = temp().join("sleeper.pid");
	jocasta.set_var("JOCASTA_TEST_SLEEPER_PID_FILE", Some(&pid_file.display().to_string()));
	let started = Instant::now();
	let trial = TrialOptions { timeout_ms: Some(1_500), ..options(launcher("sleeper")) };
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert!(started.elapsed() < Duration::from_secs(30));
	assert_eq!(rows.len(), 1);
	assert!(!rows[0].passed);
	assert_eq!(rows[0].harness_version, "omp/0.0.0-sleeper");
	assert_eq!(Value::Object(rows[0].grade.clone()), json!({ "omp exited 0": 0, "timeout": 0, "git_clean: true": 1 }));
	// The grandchild omp started is gone too.
	assert!(gone(&pid_file));
}

#[tokio::test]
async fn a_launcher_whose_version_fails_stops_the_run_before_any_trial() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	write_files(cases.path(), &[("c/case.yaml", "prompt: p\nchecks:\n  - git_clean: true\n")]);
	let message = run_err(cases.path(), &temp(), options(launcher("no-such-omp"))).await;
	assert!(message.contains("--version failed"), "{message}");
}

#[tokio::test]
async fn hidden_checks_run_on_a_clean_clone_of_the_result_with_hidden_copied_in_never_in_the_agents_checkout() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	let command = json_text(&stand_in_command("file-is out.txt hello"));
	let good = format!(
		"prompt: 'Write out.txt: hello'\nchecks:\n  - file: grade.ts\n    absent: true\nhidden_checks:\n  - command: {command}\n"
	);
	let bad =
		format!("prompt: 'Write out.txt: hi'\nchecks:\n  - file: out.txt\nhidden_checks:\n  - command: {command}\n");
	write_files(
		cases.path(),
		&[
			("good/case.yaml", &good),
			("good/hidden/grade.ts", "x\n"),
			("bad/case.yaml", &bad),
			("bad/hidden/grade.ts", "x\n"),
		],
	);
	let rows = run_with(cases.path(), &temp().join("run"), 1, options(omp())).await;
	let label = format!("hidden: command: {}", stand_in_command("file-is out.txt hello"));
	let (bad, good) = (&rows[0], &rows[1]);
	assert_eq!(
		Value::Object(good.grade.clone()),
		json!({ "omp exited 0": 1, "file: grade.ts absent": 1, label.clone(): 1 })
	);
	assert_eq!([good.hidden_passed, Some(good.passed)], [Some(true), Some(true)]);
	assert_eq!(bad.grade[&label], 0);
	assert_eq!([bad.hidden_passed, Some(bad.passed)], [Some(false), Some(false)]);
	// The grader isn't part of the agent's change.
	assert_eq!([good.lines_added, good.lines_removed], [1, 0]);
}

#[test]
fn a_hidden_folder_without_hidden_checks_is_refused() {
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[("c/case.yaml", "prompt: p\nchecks:\n  - git_clean: true\n"), ("c/hidden/grade.ts", "")],
	);
	let message = load_cases(cases.path()).unwrap_err().to_string();
	assert!(message.contains("hidden/ folder but no hidden_checks"), "{message}");
}

#[tokio::test]
async fn plan_first_follow_up_turns_with_c_the_profile_and_the_pacer_0482() {
	let mut jocasta = temp_jocasta();
	let log = omp_log(&mut jocasta);
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[("sum/case.yaml", "prompt: 'Write sum.txt: 3'\nchecks:\n  - file: sum.txt\n    contains: '3'\n")],
	);
	let paced = Arc::new(std::sync::atomic::AtomicU64::new(0));
	let counter = paced.clone();
	let variant = Variant {
		label: "a3".to_owned(),
		model: Some("acme/model-a".to_owned()),
		plan_first: true,
		followups: vec!["Review git diff {base}".to_owned()],
		..default_variant()
	};
	let trial = TrialOptions { variant: Some(variant), profile: Some("jocasta-evals".to_owned()), ..options(omp()) };
	let rows = run_evals(RunOptions {
		trial,
		cases_dir: vec![cases.path().to_path_buf()],
		out_dir: temp().join("run"),
		reps: 2,
		on_row: None,
		pace: Some(Arc::new(move || {
			counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async {})
		})),
	})
	.await
	.unwrap();
	assert_eq!(paced.load(std::sync::atomic::Ordering::SeqCst), 2);
	let summary: Vec<(bool, u64, u64, Value)> = rows
		.iter()
		.map(|row| (row.passed, row.prompts, row.turns, serde_json::to_value(row.findings).unwrap()))
		.collect();
	let findings = json!({ "total": 2, "blocking": 1 });
	assert_eq!(summary, [(true, 2, 4, findings.clone()), (true, 2, 4, findings)]);
	let all = calls(&log);
	let first = argv(&all[0]);
	assert_eq!(
		first[..8],
		[
			"--profile",
			"jocasta-evals",
			"--model",
			"acme/model-a",
			"--plan-yolo",
			"--plan-yolo-into",
			"acme/model-a",
			"--config"
		]
	);
	assert!(first[8].ends_with("eval-run.yml"));
	assert_eq!(first[9..], ["-p", "Write sum.txt: 3", "--mode", "json"]);
	// The follow-up continues the session, without plan-first, with the fixture commit for {base}.
	let second = argv(&all[1]);
	assert_eq!(second[..5], ["--profile", "jocasta-evals", "-c", "--model", "acme/model-a"]);
	let review = &second[second.len() - 3];
	assert!(review.starts_with("Review git diff ") && review.len() == "Review git diff ".len() + 40, "{review}");
	assert_eq!(all[1]["cwd"], all[0]["cwd"]);
}

#[tokio::test]
async fn a_claude_code_variant_runs_claude_p_with_stream_json_and_reads_its_result_events_0482_e4() {
	let mut jocasta = temp_jocasta();
	let log = omp_log(&mut jocasta);
	let cases = temp_dir("jocasta-evals");
	write_files(
		cases.path(),
		&[("c/case.yaml", "prompt: 'Write a.txt: hi'\nchecks:\n  - file: a.txt\n    contains: hi\n")],
	);
	let variant = Variant {
		label: "a4".to_owned(),
		harness: Harness::Claude,
		model: Some("claude-stub".to_owned()),
		args: vec!["--permission-mode".to_owned(), "bypassPermissions".to_owned()],
		..default_variant()
	};
	let trial = TrialOptions {
		variant: Some(variant),
		profile: Some("ignored-for-claude".to_owned()),
		..options(launcher("claude"))
	};
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert_eq!(
		argv(&calls(&log)[0]),
		[
			"--model",
			"claude-stub",
			"--permission-mode",
			"bypassPermissions",
			"-p",
			"Write a.txt: hi",
			"--output-format",
			"stream-json",
			"--verbose"
		]
	);
	let row = &rows[0];
	assert_eq!(
		(row.harness, row.harness_version.as_str(), row.model.as_deref(), row.turns, row.tool_calls),
		(Harness::Claude, "2.1.296 (Claude Code)", Some("claude-stub"), 2, 1)
	);
	assert_eq!(Value::Object(row.grade.clone()), json!({ "claude exited 0": 1, "file: a.txt contains \"hi\"": 1 }));
	assert_eq!(
		serde_json::to_value(row.total_usage).unwrap(),
		json!({ "input": 10, "output": 20, "cacheRead": 300, "cacheWrite": 40, "totalTokens": 370, "cost": 0.5 })
	);
}

fn trial_case(fixture: &Path, setup: String, tag: &str) -> EvalCase {
	EvalCase {
		id: "trial".to_owned(),
		dir: fixture.to_path_buf(),
		prompt: "p".to_owned(),
		checks: vec![Check { git_clean: Some(true), ..Check::default() }],
		setup: Some(setup),
		fixture: Some(fixture.to_path_buf()),
		fixture_tag: Some(tag.to_owned()),
		..EvalCase::default()
	}
}

#[tokio::test]
async fn the_trial_repo_holds_every_fixture_file_ignored_ones_too_runs_setup_on_a_tagged_commit_and_runs_none_of_setups_hooks()
 {
	let _jocasta = temp_jocasta();
	let fixture = temp_dir("jocasta-evals");
	// Upstream tracks internal/gh although `gh` (its binary) is ignored, as cli/cli does; setup installs hooks that
	// refuse every commit, husky's way (`core.hooksPath`) and simple-git-hooks' way (`.git/hooks`).
	write_files(fixture.path(), &[(".gitignore", "gh\n"), ("internal/gh/gh.go", "package gh\n")]);
	// setuptools-scm reads the version at setup: the commit and its tag must both be there by then.
	let setup = format!("git describe --tags --exact-match && {}", stand_in_command("hooks"));
	let eval_case = trial_case(fixture.path(), setup, "v2.3.0");
	let root = temp_dir("jocasta-evals");
	let made = make_trial_repo(&eval_case, 1, root.path(), &process_env(), None, None).await.unwrap();
	let copy = temp().join("copy");
	git(root.path(), &["clone", "-q", &made.repo.display().to_string(), &copy.display().to_string()], None).unwrap();
	assert!(copy.join("internal").join("gh").join("gh.go").exists());
	// The tag moved onto the commit that folds setup's output in, so the clean clone's checkout is still at it.
	assert_eq!(git(&copy, &["describe", "--tags"], None).unwrap().trim(), "v2.3.0");
	git(&made.repo, &["commit", "-q", "--allow-empty", "-m", "the agent's commit"], None).unwrap();
}

#[tokio::test]
async fn a_setup_past_its_limit_is_ended_with_its_tree_and_the_trial_is_infra_0574_amended() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	let pid_file = temp().join("setup-grandchild.pid");
	let setup = stand_in_command(&format!("hang \"{}\"", pid_file.display().to_string().replace('\\', "/")));
	let case = json!({ "prompt": "p", "setup": setup, "checks": [{ "git_clean": true }] });
	write_files(cases.path(), &[("slowsetup/case.yaml", &case.to_string())]);
	let started = Instant::now();
	let trial = TrialOptions { setup_timeout_ms: Some(2_000), ..options(omp()) };
	let message = run_err(cases.path(), &temp().join("run"), trial).await;
	assert!(message.starts_with("slowsetup: setup `") && message.ends_with("` killed at its time limit"), "{message}");
	assert!(started.elapsed() < Duration::from_secs(40));
	assert!(gone(&pid_file));
}

fn committed_repo(dir: &Path) -> PathBuf {
	sh(dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("app.txt"), "version 2 for client-a\n").unwrap();
	sh(dir, &["add", "-A"]);
	sh(dir, &["commit", "-q", "-m", "init"]);
	dir.to_path_buf()
}

#[tokio::test]
async fn the_ten_read_only_end_state_grader_cases() {
	let dir = temp_dir("jocasta-evals-grader");
	let shared = committed_repo(dir.path());
	let cases: [(&str, Check, bool); 10] = [
		("a command that exits 0", Check { command: Some("git rev-parse HEAD".into()), ..Check::default() }, true),
		(
			"a command that fails",
			Check { command: Some("git rev-parse no-such-ref".into()), ..Check::default() },
			false,
		),
		("a file that exists", Check { file: Some("app.txt".into()), ..Check::default() }, true),
		("a file that is missing", Check { file: Some("missing.txt".into()), ..Check::default() }, false),
		(
			"a file with the text",
			Check { file: Some("app.txt".into()), contains: Some("version 2".into()), ..Check::default() },
			true,
		),
		(
			"a file without the text",
			Check { file: Some("app.txt".into()), contains: Some("version 3".into()), ..Check::default() },
			false,
		),
		(
			"a missing file that should be absent",
			Check { file: Some("missing.txt".into()), absent: Some(true), ..Check::default() },
			true,
		),
		(
			"a file that should be absent",
			Check { file: Some("app.txt".into()), absent: Some(true), ..Check::default() },
			false,
		),
		("a clean tree expected clean", Check { git_clean: Some(true), ..Check::default() }, true),
		("a clean tree expected dirty", Check { git_clean: Some(false), ..Check::default() }, false),
	];
	for (name, check, expected) in cases {
		assert_eq!(grade_check(&check, &shared, None, None).await.unwrap(), expected, "{name}");
	}
}

#[tokio::test]
async fn git_clean_sees_uncommitted_changes() {
	let dir = temp_dir("jocasta-evals");
	let repo = committed_repo(dir.path());
	fs::write(repo.join("app.txt"), "changed\n").unwrap();
	let clean = |flag| Check { git_clean: Some(flag), ..Check::default() };
	assert!(!grade_check(&clean(true), &repo, None, None).await.unwrap());
	assert!(grade_check(&clean(false), &repo, None, None).await.unwrap());
}

#[tokio::test]
async fn a_hidden_check_past_the_grading_limit_is_ended_with_its_tree_and_grades_0_and_the_trial_counts() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	let pid_file = temp().join("grandchild.pid");
	let hang = stand_in_command(&format!("hang \"{}\"", pid_file.display().to_string().replace('\\', "/")));
	let case = json!({
		"prompt": "Write out.txt: hello",
		"checks": [{ "git_clean": false }],
		"hidden_checks": [{ "command": hang }],
	});
	write_files(cases.path(), &[("hang/case.yaml", &case.to_string())]);
	let started = Instant::now();
	let trial = TrialOptions { grading_timeout_ms: Some(2_000), ..options(omp()) };
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert!(started.elapsed() < Duration::from_secs(40));
	let row = &rows[0];
	assert_eq!(row.grade["omp exited 0"], 1);
	assert_eq!(row.grade[&format!("hidden: command: {hang}")], 0);
	assert_eq!(row.grade["hidden: timeout"], 0);
	assert_eq!(row.hidden_passed, Some(false));
	assert!(gone(&pid_file));
}

#[tokio::test]
async fn a_grading_setup_the_change_broke_grades_0_and_one_that_fails_on_the_fixture_commit_too_is_infra() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	let setup = stand_in_command("file-is dep.txt ok");
	let broke = json!({
		"prompt": "Write dep.txt: broken",
		"setup": setup,
		"checks": [{ "git_clean": false }],
		"hidden_checks": [{ "command": "git --version" }],
	});
	write_files(cases.path(), &[("broke/case.yaml", &broke.to_string()), ("broke/fixture/dep.txt", "ok\n")]);
	let rows = run_with(cases.path(), &temp().join("run"), 1, options(omp())).await;
	let row = &rows[0];
	assert_eq!(
		Value::Object(row.grade.clone()),
		json!({ "omp exited 0": 1, "git_clean: false": 1, "hidden: setup failed": 0 })
	);
	assert_eq!([row.hidden_passed, Some(row.passed)], [Some(false), Some(false)]);

	// Setup passes before the harness and fails after it on any commit: the setup itself is broken. The harness
	// writes the marker into the trial root, beside the repo and the grading clones.
	let infra = temp_dir("jocasta-evals");
	let flaky = json!({
		"prompt": "Write ../broken: x",
		"setup": stand_in_command("missing ../broken"),
		"checks": [{ "git_clean": true }],
		"hidden_checks": [{ "command": "git --version" }],
	});
	write_files(infra.path(), &[("flaky/case.yaml", &flaky.to_string())]);
	let message = run_err(infra.path(), &temp().join("run"), options(omp())).await;
	assert!(message.starts_with("flaky: setup `") && message.contains("` failed"), "{message}");
}

#[tokio::test]
async fn the_fixture_commit_confirmation_runs_within_the_same_grading_limit_and_cut_there_grades_hidden_timeout_evl_5()
{
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	// 1.5 s each way: the trial's setup fails inside the 2.5 s limit; the confirmation would pass, but past it. Real
	// time on purpose: the limit is the platform clock ending a child process tree.
	let setup = stand_in_command("file-is dep.txt ok 1500");
	let slow = json!({
		"prompt": "Write dep.txt: broken",
		"setup": setup,
		"checks": [{ "git_clean": false }],
		"hidden_checks": [{ "command": "git --version" }],
	});
	write_files(cases.path(), &[("slow/case.yaml", &slow.to_string()), ("slow/fixture/dep.txt", "ok\n")]);
	let trial = TrialOptions { grading_timeout_ms: Some(2_500), ..options(omp()) };
	let rows = run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert_eq!(
		Value::Object(rows[0].grade.clone()),
		json!({ "omp exited 0": 1, "git_clean: false": 1, "hidden: timeout": 0 })
	);
	assert_eq!(rows[0].hidden_passed, Some(false));
}

#[tokio::test]
async fn grading_takes_the_commit_the_agent_left_on_a_branch_of_its_own_too_evl_8() {
	let _jocasta = temp_jocasta();
	let cases = temp_dir("jocasta-evals");
	let case = json!({
		"prompt": "Fix it",
		"checks": [{ "git_clean": false }],
		"hidden_checks": [{ "command": stand_in_command("file-is out.txt hello") }],
	});
	write_files(cases.path(), &[("branch/case.yaml", &case.to_string())]);
	let rows = run_with(cases.path(), &temp().join("run"), 1, options(launcher("branch"))).await;
	assert_eq!(rows[0].hidden_passed, Some(true));
}

#[tokio::test]
async fn ponytails_leading_comment_stays_out_of_the_prompt_every_arm_appends_0577() {
	let mut jocasta = temp_jocasta();
	let log = omp_log(&mut jocasta);
	let cases = temp_dir("jocasta-evals");
	write_files(cases.path(), &[("c/case.yaml", "prompt: p\nchecks:\n  - git_clean: true\n")]);
	let prompt = temp().join("ponytail.md");
	fs::write(&prompt, "<!-- Ponytail, MIT License, example-org -->\n\n# Ponytail, lazy senior dev mode\n\nBe lazy.\n")
		.unwrap();
	let variant = Variant {
		label: "a1".to_owned(),
		append_system_prompt: Some(prompt.display().to_string()),
		..default_variant()
	};
	let trial = TrialOptions { variant: Some(variant), ..options(omp()) };
	run_with(cases.path(), &temp().join("run"), 1, trial).await;
	assert_eq!(calls(&log)[0]["appendedText"], "# Ponytail, lazy senior dev mode\n\nBe lazy.\n");
	assert!(fs::read_to_string(&prompt).unwrap().starts_with("<!-- Ponytail"));
}

#[test]
fn case_files_are_reported_with_file_and_line() {
	let cases = [
		("no check kind", "prompt: p\nchecks:\n  - contains: x\n", "case.yaml:3: checks[0] needs one of"),
		(
			"two check kinds",
			"prompt: p\nchecks:\n  - file: a\n  - command: b\n    git_clean: true\n",
			"case.yaml:4: checks[1] needs one of",
		),
		("an unknown key", "prompt: p\nchecks:\n  - file: a\nsplit: test\n", "case.yaml:4: unknown key `split`"),
		("no checks", "prompt: p\nchecks: []\n", "case.yaml:2: checks must"),
	];
	for (name, source, message) in cases {
		let dir = temp_dir("jocasta-evals");
		write_files(dir.path(), &[("c/case.yaml", source)]);
		let error = load_cases(dir.path()).unwrap_err().to_string();
		assert!(error.contains(message), "{name}: {error}");
	}
}

fn usage(input: f64, total: f64) -> Value {
	json!({ "input": input, "output": 2, "cacheRead": 3, "cacheWrite": 4, "totalTokens": input + 9.0, "cost": { "total": total } })
}

fn totals(counts: [f64; 5], cost: f64) -> Usage {
	let [input, output, cache_read, cache_write, total_tokens] = counts;
	Usage { input, output, cache_read, cache_write, total_tokens, cost }
}

fn lines(entries: &[Value]) -> String {
	entries.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")
}

#[test]
fn read_response_takes_the_model_from_the_responses_sums_usage_counts_turns_and_tool_calls_and_adds_task_subagents() {
	let call = json!({ "type": "toolCall", "id": "c1", "name": "read", "arguments": {} });
	let transcript = lines(&[
		json!({ "type": "session", "version": 3, "id": "s", "cwd": "/repo" }),
		json!({ "type": "message_end", "message": { "role": "user", "content": "hi" } }),
		json!({
			"type": "message_end",
			"message": { "role": "assistant", "model": "model-a", "content": [{ "type": "text", "text": "x" }, call, call], "usage": usage(10.0, 0.5) },
		}),
		json!({
			"type": "message_update",
			"message": { "role": "assistant", "model": "ignored", "content": [call], "usage": usage(99.0, 9.0) },
		}),
		json!({
			"type": "tool_execution_end",
			"toolName": "task",
			"result": { "details": { "results": [{ "requests": 2 }, { "requests": 1 }], "usage": usage(100.0, 1.0) } },
		}),
		// Another tool's details never count, nor does the tool result's own message_end.
		json!({ "type": "tool_execution_end", "toolName": "read", "result": { "details": { "usage": usage(7.0, 7.0) } } }),
		json!({ "type": "message_end", "message": { "role": "toolResult", "details": { "usage": usage(100.0, 1.0) } } }),
		json!({ "type": "message_end", "message": { "role": "assistant", "model": "model-b", "usage": usage(20.0, 0.25) } }),
	]);
	assert_eq!(
		read_response(&transcript),
		HarnessResponse {
			model: Some("model-b".to_owned()),
			usage: totals([30.0, 4.0, 6.0, 8.0, 48.0], 0.75),
			total_usage: totals([130.0, 6.0, 9.0, 12.0, 157.0], 1.75),
			requests: 5,
			turns: 2,
			tool_calls: 2,
		}
	);
}

#[test]
fn count_findings_counts_p0_to_p3_markers_p0_and_p1_block() {
	let counted = count_findings("[P0] a\n[P1] b\n[P2] c\n[P3] d\n[P4] not one");
	assert_eq!((counted.total, counted.blocking), (4, 2));
	let none = count_findings("No findings.");
	assert_eq!((none.total, none.blocking), (0, 0));
}

#[test]
fn read_claude_response_sums_result_events_over_turns_and_counts_tool_use_blocks() {
	let result = |cost: f64| {
		json!({
			"type": "result",
			"num_turns": 3,
			"total_cost_usd": cost,
			"usage": { "input_tokens": 1, "output_tokens": 2, "cache_read_input_tokens": 3, "cache_creation_input_tokens": 4 },
		})
	};
	let transcript = lines(&[
		json!({ "type": "assistant", "message": { "model": "m-1", "content": [{ "type": "tool_use" }, { "type": "text" }] } }),
		result(0.25),
		json!({ "type": "assistant", "message": { "model": "m-2", "content": [{ "type": "tool_use" }] } }),
		result(0.5),
	]);
	let total = totals([2.0, 4.0, 6.0, 8.0, 20.0], 0.75);
	assert_eq!(
		read_claude_response(&transcript),
		HarnessResponse {
			model: Some("m-2".to_owned()),
			usage: total,
			total_usage: total,
			requests: 6,
			turns: 6,
			tool_calls: 2,
		}
	);
}

#[test]
fn a_claude_code_run_killed_at_the_limit_has_no_result_its_turns_are_its_distinct_assistant_messages_evl_3() {
	let block =
		|id: &str, kind: &str| json!({ "type": "assistant", "message": { "id": id, "content": [{ "type": kind }] } });
	let transcript = lines(&[
		json!({ "type": "system", "subtype": "init" }),
		block("msg_1", "text"),
		block("msg_1", "tool_use"),
		block("msg_2", "tool_use"),
	]);
	let response = read_claude_response(&transcript);
	assert_eq!((response.turns, response.requests, response.tool_calls), (2, 2, 2));
}

#[test]
fn a_last_line_torn_by_a_kill_is_skipped_by_every_transcript_reader_dup_1() {
	let omp = format!(
		"{}\n{}",
		json!({ "type": "message_end", "message": { "role": "assistant", "content": [{ "type": "text", "text": "Done." }] } }),
		"{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"te"
	);
	assert_eq!(read_response(&omp).turns, 1);
	assert_eq!(assistant_text(&omp, Harness::Omp), "Done.");
	let claude = format!(
		"{}\n{{\"type\":\"res",
		json!({ "type": "assistant", "message": { "id": "m", "content": [{ "type": "text", "text": "Hi" }] } })
	);
	assert_eq!(read_claude_response(&claude).turns, 1);
	assert_eq!(assistant_text(&claude, Harness::Claude), "Hi");
}

#[test]
fn trial_args_a_claude_follow_up_continues_with_c_and_never_takes_omps_overlay_or_profile() {
	let variant = Variant { harness: Harness::Claude, model: Some("m".to_owned()), ..default_variant() };
	let turn = Turn {
		prompt: "next".to_owned(),
		follow_up: true,
		overlay: "/o.yml".to_owned(),
		profile: Some("p".to_owned()),
		..Turn::default()
	};
	assert_eq!(
		trial_args(&variant, &turn).unwrap(),
		["-c", "--model", "m", "-p", "next", "--output-format", "stream-json", "--verbose"]
	);
}
