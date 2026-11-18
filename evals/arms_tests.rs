//! Ported from `test/evals/arms.test.ts`: the cases that need only `variants.rs` and `harness.rs`.
//!
//! Deferred, with the module each needs: "the main run is 30 tasks × 3 reps × 7 arms = 630 trials" (`evals/bench.rs`,
//! `evals/tasks.rs`); "0587: the omp arms refuse an omp folder whose MCP files …" (`mcpProblem`, `evals/bench.rs`).
//! "A6's layer keeps its data under the OS temp folder" is the library's test (`src/frontier/layer/bench_tests.rs`).
//! "A6 fails loud without Jocasta's layer build" has no Rust twin: the layer is linked in, so there is no checkout
//! without it.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::followup::{FOLLOWUP_ARMS, FOLLOWUP_REPS, FOLLOWUP_TASKS, pick_followup_tasks};
use super::harness::{
	A7_API_KEY_ENV, TrialRecords, Turn, claude_sessions, opencode_sessions, read_opencode_response,
	read_prewalk_switch, save_sessions,
};
use super::plan_review::PlanReviewChoice;
use super::runner::{EvalCase, trial_args, trial_env};
use super::tasks::{BenchTask, TaskLanguage, TaskSet, plan_trials};
use super::variants::{Harness, Variant, load_variants};
use crate::lib::args::REPO_ROOT;
use jocasta::shared::testing::temp_dir;

fn variants_dir() -> PathBuf {
	REPO_ROOT.join("evals").join("variants")
}

fn dir_variant(dir: &Path, name: &str) -> jocasta::shared::errors::Result<Variant> {
	load_variants(dir, &[name.to_owned()])
}

fn arm(name: &str) -> Variant {
	dir_variant(&variants_dir(), name).unwrap()
}

fn entry(line: &Value) -> String {
	format!("{line}\n")
}

#[test]
fn the_prewalk_detector_reads_a_model_change_entry_with_the_role_prewalk_from_the_trials_own_session_files_0608() {
	let temp = temp_dir("jocasta-arms-");
	let dir = temp.path();
	let header = entry(&json!({ "type": "session", "id": "s1" }));
	let message = entry(&json!({ "type": "message", "message": { "role": "assistant", "model": "m" } }));
	// No switch: the session's model changes are the user's (`default`, `temporary`) or a retry's, never `prewalk`.
	std::fs::write(
		dir.join("a.jsonl"),
		header.clone()
			+ &entry(&json!({ "type": "model_change", "model": "anthropic/x", "role": "default" }))
			+ &message,
	)
	.unwrap();
	assert!(!read_prewalk_switch(dir).unwrap());
	// A subagent's file in a subfolder is not the trial's own: a prewalk entry there does not count.
	std::fs::create_dir(dir.join("sub")).unwrap();
	std::fs::write(
		dir.join("sub").join("b.jsonl"),
		entry(&json!({ "type": "model_change", "model": "anthropic/y", "role": "prewalk" })),
	)
	.unwrap();
	assert!(!read_prewalk_switch(dir).unwrap());
	std::fs::write(
		dir.join("c.jsonl"),
		header
			+ &message
			+ &entry(&json!({ "type": "model_change", "model": "anthropic/y", "role": "prewalk" }))
			+ &message,
	)
	.unwrap();
	assert!(read_prewalk_switch(dir).unwrap());
	assert!(!read_prewalk_switch(&dir.join("missing")).unwrap());
}

#[test]
fn plan_review_role_needs_plan_review_and_is_omps_only() {
	let temp = temp_dir("jocasta-arms-");
	let dir = temp.path();
	std::fs::write(dir.join("role.yaml"), "model: m\nplan_review_role: smol\n").unwrap();
	let error = dir_variant(dir, "role").unwrap_err().to_string();
	assert!(error.contains("plan_review_role answers omp's plan review"), "{error}");
	std::fs::write(dir.join("oc.yaml"), "harness: opencode\nplan_review: keep\nplan_review_role: smol\n").unwrap();
	let error = dir_variant(dir, "oc").unwrap_err().to_string();
	assert!(error.contains("plan_review, plan_review_role apply to omp only"), "{error}");
}

#[test]
fn omp_only_keys_are_refused_on_opencode() {
	let temp = temp_dir("jocasta-arms-");
	let dir = temp.path();
	std::fs::write(dir.join("oc.yaml"), "harness: opencode\nlayer: true\nsettings: { a: 1 }\n").unwrap();
	let error = dir_variant(dir, "oc").unwrap_err().to_string();
	assert!(error.contains("settings, layer apply to omp only, not opencode"), "{error}");
	assert_eq!(Harness::Opencode.as_str(), "opencode");
}

fn concat(items: &[String], more: &[&str]) -> Vec<String> {
	items.iter().cloned().chain(more.iter().map(|item| (*item).to_owned())).collect()
}

#[test]
fn each_follow_up_arm_differs_from_a6_in_its_one_part_only() {
	let a6 = arm("a6");
	assert!(arm("f-keep-context").layer);
	assert_eq!(arm("f-keep-context").plan_review, Some(PlanReviewChoice::Keep));
	assert!(arm("f-keep-smol").layer);
	assert_eq!(arm("f-keep-smol").plan_review, Some(PlanReviewChoice::Keep));
	assert_eq!(arm("f-keep-smol").plan_review_role.as_deref(), Some("smol"));
	assert_eq!(a6.plan_review_role, None);
	// Prewalk is on A1, not A6: A6 starts in plan mode, where omp turns the automatic switch off.
	let a1 = arm("a1");
	assert_eq!(arm("f-prewalk-clean").args, concat(&a1.args, &["--prewalk"]));
	assert_eq!(arm("f-prewalk-clean").plan_review, None);
	assert_eq!(arm("f-prewalk-clean").plan_first, a1.plan_first);
	// 0620: the advisor on A6, as panes run it; `--thinking auto` last on A1, so it replaces A0's `high`.
	let mut expected = a6.settings.clone().unwrap();
	expected.insert("advisor".to_owned(), json!({ "enabled": true, "reviewInterval": 3 }));
	let mut roles = expected["modelRoles"].as_object().cloned().unwrap_or_default();
	roles.insert("advisor".to_owned(), json!("anthropic/claude-fable-5-2:medium"));
	expected.insert("modelRoles".to_owned(), Value::Object(roles));
	assert_eq!(arm("f-advisor").settings, Some(expected));
	assert_eq!(arm("f-advisor").plan_review, Some(PlanReviewChoice::Execute));
	assert_eq!(arm("f-think-auto").args, concat(&a1.args, &["--thinking", "auto"]));
	assert_eq!(arm("f-think-auto").settings, a1.settings);
	let ponytail = arm("f-ponytail-if").append_system_prompt.unwrap();
	assert!(ponytail.ends_with("ponytail-important-if.md"), "{ponytail}");
	let plain = std::fs::read_to_string(a6.append_system_prompt.unwrap()).unwrap();
	let wrapped = std::fs::read_to_string(ponytail).unwrap();
	// Same Ponytail body; only the Running tests rules sit inside the conditional block.
	let body = |text: &str| text[text.find("# Ponytail, lazy senior dev mode").unwrap()..].to_owned();
	assert_eq!(
		body(&wrapped).replacen("<important if=\"you are running or writing tests\">\n", "", 1).replacen(
			"</important>\n",
			"",
			1
		),
		body(&plain)
	);
	assert!(wrapped.contains("<important if=\"you are running or writing tests\">\n- Save test output"));
}

#[test]
fn session_ids_come_from_opencodes_and_claude_codes_json_output() {
	let id = "ses_24debf3a8ffeNIh02hZdU8Yx4V";
	let oc = format!(
		"{}\n{}\n",
		json!({ "type": "step_start", "sessionID": id }),
		json!({ "type": "text", "sessionID": id })
	);
	assert_eq!(opencode_sessions(&oc), [id]);
	let cc = format!(
		"{}\n",
		json!({ "type": "system", "subtype": "init", "session_id": "1001133b-1a93-458d-a92a-ce512911db5c" })
	);
	assert_eq!(claude_sessions(&cc), ["1001133b-1a93-458d-a92a-ce512911db5c"]);
}

#[test]
fn saved_sessions_omps_folder_is_copied_whole_claude_codes_transcript_and_subagents_are_found_by_session_id() {
	let temp = temp_dir("jocasta-arms-");
	let omp = temp.path().join("sessions");
	std::fs::create_dir_all(omp.join("sub")).unwrap();
	std::fs::write(omp.join("main.jsonl"), "{}\n").unwrap();
	std::fs::write(omp.join("sub").join("agent.jsonl"), "{}\n").unwrap();
	let saved = temp.path().join("saved");
	std::fs::create_dir_all(&saved).unwrap();
	std::fs::write(saved.join("stale.jsonl"), "{}\n").unwrap();
	let records =
		|transcript: &str, session_dir: Option<PathBuf>, launcher: &str, env: Vec<(&str, String)>| TrialRecords {
			transcript: transcript.to_owned(),
			session_dir,
			launcher: vec![launcher.to_owned()],
			env: env.into_iter().map(|(name, value)| (name.to_owned(), value)).collect(),
			cwd: PathBuf::from("."),
			scrub: None,
		};
	save_sessions(Harness::Omp, &saved, &records("", Some(omp), "omp", vec![])).unwrap();
	assert!(saved.join("main.jsonl").exists() && saved.join("sub").join("agent.jsonl").exists());
	assert!(!saved.join("stale.jsonl").exists());

	let claude_home = temp.path().join("claude-home");
	let id = "1001133b-1a93-458d-a92a-ce512911db5c";
	let project = claude_home.join("projects").join("C--t-repo");
	std::fs::create_dir_all(project.join(id).join("subagents")).unwrap();
	std::fs::write(project.join(format!("{id}.jsonl")), "{}\n").unwrap();
	std::fs::write(project.join(id).join("subagents").join("agent-a1.jsonl"), "{}\n").unwrap();
	let saved_claude = temp.path().join("claude");
	let transcript = json!({ "type": "system", "subtype": "init", "session_id": id }).to_string();
	save_sessions(
		Harness::Claude,
		&saved_claude,
		&records(&transcript, None, "claude", vec![("CLAUDE_CONFIG_DIR", claude_home.display().to_string())]),
	)
	.unwrap();
	assert!(saved_claude.join(format!("{id}.jsonl")).exists());
	assert!(saved_claude.join(id).join("subagents").join("agent-a1.jsonl").exists());
}

#[test]
fn opencodes_usage_comes_from_its_exports_steps_as_requests_reasoning_as_output_its_own_cost() {
	let temp = temp_dir("jocasta-arms-");
	let dir = temp.path();
	let export = json!({
		"info": { "id": "ses_1" },
		"messages": [
			{ "info": { "role": "user" }, "parts": [] },
			{
				"info": { "role": "assistant", "modelID": "claude-opus-5-5" },
				"parts": [
					{ "type": "tool", "tool": "read", "state": { "status": "completed" } },
					{
						"type": "step-finish",
						"tokens": { "input": 10, "output": 5, "reasoning": 3, "cache": { "read": 100, "write": 20 } },
						"cost": 0.01,
					},
					{
						"type": "step-finish",
						"tokens": { "input": 1, "output": 1, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
						"cost": 0.002,
					},
				],
			},
		],
	});
	std::fs::write(dir.join("ses_1.json"), export.to_string()).unwrap();
	let response = read_opencode_response(dir).unwrap();
	assert_eq!(response.model.as_deref(), Some("claude-opus-5-5"));
	assert_eq!((response.requests, response.turns, response.tool_calls), (2, 1, 1));
	let usage = response.usage;
	assert_eq!(
		(usage.input, usage.output, usage.cache_read, usage.cache_write, usage.total_tokens),
		(11.0, 9.0, 100.0, 20.0, 140.0)
	);
	assert!((usage.cost - 0.012).abs() < 5e-13);
}

fn stub_task(id: &str, language: TaskLanguage, set: TaskSet) -> BenchTask {
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
	let of = |prefix: &str, n: usize, language| {
		(0..n).map(move |i| stub_task(&format!("{prefix}-{i}"), language, TaskSet::Main)).collect::<Vec<_>>()
	};
	let mut all = of("py", 12, TaskLanguage::Python);
	all.extend(of("ts", 8, TaskLanguage::TypeScript));
	all.extend(of("js", 4, TaskLanguage::JavaScript));
	all.extend(of("go", 6, TaskLanguage::Go));
	all.push(stub_task("spare-0", TaskLanguage::Go, TaskSet::Spare));
	all
}

fn turn(prompt: &str, follow_up: bool) -> Turn {
	Turn { prompt: prompt.to_owned(), follow_up, overlay: "o.yml".to_owned(), ..Turn::default() }
}

#[test]
fn a6_is_a3_plus_the_layer_plan_first_through_omps_plan_review_answered_execute_every_omp_arm_warms_only_while_streaming_0567()
 {
	let a3 = arm("a3");
	let a6 = arm("a6");
	assert!(a6.layer);
	assert_eq!(a6.followups, a3.followups);
	assert!(!a6.plan_first);
	assert_eq!(a6.plan_review, Some(PlanReviewChoice::Execute));
	assert!(a3.plan_first);
	assert_eq!(a3.plan_review, None);
	let roles = &a6.settings.as_ref().unwrap()["modelRoles"];
	assert_eq!(roles["default"], "anthropic/claude-opus-5-5:high");
	assert_eq!(roles["plan"], "anthropic/claude-opus-5-5:high");
	for name in [
		"a0",
		"a1",
		"a2",
		"a3",
		"a6",
		"f-prewalk-clean",
		"f-keep-context",
		"f-keep-smol",
		"f-ponytail-if",
		"f-advisor",
		"f-think-auto",
	] {
		assert_eq!(arm(name).settings.as_ref().unwrap()["providers"]["cacheWarming"], "streaming", "{name}");
	}
	// 0619: every omp arm but the advisor's has the advisor off, whatever the machine's settings say.
	assert_eq!(arm("a1").settings.as_ref().unwrap()["advisor"]["enabled"], false);
	assert_eq!(arm("a6").settings.as_ref().unwrap()["advisor"]["enabled"], false);
	// 0574: the two arms whose harness updates itself have it off.
	assert_eq!(arm("a4").env, std::collections::HashMap::from([("DISABLE_AUTOUPDATER".to_owned(), "1".to_owned())]));
	assert_eq!(
		arm("a7").env,
		std::collections::HashMap::from([("OPENCODE_DISABLE_AUTOUPDATE".to_owned(), "1".to_owned())])
	);
	// A6's first turn is interactive: no -p, the extension, the prompt last; its review turns are A3's print turns.
	let first = trial_args(
		&a6,
		&Turn { session_dir: Some("s".to_owned()), extension: Some("x.js".to_owned()), ..turn("Fix it", false) },
	)
	.unwrap();
	assert!(!first.contains(&"-p".to_owned()));
	assert!(!first.contains(&"--plan-yolo".to_owned()));
	assert_eq!(first[first.len() - 5..], ["--config", "o.yml", "-e", "x.js", "Fix it"]);
	let review = trial_args(&a6, &Turn { session_dir: Some("s".to_owned()), ..turn("review", true) }).unwrap();
	assert_eq!(review[review.len() - 4..], ["-p", "review", "--mode", "json"]);
	let dir = temp_dir("jocasta-arms");
	std::fs::write(dir.path().join("both.yaml"), "model: m\nplan_first: true\nplan_review: execute\n").unwrap();
	let error = dir_variant(dir.path(), "both").unwrap_err().to_string();
	assert!(error.contains("two ways to plan first"), "{error}");
	let a7 = arm("a7");
	assert_eq!(a7.harness, Harness::Opencode);
	assert_eq!(a7.model.as_deref(), Some("anthropic/claude-opus-5-5"));
	assert_eq!(a7.args, ["--variant", "high", "--auto"]);
	assert!(!a7.layer);
	assert_eq!(
		trial_args(&a7, &turn("Fix it", false)).unwrap(),
		["run", "--model", "anthropic/claude-opus-5-5", "--variant", "high", "--auto", "--format", "json", "Fix it"]
	);
	let again = trial_args(&a7, &Turn { session: Some("ses_abc".to_owned()), ..turn("again", true) }).unwrap();
	assert_eq!(again[..3], ["run", "--session", "ses_abc"]);
	let error = trial_args(&a7, &turn("again", true)).unwrap_err().to_string();
	assert!(error.contains("first turn's session"), "{error}");
}

#[test]
fn the_follow_up_study_10_main_tasks_in_the_main_sets_language_proportions_x_2_reps_x_a6_a1_and_six_comparison_arms_0571_0575_0605_0608_0620()
 {
	assert_eq!(
		FOLLOWUP_ARMS,
		["a6", "a1", "f-keep-context", "f-keep-smol", "f-ponytail-if", "f-prewalk-clean", "f-advisor", "f-think-auto"]
	);
	let set = main_set();
	let picked = pick_followup_tasks(&set, 482).unwrap();
	assert_eq!(picked.len(), FOLLOWUP_TASKS);
	let count = |languages: &[TaskLanguage]| picked.iter().filter(|task| languages.contains(&task.language)).count();
	assert_eq!(
		[
			count(&[TaskLanguage::Python]),
			count(&[TaskLanguage::TypeScript, TaskLanguage::JavaScript]),
			count(&[TaskLanguage::Go])
		],
		[4, 4, 2]
	);
	assert!(!picked.iter().any(|task| task.set == TaskSet::Spare));
	let again = pick_followup_tasks(&set, 482).unwrap();
	assert_eq!(
		again.iter().map(|task| task.instance_id.as_str()).collect::<Vec<_>>(),
		picked.iter().map(|task| task.instance_id.as_str()).collect::<Vec<_>>()
	);
	let arms: Vec<String> = FOLLOWUP_ARMS.iter().map(|name| (*name).to_owned()).collect();
	assert_eq!(plan_trials(&picked, &arms, FOLLOWUP_REPS, 482).len(), 160);
}

#[test]
fn a7s_key_reaches_opencode_only_every_trials_environment_drops_it_and_any_anthropic_api_key() {
	let mut jocasta = jocasta::shared::testing::temp_jocasta();
	jocasta.set_var(A7_API_KEY_ENV, Some("sk-test"));
	jocasta.set_var("ANTHROPIC_API_KEY", Some("sk-other"));
	let folder = temp_dir("jocasta-arms");
	let env = trial_env(&EvalCase::default(), folder.path(), &std::collections::HashMap::new());
	assert!(!env.contains_key(A7_API_KEY_ENV));
	assert!(!env.contains_key("ANTHROPIC_API_KEY"));
}
