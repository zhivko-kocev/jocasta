//! Tests of `weekly_review.rs`, and the stand-in for omp's headless run (`fake_omp`, the Rust form of `fake-omp.ts`,
//! 0630). The TypeScript cases run the review through the whole service; these run it on the part, with the same fake
//! omp and fake mise as programs.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::*;
use crate::frontier::server::mise::tests::captured_shim;
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::status::tests::{World, world};
use crate::shared::pathkey::real_path;
use crate::shared::testing::{TempDir, temp_dir};

/// A stand-in for omp's headless run (the weekly big-picture review, 0470): writes its argv to `argv.json` in its
/// working folder, then answers with a drift line, or with `FAKE_OMP_REPLY` when set, and exits with `FAKE_OMP_EXIT`
/// when set (3 is omp's cost-cap exit). `FAKE_OMP_LOG` gets a line per run, `FAKE_OMP_SLEEP_MS` makes it wait first. It
/// acts only when the test executable was started as omp: after `--`, the arguments the review gave.
#[test]
fn fake_omp() {
	let args: Vec<String> = std::env::args().skip_while(|arg| arg != "--").skip(1).collect();
	if !args.iter().any(|arg| arg == "--max-cost") {
		return;
	}
	fs::write("argv.json", serde_json::to_string(&args).expect("argv as JSON")).expect("writing argv.json");
	if let Some(log) = env::var("FAKE_OMP_LOG") {
		let mut file = fs::OpenOptions::new().create(true).append(true).open(log).expect("the omp log");
		file.write_all(b"run\n").expect("logging a run");
	}
	let sleep: u64 = env::var("FAKE_OMP_SLEEP_MS").and_then(|ms| ms.parse().ok()).unwrap_or(0);
	std::thread::sleep(std::time::Duration::from_millis(sleep));
	println!(
		"{}",
		env::var("FAKE_OMP_REPLY").unwrap_or_else(|| "Looked at the week.\n\nDRIFT: no drift found".to_owned())
	);
	let code: i32 = env::var("FAKE_OMP_EXIT").and_then(|code| code.parse().ok()).unwrap_or(0);
	if code != 0 {
		std::process::exit(code);
	}
}

const EMPTY: &str = "<empty>";

fn git(dir: &Path, args: &[&str]) -> String {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
	String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit_in(dir: &Path, text: &str, message: &str) {
	fs::write(dir.join("README.md"), text).unwrap();
	git(dir, &["commit", "-qam", message]);
}

/// A demo project: a repo with one commit, as the catalog would list it.
fn demo(w: &World) -> Project {
	let main = real_path(w.j.root()).join("demo");
	fs::create_dir_all(&main).unwrap();
	git(&main, &["init", "-q", "-b", "main"]);
	fs::write(main.join("README.md"), "demo\n").unwrap();
	git(&main, &["add", "."]);
	git(&main, &["commit", "-q", "-m", "init demo"]);
	Project {
		id: "p-demo".to_owned(),
		identity: String::new(),
		name: "demo".to_owned(),
		main_path: main,
		git: true,
		paths: vec![],
	}
}

struct Fixture {
	w: World,
	project: Project,
	_shims: TempDir,
}

/// The fake omp (this executable on `fake_omp`) and the fake mise (a shim), a demo project with two commits this week.
async fn fixture(extra: impl FnOnce(&mut crate::frontier::server::service::ServiceOptions)) -> Fixture {
	let shims = temp_dir("shims");
	let mise = captured_shim(shims.path(), "mise", "frontier::server::mise::tests::fake_mise");
	let exe = std::env::current_exe().expect("the test executable").to_string_lossy().into_owned();
	let omp = ["--exact", "frontier::server::weekly_review::tests::fake_omp", "--nocapture", "--"];
	let omp: Vec<String> = std::iter::once(exe).chain(omp.iter().map(|arg| (*arg).to_owned())).collect();
	let w = world(|options| {
		options.omp = Some(omp);
		options.mise = Some(Some(vec![mise.to_string_lossy().into_owned()]));
		extra(options);
	});
	let project = demo(&w);
	commit_in(&project.main_path, "demo\nA second change.\n", "Second change");
	Fixture { w, project, _shims: shims }
}

/// Sets the fake mise's answers (a trusted folder with the `scorecard` task) with `extra` over them, until the test ends.
fn mise(f: &mut Fixture, extra: &[(&str, &str)]) {
	let mut answers = vec![
		("FAKE_MISE_PARANOID", "[\"config\"]\n"),
		("FAKE_MISE_TRUST", EMPTY),
		("FAKE_MISE_ENV_JSON", "{}"),
		("FAKE_MISE_TASKS", "[{\"name\": \"scorecard\"}]"),
	];
	answers.extend_from_slice(extra);
	for (name, value) in answers {
		f.w.j.set_var(name, Some(value));
	}
}

/// One review of the project; its result line and the argv the agent was started with.
async fn review(f: &Fixture) -> (String, Vec<String>) {
	let line = f.w.ctx.weekly.review(&f.project).await.unwrap().unwrap_or_default();
	(line, argv_of(&f.project))
}

fn argv_of(project: &Project) -> Vec<String> {
	let text = fs::read_to_string(project_dir(&project.id).join("weekly").join("argv.json")).expect("the agent's argv");
	serde_json::from_str(&text).unwrap()
}

fn prompt_of(argv: &[String]) -> &str {
	argv.last().unwrap()
}

fn kept(project: &Project) -> PathBuf {
	project_dir(&project.id).join("scorecard")
}

// ---- weekly-scorecard.test.ts ----

#[tokio::test]
async fn a_project_s_scorecard_task_runs_before_the_agent_its_output_is_kept_for_the_week_and_the_agent_s_prompt_carries_it_with_an_earlier_week_both_and_the_report_names_the_lines_that_changed_0616()
 {
	let mut f = fixture(|_| {}).await;
	mise(&mut f, &[("FAKE_MISE_SCORECARD", "functions over 80 lines: 13\nduplicated: 0.18%\n")]);
	let (line, argv) = review(&f).await;
	assert!(prompt_of(&argv).contains("functions over 80 lines: 13 | duplicated: 0.18%"));
	assert!(prompt_of(&argv).contains("measurements to read as questions, not targets"));
	assert!(prompt_of(&argv).contains("There is no earlier week to compare."));
	assert!(line.ends_with("no drift found · scorecard: no earlier week to compare"), "{line}");
	let files: Vec<_> = fs::read_dir(kept(&f.project)).unwrap().collect();
	assert_eq!(files.len(), 1);
	let stored = fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
	assert_eq!(stored, "functions over 80 lines: 13\nduplicated: 0.18%");

	fs::write(kept(&f.project).join("2000-W01.txt"), "functions over 80 lines: 15\nduplicated: 0.18%\nold: 1").unwrap();
	mise(&mut f, &[("FAKE_MISE_SCORECARD", "functions over 80 lines: 13\nduplicated: 0.20%\nnew: 2\n")]);
	let (line, argv) = review(&f).await;
	assert!(prompt_of(&argv).contains("This week: functions over 80 lines: 13 | duplicated: 0.20% | new: 2"));
	assert!(
		prompt_of(&argv).contains("Last week (2000-W01): functions over 80 lines: 15 | duplicated: 0.18% | old: 1")
	);
	assert!(
		line.ends_with(
			"no drift found · scorecard: changed since last week: functions over 80 lines: 15 → 13; duplicated: 0.18% → 0.20%; new: 2"
		),
		"{line}"
	);
}

#[tokio::test]
async fn a_project_without_the_task_no_run_no_line_the_review_as_it_is_0616() {
	let mut f = fixture(|_| {}).await;
	let log = f.w.j.root().join("mise.log");
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]"), ("FAKE_MISE_LOG", &log.to_string_lossy())]);
	let (line, argv) = review(&f).await;
	assert!(line.ends_with("(2 commits): no drift found"), "{line}");
	assert!(!prompt_of(&argv).contains("scorecard"));
	assert!(!fs::read_to_string(&log).unwrap_or_default().contains("run scorecard"));
	assert!(!kept(&f.project).exists());
}

#[tokio::test]
async fn a_scorecard_task_that_fails_runs_past_its_limit_or_sits_in_an_untrusted_config_the_review_still_runs_and_says_why_there_is_no_scorecard_0616()
 {
	let mut f = fixture(|options| options.scorecard_ms = Some(4000)).await;
	mise(&mut f, &[("FAKE_MISE_SCORECARD", "partial: 1\n"), ("FAKE_MISE_SCORECARD_EXIT", "2")]);
	let (line, argv) = review(&f).await;
	assert!(line.ends_with("no drift found · no scorecard: the scorecard task exited 2: scorecard: boom"), "{line}");
	assert!(!prompt_of(&argv).contains("partial"));

	mise(&mut f, &[("FAKE_MISE_SCORECARD", "late: 1\n"), ("FAKE_MISE_SCORECARD_SLEEP_MS", "60000")]);
	f.w.j.set_var("FAKE_MISE_SCORECARD_EXIT", None);
	let (line, argv) = review(&f).await;
	assert!(line.ends_with("no drift found · no scorecard: the scorecard task ran past its 4 s limit"), "{line}");
	assert!(!prompt_of(&argv).contains("late"));

	let config = f.project.main_path.join("mise.toml");
	let untrusted = format!("{}: untrusted\n", config.display());
	f.w.j.set_var("FAKE_MISE_SCORECARD_SLEEP_MS", None);
	mise(&mut f, &[("FAKE_MISE_TRUST", &untrusted), ("FAKE_MISE_SCORECARD", "never: 1\n")]);
	let (line, _) = review(&f).await;
	assert!(
		line.ends_with(&format!("no drift found · no scorecard: {} isn't trusted in mise", config.display())),
		"{line}"
	);
	assert!(!kept(&f.project).exists());
}

#[tokio::test]
async fn an_oversized_scorecard_output_is_cut_to_the_cap_in_what_is_kept_and_in_the_prompt_0616() {
	let mut f = fixture(|_| {}).await;
	let big = format!("{}\n", "x".repeat(SCORECARD_MAX_CHARS * 3));
	mise(&mut f, &[("FAKE_MISE_SCORECARD", &big)]);
	let (_, argv) = review(&f).await;
	let files: Vec<_> = fs::read_dir(kept(&f.project)).unwrap().collect();
	let stored = fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
	assert_eq!(stored, format!("{}\n[cut at {SCORECARD_MAX_CHARS} characters]", "x".repeat(SCORECARD_MAX_CHARS)));
	assert!(prompt_of(&argv).contains(stored.split('\n').next().unwrap()));
	assert!(prompt_of(&argv).len() < SCORECARD_MAX_CHARS + 3000);
}

// ---- service-allow.test.ts: big_picture_review ----

#[tokio::test]
async fn one_commit_is_no_week_to_review_and_two_make_the_range_from_the_empty_tree() {
	let w = world(|_| {});
	let project = demo(&w);
	assert_eq!(w.ctx.weekly.range(&project).await.unwrap(), None);
	assert_eq!(w.ctx.weekly.review(&project).await.unwrap(), None);
	assert!(!project_dir(&project.id).join("weekly").join("argv.json").exists());
	commit_in(&project.main_path, "demo\nA second change.\n", "Second change");
	let head = git(&project.main_path, &["rev-parse", "--short", "HEAD"]);
	let week = w.ctx.weekly.range(&project).await.unwrap().unwrap();
	assert_eq!(
		week,
		Week {
			trunk: "main".to_owned(),
			now: head.clone(),
			from: EMPTY_TREE.to_owned(),
			range: format!("{EMPTY_TREE}..{head}"),
			merged: 2
		}
	);
	// A project that isn't a git repository has no trunk, so no week.
	let plain = Project { git: false, ..project };
	assert_eq!(w.ctx.weekly.range(&plain).await.unwrap(), None);
}

#[tokio::test]
async fn big_picture_review_starts_omp_headless_and_read_only_on_trunk_s_past_week_and_its_drift_line_is_the_result_0460_0470_0577()
 {
	let mut f = fixture(|_| {}).await;
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]")]);
	let head = git(&f.project.main_path, &["rev-parse", "--short", "HEAD"]);
	assert!(!f.w.ctx.weekly.running(&f.project.id));
	let (line, argv) = review(&f).await;
	assert_eq!(line, format!("{EMPTY_TREE}..{head} (2 commits): no drift found"));
	assert!(!f.w.ctx.weekly.running(&f.project.id));
	assert!(!argv.contains(&"--continue".to_owned()));
	let at = argv.iter().position(|arg| arg == "-p").unwrap();
	let main = f.project.main_path.to_string_lossy().into_owned();
	assert_eq!(argv[at..at + 5], ["-p", "--tools", "read,grep,glob,task", "--add-dir", main.as_str()]);
	assert!(argv.contains(&overlay_file(&f.project.id).to_string_lossy().into_owned()));
	// The run's cost cap (0608), before print mode starts.
	let at = argv.iter().position(|arg| arg == "--max-cost").unwrap();
	assert_eq!(argv[at..at + 3], ["--max-cost", "25", "-p"]);
	// omp's async jobs off, or print mode exits before the reviewer subagent answers; no advisor (0619).
	let headless = shipped_config_dir(None).join("headless.yml").to_string_lossy().into_owned();
	let at = argv.iter().position(|arg| *arg == headless).unwrap();
	assert_eq!(argv[at - 1], "--config");
	let text = fs::read_to_string(&headless).unwrap();
	assert!(text.contains("async:") && text.contains("advisor:"), "{text}");
	let prompt = prompt_of(&argv);
	for part in [
		"read-only",
		"skill://lessons",
		"report only material findings; prefer one strong finding over several weak ones",
		"Run the drift subagent on that range.",
		"End your reply with one line on its own: `DRIFT:",
	] {
		assert!(prompt.contains(part), "{part}: {prompt}");
	}
	let rubric = shipped_config_dir(None).join("design-rubric.md").to_string_lossy().replace('\\', "/");
	assert!(prompt.contains(&rubric));
	// A young repo is reviewed from its start: its commits are listed by their end, not the range.
	assert!(prompt.contains(&format!("log --oneline {head}`")), "{prompt}");
	// 0621: the week's production numbers, computed before the agent and kept for the week.
	assert!(prompt.contains("This week: Threads finished this week: none"));
	assert_eq!(fs::read_dir(project_dir(&f.project.id).join("production")).unwrap().count(), 1);
}

#[tokio::test]
async fn the_project_s_rules_files_are_named_in_the_prompt_when_it_has_them() {
	let mut f = fixture(|_| {}).await;
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]")]);
	let (_, argv) = review(&f).await;
	assert!(!prompt_of(&argv).contains("the project's rules ("));
	fs::write(f.project.main_path.join("AGENTS.md"), "rules\n").unwrap();
	let (_, argv) = review(&f).await;
	let rules = f.project.main_path.join("AGENTS.md").to_string_lossy().replace('\\', "/");
	assert!(
		prompt_of(&argv).contains(&format!("give it the project's rules ({rules}) and its lessons in skill://lessons"))
	);
}

#[tokio::test]
async fn two_reviews_at_once_are_one_run_and_neither_ends_the_other_s_processes_svc_12() {
	let mut f = fixture(|_| {}).await;
	let log = f.w.j.root().join("omp.log");
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]"), ("FAKE_OMP_LOG", &log.to_string_lossy())]);
	let weekly = &f.w.ctx.weekly;
	let (first, second, running) = tokio::join!(weekly.review(&f.project), weekly.review(&f.project), async {
		tokio::task::yield_now().await;
		weekly.running(&f.project.id)
	});
	assert!(running);
	let (first, second) = (first.unwrap().unwrap(), second.unwrap().unwrap());
	assert!(first.ends_with("no drift found"));
	assert_eq!(first, second);
	assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
	assert!(!weekly.running(&f.project.id));
	// Not at once: a second, later one is a run of its own.
	review(&f).await;
	assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 2);
}

#[tokio::test]
async fn a_run_omp_ended_at_its_cost_cap_is_reported_as_such_0608() {
	let mut f = fixture(|_| {}).await;
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]"), ("FAKE_OMP_REPLY", "Cost cap reached"), ("FAKE_OMP_EXIT", "3")]);
	let error = f.w.ctx.weekly.review(&f.project).await.unwrap_err().to_string();
	assert!(error.contains("gave no result (it reached its $25 cost cap)"), "{error}");
	assert!(error.starts_with("the big-picture review of "), "{error}");
	assert!(!f.w.ctx.weekly.running(&f.project.id));
}

#[tokio::test]
async fn a_run_that_ends_without_a_drift_line_says_how_it_ended() {
	let mut f = fixture(|_| {}).await;
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]"), ("FAKE_OMP_REPLY", "boom"), ("FAKE_OMP_EXIT", "2")]);
	let error = f.w.ctx.weekly.review(&f.project).await.unwrap_err().to_string();
	assert!(error.ends_with("gave no result (omp exited 2: boom)"), "{error}");
}

#[tokio::test]
async fn a_run_past_its_time_is_stopped_and_says_it_ran_out_of_time_0470() {
	let mut f = fixture(|options| options.weekly_review_ms = Some(1500)).await;
	mise(&mut f, &[("FAKE_MISE_TASKS", "[]"), ("FAKE_OMP_SLEEP_MS", "20000")]);
	let error = f.w.ctx.weekly.review(&f.project).await.unwrap_err().to_string();
	assert!(error.ends_with("gave no result (it ran out of time)"), "{error}");
}

// ---- the pure parts ----

#[test]
fn the_drift_line_is_the_last_one_with_its_markup_around_it_cut_off() {
	assert_eq!(drift_of("Looked.\n\nDRIFT: no drift found\n").as_deref(), Some("no drift found"));
	assert_eq!(drift_of("DRIFT: first\nmore\n- `DRIFT: second one`  \n").as_deref(), Some("second one"));
	assert_eq!(drift_of("> drift:   quoted, lower case\r\nthe end\r\n").as_deref(), Some("quoted, lower case"));
	assert_eq!(drift_of("DRIFT: strongest finding, in a.rs\r\n").as_deref(), Some("strongest finding, in a.rs"));
	assert_eq!(drift_of("no marker here\nDRIFTING: no\n"), None);
	assert_eq!(drift_of("DRIFT:   \n"), None);
}

#[test]
fn what_changed_since_last_week_is_said_by_label_and_new_lines_are_said_as_they_are() {
	let before = "functions over 80 lines: 15\nduplicated: 0.18%\nold: 1";
	assert_eq!(
		changes(before, "functions over 80 lines: 13\nduplicated: 0.20%\nnew: 2"),
		"changed since last week: functions over 80 lines: 15 → 13; duplicated: 0.18% → 0.20%; new: 2"
	);
	assert_eq!(changes(before, before), "no line changed since last week");
	assert_eq!(changes("lone", "lone: 3"), "changed since last week: lone → 3");
	assert_eq!(changes("a: 1: x", "a: 1: y"), "changed since last week: a: 1: x → 1: y");
}

#[test]
fn lines_are_joined_for_the_prompt_without_newlines_and_overlong_output_is_cut_with_a_last_line_that_says_so() {
	assert_eq!(one_line("  a: 1 \n\n b: 2\n"), "a: 1 | b: 2");
	assert_eq!(capped("short"), "short");
	let exact = "x".repeat(SCORECARD_MAX_CHARS);
	assert_eq!(capped(&exact), exact);
	let long = "y".repeat(SCORECARD_MAX_CHARS + 1);
	assert_eq!(capped(&long), format!("{}\n[cut at 6000 characters]", "y".repeat(SCORECARD_MAX_CHARS)));
	// Counted in UTF-16 units, as the TypeScript's `length` is: a character outside the BMP counts two.
	let wide = "😀".repeat(SCORECARD_MAX_CHARS / 2 + 1);
	assert_eq!(capped(&wide), format!("{}\n[cut at 6000 characters]", "😀".repeat(SCORECARD_MAX_CHARS / 2)));
}

#[test]
fn the_week_is_the_iso_week_of_now_and_a_kept_text_is_replaced_within_the_week_and_found_in_the_next() {
	let week = iso_week();
	assert!(Regex::new(r"^\d{4}-W\d\d$").unwrap().is_match(&week), "{week}");
	let dir = temp_dir("keep");
	let kept = dir.path().join("kept");
	assert_eq!(keep_for_week(&kept, "one").unwrap(), None);
	assert_eq!(fs::read_to_string(kept.join(format!("{week}.txt"))).unwrap(), "one");
	// This week's file is replaced and is not "last week".
	assert_eq!(keep_for_week(&kept, "two").unwrap(), None);
	assert_eq!(fs::read_to_string(kept.join(format!("{week}.txt"))).unwrap(), "two");
	fs::write(kept.join("2000-W01.txt"), "old").unwrap();
	fs::write(kept.join("2001-W05.txt"), "newer").unwrap();
	fs::write(kept.join("9999-W01.txt"), "future").unwrap();
	assert_eq!(keep_for_week(&kept, "three").unwrap(), Some(("2001-W05".to_owned(), "newer".to_owned())));
	assert_eq!(SCORECARD_MAX_CHARS, 6000);
}
