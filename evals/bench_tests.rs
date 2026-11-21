//! Ported from `test/evals/arms.test.ts` (the two cases that need `evals/bench_cli.rs`) and `test/evals/tasks.test.ts` (the
//! cases that call `runBench` or `runValidation`). The process rows of `test/evals/args.test.ts` and `compare.test.ts` are
//! `tests/evals_cli.rs`'s.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use regex::Regex;
use serde_json::{Map, Value, json};

use super::bench::{Arm, BenchOptions, Execute, LogFn, RUN_LOCK, TrialJob, TrialOutcome, VersionOf, run_bench};
use super::bench_cli::{DEFAULT_ARMS, mcp_problem};
use super::bench_validate::run_validation;
use super::runner_tests::temp;
use super::stand_in::{exe, launcher};
use super::tasks::{BenchTask, TASK_PROMPT_PREFIX, TaskLanguage, TaskSet, plan_trials, row_key};
use super::tasks_tests::{fake_row, local_task};
use super::variants::{Harness, Variant, default_variant, load_variants};
use crate::lib::args::REPO_ROOT;
use jocasta::shared::jsonl::read_jsonl_file;
use jocasta::shared::testing::{temp_dir, temp_jocasta};

fn task(id: &str, language: TaskLanguage, set: TaskSet) -> BenchTask {
	BenchTask {
		instance_id: id.to_owned(),
		set,
		repo: String::new(),
		repo_url: String::new(),
		base_commit: String::new(),
		problem_statement: String::new(),
		gold_patch: String::new(),
		test_patch: String::new(),
		hidden_test_files: Vec::new(),
		setup_command: String::new(),
		test_command: String::new(),
		language,
		pr_url: String::new(),
		issue_urls: Vec::new(),
		merge_commit: String::new(),
	}
}

/// The main set's mix: 12 Python, 12 TypeScript/JavaScript, 6 Go.
fn main_set() -> Vec<BenchTask> {
	let numbered = |prefix: &str, count: usize, language| {
		(0..count).map(|i| task(&format!("{prefix}-{i}"), language, TaskSet::Main)).collect::<Vec<_>>()
	};
	[
		numbered("py", 12, TaskLanguage::Python),
		numbered("ts", 8, TaskLanguage::TypeScript),
		numbered("js", 4, TaskLanguage::JavaScript),
		numbered("go", 6, TaskLanguage::Go),
		vec![task("spare-0", TaskLanguage::Go, TaskSet::Spare)],
	]
	.concat()
}

#[test]
fn the_main_run_is_30_tasks_x_3_reps_x_7_arms_eq_630_trials_0543() {
	assert_eq!(DEFAULT_ARMS, ["a0", "a1", "a2", "a3", "a4", "a6", "a7"]);
	let arms: Vec<String> = DEFAULT_ARMS.iter().map(|arm| (*arm).to_owned()).collect();
	let main: Vec<BenchTask> = main_set().into_iter().filter(|t| t.set == TaskSet::Main).collect();
	assert_eq!(plan_trials(&main, &arms, 3, 482).len(), 630);
	let variants = REPO_ROOT.join("evals").join("variants");
	for arm in &arms {
		load_variants(&variants, std::slice::from_ref(arm)).unwrap_or_else(|error| panic!("{arm}: {error}"));
	}
}

#[test]
fn _0587_0652_a_layered_arm_refuses_an_omp_folder_that_hides_its_own_server_and_his_other_servers_load_in_every_arm() {
	let held = temp_dir("jocasta-arms");
	let dir = held.path();
	let write = |name: &str, config: Value| fs::write(dir.join(name), config.to_string()).unwrap();
	let file = |name: &str| dir.join(name).display().to_string();
	let layered = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
	assert_eq!(mcp_problem(dir, &layered(&["a6"])), None);
	// His servers, Jocasta's among them, reach every omp arm alike (0652): no arm without a layer refuses them.
	write(
		"mcp.json",
		json!({ "mcpServers": { "jocasta": { "command": "jocasta" }, "whiteboard": { "command": "node" } } }),
	);
	write(".mcp.json", json!({ "mcpServers": { "acme": { "command": "acme" } } }));
	assert_eq!(mcp_problem(dir, &[]), None);
	// A "jocasta" entry, even turned off, hides the layer's own server from A6.
	write("mcp.json", json!({ "mcpServers": { "jocasta": { "command": "jocasta", "enabled": false } } }));
	fs::write(dir.join(".mcp.json"), "not json").unwrap();
	assert_eq!(
		mcp_problem(dir, &layered(&["a6", "f-keep-context"])),
		Some(format!(
			"a6, f-keep-context would lose the layer's own MCP server \"jocasta\": {} has a \"jocasta\" entry, which hides the layer's even turned off. Remove it or use a --profile without it; no trial ran.",
			file("mcp.json")
		))
	);
	// disabledServers hides the layer's of the same name too.
	write(
		"mcp.json",
		json!({ "mcpServers": { "acme": { "command": "acme" } }, "disabledServers": ["acme", "jocasta"] }),
	);
	let problem = mcp_problem(dir, &layered(&["a6"])).unwrap();
	assert!(problem.contains(&format!("{} names \"jocasta\" in disabledServers", file("mcp.json"))), "{problem}");
}

/// A local upstream in a folder of its own (`localTask()`), left for the sandbox to remove.
fn local() -> BenchTask {
	local_task(&temp())
}

fn host() -> Box<dyn Fn() -> Map<String, Value>> {
	Box::new(|| {
		let Value::Object(map) = json!({
			"startedAt": "2026-10-27T10:00:00.000Z",
			"localHour": 10,
			"platform": "win32",
			"osRelease": "10.0",
			"arch": "x64",
			"cpu": "stub",
			"cpuCount": 4,
			"memoryGb": 8,
			"versions": {},
		}) else {
			unreachable!("an object literal")
		};
		map
	})
}

fn stub_arm(name: &str) -> Arm {
	Arm {
		name: name.to_owned(),
		variant: Variant { label: name.to_owned(), ..default_variant() },
		launcher: vec!["omp".to_owned()],
		harness_version: "omp/0.0.0-stub".to_owned(),
	}
}

fn a7_arm() -> Arm {
	Arm {
		name: "a7".to_owned(),
		variant: Variant { label: "a7".to_owned(), harness: Harness::Opencode, ..default_variant() },
		launcher: vec!["opencode".to_owned()],
		harness_version: "1.18.22".to_owned(),
	}
}

/// The options every case starts from: one worker, the stub host, and no log.
fn options(tasks: Vec<BenchTask>, out: &Path, arms: Vec<Arm>, reps: u64) -> BenchOptions {
	let mut options = BenchOptions::new(tasks, temp(), out.to_path_buf(), arms, reps);
	options.workers = 1;
	options.describe_host = Some(host());
	options.log = Some(Arc::new(|_| {}));
	options
}

fn execute<F>(run: F) -> Execute
where
	F: Fn(TrialJob) -> TrialOutcome + Send + Sync + 'static,
{
	Arc::new(move |job| {
		let outcome = run(job);
		Box::pin(async move { outcome })
	})
}

fn row_outcome(job: &TrialJob) -> TrialOutcome {
	TrialOutcome { row: Some(fake_row(|row| row.rep = job.rep)), error: None, stderr: String::new() }
}

fn error_outcome(error: &str) -> TrialOutcome {
	TrialOutcome { row: None, error: Some(error.to_owned()), stderr: String::new() }
}

/// The lines the run logged.
fn recording(options: &mut BenchOptions) -> Arc<Mutex<Vec<String>>> {
	let lines = Arc::new(Mutex::new(Vec::new()));
	let sink = Arc::clone(&lines);
	let log: LogFn = Arc::new(move |line| sink.lock().unwrap().push(line.to_owned()));
	options.log = Some(log);
	lines
}

fn rows(file: &Path) -> Vec<Value> {
	read_jsonl_file(file).unwrap()
}

fn conditions(out: &Path) -> Value {
	serde_json::from_str(&fs::read_to_string(out.join("conditions.json")).unwrap()).unwrap()
}

fn error_of(result: jocasta::shared::errors::Result<u8>) -> String {
	result.expect_err("the run is refused").to_string()
}

#[tokio::test]
async fn run_bench_retries_an_infra_error_later_writes_rows_with_the_benchmarks_fields_and_resumes() {
	let task = local();
	let out = temp().join("run");
	type Called = Vec<(Option<String>, String)>;
	let jobs: Arc<Mutex<Called>> = Arc::default();
	let calls = Arc::new(AtomicUsize::new(0));
	let run = {
		let (jobs, calls) = (Arc::clone(&jobs), Arc::clone(&calls));
		execute(move |job| {
			jobs.lock().unwrap().push((job.options.transcript_name.clone(), job.eval_case.prompt.clone()));
			if calls.fetch_add(1, Ordering::SeqCst) == 0 {
				return error_outcome("acme__widget-7: setup `git --version` failed: boom");
			}
			let name = job.options.transcript_name.clone().unwrap();
			fs::write(
				job.options.out_dir.join("transcripts").join(format!("{name}.jsonl")),
				format!("{}\n", json!({ "text": "read https://github.com/acme/widget/pull/7" })),
			)
			.unwrap();
			let label = job.options.variant.as_ref().unwrap().label.clone();
			TrialOutcome {
				row: Some(fake_row(|row| {
					row.rep = job.rep;
					row.variant = label;
				})),
				error: None,
				stderr: String::new(),
			}
		})
	};
	let make = || {
		let mut options = options(vec![task.clone()], &out, vec![stub_arm("a0"), stub_arm("a1")], 1);
		options.workers = 2;
		options.execute = Some(Arc::clone(&run));
		options
	};
	assert_eq!(run_bench(make()).await.unwrap(), 0);
	assert_eq!(calls.load(Ordering::SeqCst), 3);
	let errors = rows(&out.join("errors.jsonl"));
	assert_eq!(errors.len(), 1);
	assert_eq!(
		(errors[0]["kind"].as_str(), errors[0]["attempt"].as_u64(), errors[0]["harness"].as_str()),
		(Some("setup"), Some(1), Some("omp"))
	);
	let results = rows(&out.join("results.jsonl"));
	assert_eq!(results.len(), 2);
	let key = |row: &Value| {
		row_key(row["task"].as_str().unwrap(), row["arm"].as_str().unwrap(), row["rep"].as_u64().unwrap())
	};
	let retried = results.iter().find(|row| key(row) == key(&errors[0])).unwrap();
	assert_eq!(retried["attempt"], json!(2));
	assert_eq!(retried["language"], json!("JavaScript"));
	assert_eq!(retried["leak"], json!(["https://github.com/acme/widget/pull/7", "github.com/acme/widget/pull/7"]));
	let mut indexes: Vec<u64> = results.iter().map(|row| row["trialIndex"].as_u64().unwrap()).collect();
	indexes.sort_unstable();
	assert_eq!(indexes, [0, 1]);
	let (name, prompt) = jobs.lock().unwrap()[0].clone();
	assert!(Regex::new(r"^acme__widget-7-a[01]-0$").unwrap().is_match(&name.unwrap()));
	assert_eq!(prompt, format!("{TASK_PROMPT_PREFIX}{}", task.problem_statement));
	let held = conditions(&out);
	assert_eq!(held["seed"], json!(482));
	assert_eq!(held["arms"], json!(["a0", "a1"]));
	assert_eq!(held["reps"], json!(1));
	assert_eq!(held["tasks"], json!(["acme__widget-7"]));
	assert_eq!(held["order"].as_array().unwrap().len(), 2);
	assert!(held["sessions"][0]["finishedAt"].is_string());
	// Run again: nothing left to do.
	assert_eq!(run_bench(make()).await.unwrap(), 0);
	assert_eq!(calls.load(Ordering::SeqCst), 3);
	assert_eq!(conditions(&out)["sessions"].as_array().unwrap().len(), 2);
	let mut other = make();
	other.seed = 1;
	assert!(error_of(run_bench(other).await).contains("another plan"));
}

#[tokio::test]
async fn with_two_workers_no_two_trials_of_one_task_run_at_once_the_rest_keeps_the_seeded_order() {
	let tasks = vec![local(), BenchTask { instance_id: "acme__widget-8".to_owned(), ..local() }];
	let arms = vec![stub_arm("a0"), stub_arm("a1"), stub_arm("a2")];
	#[derive(Default)]
	struct Seen {
		running: Vec<String>,
		started: Vec<String>,
		overlap: usize,
		parallel: usize,
	}
	let seen: Arc<Mutex<Seen>> = Arc::default();
	let mut options = options(tasks.clone(), &temp().join("run"), arms, 2);
	options.workers = 2;
	let run: Execute = {
		let seen = Arc::clone(&seen);
		Arc::new(move |job| {
			let seen = Arc::clone(&seen);
			Box::pin(async move {
				let id = job.eval_case.id.clone();
				{
					let mut seen = seen.lock().unwrap();
					if seen.running.contains(&id) {
						seen.overlap += 1;
					}
					seen.running.push(id.clone());
					seen.parallel = seen.parallel.max(seen.running.len());
					seen.started.push(format!("{id}/{}", job.options.transcript_name.clone().unwrap()));
				}
				// Yield some turns, so the other worker starts its trial while this one is still running.
				for _ in 0..10 {
					tokio::task::yield_now().await;
				}
				seen.lock().unwrap().running.retain(|running| *running != id);
				row_outcome(&job)
			})
		})
	};
	options.execute = Some(run);
	assert_eq!(run_bench(options).await.unwrap(), 0);
	let seen = seen.lock().unwrap();
	assert_eq!(seen.overlap, 0);
	assert_eq!(seen.parallel, 2);
	assert_eq!(seen.started.len(), 12);
	let arms: Vec<String> = ["a0", "a1", "a2"].iter().map(|arm| (*arm).to_owned()).collect();
	let order = plan_trials(&tasks, &arms, 2, 482);
	// The first trial in the order starts first; the second worker takes the first one of the other task.
	let first = &order[0];
	assert_eq!(seen.started[0], format!("{}/{}-{}-{}", first.task, first.task, first.arm, first.rep));
	let other = order.iter().find(|trial| trial.task != first.task).unwrap();
	assert_eq!(seen.started[1], format!("{}/{}-{}-{}", other.task, other.task, other.arm, other.rep));
}

#[tokio::test]
async fn a_second_bench_on_the_same_out_is_refused_while_the_first_runs_a_dead_holders_lock_is_taken_over_0574() {
	let out = temp().join("run");
	fs::create_dir_all(&out).unwrap();
	let task = local();
	let make = || {
		let mut options = options(vec![task.clone()], &out, vec![stub_arm("a0")], 1);
		options.execute = Some(execute(|job| row_outcome(&job)));
		options
	};
	// This test's own process stands for a live bench.
	let started_at = "2026-11-14T09:02:00.000Z";
	fs::write(out.join(RUN_LOCK), json!({ "pid": std::process::id(), "startedAt": started_at }).to_string()).unwrap();
	let refused = error_of(run_bench(make()).await);
	let expected = format!(
		r"is in use by a bench \(pid {}, since \d{{4}}-\d\d-\d\d \d\d:\d\d\); stop it or use another --out",
		std::process::id()
	);
	assert!(Regex::new(&expected).unwrap().is_match(&refused), "{refused}");
	assert!(!out.join("results.jsonl").exists());
	// A crashed run's lock: its process is gone.
	let mut child = Command::new(exe()).args(["stand-in", "sleep", "0"]).spawn().unwrap();
	let gone = child.id();
	child.wait().unwrap();
	fs::write(out.join(RUN_LOCK), json!({ "pid": gone, "startedAt": started_at }).to_string()).unwrap();
	assert_eq!(run_bench(make()).await.unwrap(), 0);
	assert!(!out.join(RUN_LOCK).exists());
}

#[tokio::test]
async fn a_resume_with_another_limit_setup_limit_profile_or_harness_version_is_refused_0574() {
	let out = temp().join("run");
	let task = local();
	let make = || {
		let mut options = options(vec![task.clone()], &out, vec![stub_arm("a0")], 1);
		options.timeout_ms = Some(90 * 60_000);
		options.profile = Some("bench".to_owned());
		options.execute = Some(execute(|_| error_outcome("acme__widget-7: setup `x` failed: boom")));
		options
	};
	run_bench(make()).await.unwrap();
	let held = conditions(&out);
	assert_eq!(
		[&held["held"]["setupLimitMinutes"], &held["sessions"][0]["setupLimitMinutes"]],
		[&json!(60), &json!(60)]
	);
	assert_eq!(held["sessions"][0]["gradingLimitMinutes"], json!(60));
	let refusal = format!(
		"{} ran with --timeout 90, setup limit 60 min, profile bench, omp/0.0.0-stub; this resume has",
		out.display()
	);
	let mut slower = make();
	slower.timeout_ms = Some(60 * 60_000);
	assert!(error_of(run_bench(slower).await).contains(&format!("{refusal} --timeout 60;")));
	let mut shorter = make();
	shorter.setup_timeout_ms = Some(30 * 60_000);
	assert!(error_of(run_bench(shorter).await).contains(&format!("{refusal} setup limit 30 min;")));
	let mut bare = make();
	bare.profile = None;
	assert!(error_of(run_bench(bare).await).contains(&format!("{refusal} no profile;")));
	let mut newer = make();
	newer.arms = vec![Arm { harness_version: "omp/0.0.1-stub".to_owned(), ..stub_arm("a0") }];
	assert!(
		error_of(run_bench(newer).await)
			.contains(&format!("{refusal} omp/0.0.1-stub; resume with the same flags or use a new --out"))
	);
	assert_eq!(run_bench(make()).await.unwrap(), 1);
}

#[tokio::test]
async fn an_arm_whose_harness_updated_itself_stops_the_other_arms_run_on_0574() {
	let a7_runs = Arc::new(AtomicUsize::new(0));
	let mut options = options(
		vec![local()],
		&temp().join("run"),
		vec![Arm { harness_version: "1.18.22".to_owned(), ..stub_arm("a7") }, stub_arm("a0")],
		3,
	);
	let lines = recording(&mut options);
	options.execute = Some({
		let a7_runs = Arc::clone(&a7_runs);
		execute(move |job| {
			if job.options.variant.as_ref().is_some_and(|variant| variant.label == "a7") {
				a7_runs.fetch_add(1, Ordering::SeqCst);
			}
			row_outcome(&job)
		})
	});
	let version_of: VersionOf =
		Arc::new(|arm| Ok(if arm.name == "a7" { "1.18.23".to_owned() } else { arm.harness_version.clone() }));
	options.version_of = Some(version_of);
	assert_eq!(run_bench(options).await.unwrap(), 1);
	assert_eq!(a7_runs.load(Ordering::SeqCst), 1);
	let lines = lines.lock().unwrap();
	assert!(lines.contains(
		&"stopped a7: omp --version now says 1.18.23; the run started on 1.18.22 (0482 pins it); 2 a7 trials not run"
			.to_owned()
	));
	assert_eq!(lines.iter().filter(|line| line.starts_with("pass") || line.starts_with("FAIL")).count(), 4);
}

#[tokio::test]
async fn three_sign_in_failures_in_a_row_stop_the_run_with_exit_code_3() {
	let calls = Arc::new(AtomicUsize::new(0));
	let mut options = options(vec![local()], &temp().join("run"), vec![stub_arm("a0")], 5);
	let lines = recording(&mut options);
	options.execute = Some({
		let calls = Arc::clone(&calls);
		execute(move |_| {
			calls.fetch_add(1, Ordering::SeqCst);
			TrialOutcome {
				row: Some(fake_row(|row| {
					row.grade = [("omp exited 0".to_owned(), json!(0))].into_iter().collect();
					row.turns = 0;
					row.tool_calls = 0;
				})),
				error: None,
				stderr: "Error: not logged in; run omp and /login".to_owned(),
			}
		})
	});
	assert_eq!(run_bench(options).await.unwrap(), 3);
	assert_eq!(calls.load(Ordering::SeqCst), 3);
	let last = lines.lock().unwrap().last().cloned().unwrap();
	assert!(Regex::new(r"3 trials in a row could not sign in to omp").unwrap().is_match(&last), "{last}");
}

#[tokio::test]
async fn a_trial_runs_through_run_trial_in_a_worker_thread_with_the_fake_omp_its_root_in_work_logs_kept() {
	let mut jocasta = temp_jocasta();
	let log = jocasta.root().join("omp-calls.jsonl");
	jocasta.set_var("JOCASTA_TEST_OMP_LOG", Some(&log.display().to_string()));
	let out = temp().join("run");
	let work = temp().join("w");
	let mut options = options(vec![local()], &out, vec![Arm { launcher: launcher("omp"), ..stub_arm("a0") }], 1);
	options.work_dir = Some(work.clone());
	assert_eq!(run_bench(options).await.unwrap(), 0);
	let results = rows(&out.join("results.jsonl"));
	let row = &results[0];
	for (field, expected) in [
		("case", json!("acme__widget-7")),
		("task", json!("acme__widget-7")),
		("arm", json!("a0")),
		("rep", json!(0)),
		("attempt", json!(1)),
		("trialIndex", json!(0)),
		("leak", json!([])),
		("model", json!("stub-model")),
		("hiddenPassed", json!(false)),
		("passed", json!(false)),
	] {
		assert_eq!(row[field], expected, "{field}");
	}
	assert_eq!(row["grade"]["omp exited 0"], json!(1));
	assert!(out.join("transcripts").join("acme__widget-7-a0-0.jsonl").exists());
	let logs = out.join("logs").join("acme__widget-7-a0-0");
	let setup = fs::read_to_string(logs.join("setup.log")).unwrap();
	assert!(Regex::new(r"\$ git --version\ncwd: .*\nexit: 0\n").unwrap().is_match(&setup), "{setup}");
	let grade = fs::read_to_string(logs.join("grade.log")).unwrap();
	assert!(grade.contains("stand-in file-is lib.txt fixed"), "{grade}");
	assert!(Regex::new(r"exit: 1\n").unwrap().is_match(&grade), "{grade}");
	// The harness ran in <work>\t??????\repo, a short root.
	let calls: Vec<Value> = read_jsonl_file(&log).unwrap();
	let call = &calls[0];
	let cwd = call["cwd"].as_str().unwrap();
	assert!(Regex::new(r"[\\/]w[\\/]t[^\\/]{6}[\\/]repo$").unwrap().is_match(cwd), "{cwd}");
	assert!(cwd.starts_with(&work.display().to_string()), "{cwd}");
	assert_eq!(conditions(&out)["sessions"][0]["workRoot"], json!(work.display().to_string()));
	// The ledger (0538): omp ran with --session-dir in the root, its session file was saved and priced.
	assert!(call["argv"].as_array().unwrap().iter().any(|arg| arg == "--session-dir"));
	assert!(out.join("sessions").join("acme__widget-7-a0-0").join("stub-session.jsonl").exists());
	let ledger = &row["ledger"];
	for (field, expected) in
		[("sessions", 1.0), ("requests", 1.0), ("toolCalls", 1.0), ("input", 1000.0), ("output", 100.0)]
	{
		assert_eq!(ledger[field].as_f64(), Some(expected), "{field}");
	}
	assert!((ledger["cost"].as_f64().unwrap() - (1000.0 * 4e-6 + 100.0 * 2e-5)).abs() < 1e-12);
	let kinds: Vec<(String, String, String)> = rows(&out.join("ledger.jsonl"))
		.iter()
		.map(|row| {
			let text = |field: &str| row[field].as_str().unwrap().to_owned();
			(text("kind"), text("task"), text("arm"))
		})
		.collect();
	let expected = |kind: &str| (kind.to_owned(), "acme__widget-7".to_owned(), "a0".to_owned());
	assert_eq!(kinds, [expected("request"), expected("tool")]);
	assert_eq!(rows(&out.join("spend.jsonl")).len(), 1);
}

/// An A7 trial: writes an opencode export with one step of `input` tokens on `model` into the trial's sessions folder.
fn a7_execute(model: &'static str, input: u64) -> Execute {
	execute(move |job| {
		let dir: PathBuf = job.options.sessions_dir.clone().unwrap();
		fs::create_dir_all(&dir).unwrap();
		let exported = json!({
			"info": { "id": format!("ses_{}", job.rep) },
			"messages": [{
				"info": { "role": "assistant", "providerID": "anthropic", "modelID": model, "time": { "created": 1 } },
				"parts": [{ "type": "step-finish", "tokens": { "input": input, "output": 0, "reasoning": 0, "cache": { "read": 0, "write": 0 } } }],
			}],
		});
		fs::write(dir.join(format!("ses_{}.json", job.rep)), exported.to_string()).unwrap();
		TrialOutcome {
			row: Some(fake_row(|row| {
				row.rep = job.rep;
				row.harness = Harness::Opencode;
				row.grade = [("opencode exited 0".to_owned(), json!(1))].into_iter().collect();
			})),
			error: None,
			stderr: String::new(),
		}
	})
}

#[tokio::test]
async fn a7_stops_once_its_priced_spend_reaches_the_limit_the_other_arms_run_on_exit_code_4_0537() {
	let out = temp().join("run");
	// 100,000 input tokens on claude-opus-5-5 at $4/M: $0.40 a trial; the limit $1 stops A7 after 3 trials.
	let mut options = options(vec![local()], &out, vec![a7_arm(), stub_arm("a0")], 5);
	let lines = recording(&mut options);
	options.a7_spend_limit_usd = Some(1.0);
	let a7 = a7_execute("claude-opus-5-5", 100_000);
	options.execute = Some(Arc::new(move |job| {
		if job.options.variant.as_ref().is_some_and(|variant| variant.harness == Harness::Opencode) {
			a7(job)
		} else {
			let outcome = row_outcome(&job);
			Box::pin(async move { outcome })
		}
	}));
	assert_eq!(run_bench(options).await.unwrap(), 4);
	let results = rows(&out.join("results.jsonl"));
	let of = |arm: &str| results.iter().filter(|row| row["arm"] == arm).collect::<Vec<_>>();
	assert_eq!(of("a7").len(), 3);
	assert_eq!(of("a0").len(), 5);
	assert!((of("a7")[0]["ledger"]["cost"].as_f64().unwrap() - 0.4).abs() < 1e-12);
	let last = lines.lock().unwrap().last().cloned().unwrap();
	let pattern = r"^A7 stopped \(A7's priced spend is \$1\.20; limit \$1, 0537\): 2 A7 trials not run";
	assert!(Regex::new(pattern).unwrap().is_match(&last), "{last}");
	assert_eq!(conditions(&out)["sessions"][0]["a7SpendUsd"], json!(1.2));
}

#[tokio::test]
async fn an_a7_model_the_price_table_lacks_stops_a7_since_its_spend_cant_be_counted() {
	let mut options = options(vec![local()], &temp().join("run"), vec![a7_arm()], 3);
	let lines = recording(&mut options);
	options.execute = Some(a7_execute("no-such-model", 10));
	assert_eq!(run_bench(options).await.unwrap(), 4);
	let last = lines.lock().unwrap().last().cloned().unwrap();
	assert!(last.contains("the price table lacks no-such-model"), "{last}");
}

#[tokio::test]
async fn the_hidden_tests_fail_on_the_base_commit_and_pass_three_times_with_the_gold_patch() {
	let good = local();
	let broken =
		BenchTask { instance_id: "acme__widget-8".to_owned(), test_command: "git --version".to_owned(), ..local() };
	let out = temp().join("run");
	let work = temp().join("w");
	let failed = run_validation(&[good, broken], &temp(), &out, &|_| {}, Some(&work)).await.unwrap();
	assert_eq!(failed, ["acme__widget-8"]);
	let validated = rows(&out.join("validation.jsonl"));
	let (good_row, broken_row) = (&validated[0], &validated[1]);
	assert_eq!(good_row["task"], json!("acme__widget-7"));
	assert_eq!(
		(&good_row["null_failed"], &good_row["gold_passes"], &good_row["error"]),
		(&json!(true), &json!(3), &Value::Null)
	);
	assert_eq!(good_row["seconds"]["gold"].as_array().unwrap().len(), 3);
	assert_eq!(broken_row["task"], json!("acme__widget-8"));
	assert_eq!((&broken_row["null_failed"], &broken_row["gold_passes"]), (&json!(false), &json!(3)));
	let logs = out.join("validation").join("acme__widget-7");
	assert!(fs::read_to_string(logs.join("setup.log")).unwrap().contains("$ git --version"));
	assert!(Regex::new(r"exit: 1\n").unwrap().is_match(&fs::read_to_string(logs.join("null.log")).unwrap()));
	for i in 1..=3 {
		let text = fs::read_to_string(logs.join(format!("gold-{i}.log"))).unwrap();
		assert!(text.contains("exit: 0\n"), "{text}");
	}
	assert!(work.exists());
}
