//! Tests of `thread_start.rs`: the cases of `service-thread-start.test.ts`, `service-regressions.test.ts` (UX-11's
//! slugs and half-done start, C10's focus and last-started-in picks, SVC-7's undo, SEC-LOC-5's `done_when` guard, C6's
//! layer-off start), `service-keys.test.ts` (the relay check, "thread_start's brief and done_when") and
//! `service-handoff.test.ts` (CO-5) whose subject is a thread start, rewritten at part level on
//! `context::test_context` (the service and its HTTP server come in a later wave).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use regex::Regex;
use serde_json::{Value, json};

use super::*;
use crate::frontier::server::allow::{Answer, Confirm, Confirmation};
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::keys::Keys;
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::surface::Surface;
use crate::frontier::server::wording::{WHEN_YOU_FINISH, fallback_slug};
use crate::shared::bridge_wire::BRIDGE_EXTENSION;
use crate::shared::env::overlay as env_overlay;
use crate::shared::git::git;
use crate::shared::layer::LAYER_ENV;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::testing::{TempDir, temp_dir};

const BRIEF: &str = "Add a README line.";

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn make_repo(dir: &Path) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	// Another text and message per repo: one root commit makes one project (0103).
	let name = dir.file_name().unwrap().to_string_lossy();
	fs::write(dir.join("README.md"), format!("{name}\n")).unwrap();
	sh(dir, &["add", "."]);
	sh(dir, &["commit", "-q", "-m", &format!("init {name}")]);
	real_path(dir)
}

fn strings(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

fn lf(text: &str) -> String {
	text.replace("\r\n", "\n")
}

fn shipped(file: &str) -> String {
	shipped_config_dir(None).join(file).display().to_string()
}

fn guard() -> Vec<String> {
	strings(&["--config", &shipped("guard.yml"), "-e", &shipped(BRIDGE_EXTENSION)])
}

/// His Allow: every question is kept and answered by `allow`.
fn asking(allow: &Arc<AtomicBool>, asked: &Arc<Mutex<Vec<Confirmation>>>) -> Confirm {
	let (allow, asked) = (Arc::clone(allow), Arc::clone(asked));
	Arc::new(move |ask: Confirmation, _signal| -> BoxFuture<'static, crate::shared::errors::Result<Answer>> {
		asked.lock().unwrap().push(ask);
		let allow = allow.load(Ordering::SeqCst);
		Box::pin(async move { Ok(Answer { allow, how: PopupEnd::Answered }) })
	})
}

/// The TypeScript's `demoProject` and `startThread`: git projects under one watched root, a context whose herdr brings
/// omp up idle at once, his Allow given (flip `allow`).
struct World {
	t: TestContext,
	_root: TempDir,
	code: PathBuf,
	asked: Arc<Mutex<Vec<Confirmation>>>,
	allow: Arc<AtomicBool>,
}

fn world_with(options: impl FnOnce(&mut ServiceOptions)) -> World {
	let asked = Arc::new(Mutex::new(Vec::new()));
	let allow = Arc::new(AtomicBool::new(true));
	let (confirm_asked, confirm_allow) = (Arc::clone(&asked), Arc::clone(&allow));
	let t = test_context(|o| {
		o.omp = Some(vec!["omp".to_owned()]);
		o.pane_prompt = Some(None);
		o.mise = Some(None);
		o.confirm = Some(asking(&confirm_allow, &confirm_asked));
		o.agent_wait_ms = Some(10_000);
		options(o);
	});
	t.herdr.state().omp_up_on_create = Some(AgentState::Idle);
	let root = temp_dir("thread-start");
	let code = real_path(root.path()).join("code");
	fs::create_dir_all(&code).unwrap();
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let roots =
		format!("version: 1\nroots:\n  - path: {}\n    depth: 2\n", code.display().to_string().replace('\\', "/"));
	fs::write(file, roots).unwrap();
	World { t, _root: root, code, asked, allow }
}

fn world() -> World {
	world_with(|_| {})
}

impl World {
	/// A git project `name` in the watched root; its main checkout.
	fn project(&self, name: &str) -> PathBuf {
		make_repo(&self.code.join(name))
	}

	async fn start(&self, project: &str, title: &str, brief: &str) -> Result<Started> {
		self.t.ctx.starts.start_with(project, title, brief, StartOptions::new(Door::Boss)).await
	}

	/// `startThread`: `Readme line` in demo; its pane, worktree and reply.
	async fn start_thread(&self, title: &str, brief: &str) -> (String, PathBuf, String) {
		let reply = self.start("demo", title, brief).await.unwrap();
		assert!(reply.done, "{}", reply.text);
		let pane = self.t.herdr.state().pane_list.last().cloned().expect("a thread pane");
		(pane.pane_id, pane.cwd.expect("its folder"), reply.text)
	}

	fn project_id(&self, name: &str) -> String {
		self.t.ctx.places.find(name).unwrap().id
	}

	/// The task file folder (0540) of the project's open thread titled `title`.
	fn task_dir_of(&self, project: &str, title: &str) -> PathBuf {
		let found = crate::frontier::server::task_file::project_tasks(&self.project_id(project));
		let task = found.iter().filter(|task| task.title == title && !task.closed).max_by_key(|task| task.created_at);
		let task = task.expect("a task record");
		task_dir(&task.project_id, &task.id)
	}

	fn worktrees(&self) -> Vec<Value> {
		self.t.herdr.of("createWorktree").into_iter().map(|call| call[1].clone()).collect()
	}

	fn prompts(&self) -> Vec<(String, String)> {
		self.t
			.herdr
			.of("prompt")
			.into_iter()
			.map(|call| (call[0].as_str().unwrap().to_owned(), call[1].as_str().unwrap().to_owned()))
			.collect()
	}

	fn thread_command(&self, project: &str, title: &str, extra: &[&str]) -> Vec<String> {
		let prompt = self.task_dir_of(project, title).join("prompt.md").display().to_string();
		let mut command = strings(&["omp", "--append-system-prompt", &prompt]);
		command.extend(guard());
		command.extend(["--config".to_owned(), overlay_file(&self.project_id(project)).display().to_string()]);
		command.extend(strings(extra));
		// A thread's cost cap, `threads.maxCostUsd` (75); a heavy thread's extra names its own (0686).
		if !extra.contains(&"--max-cost") {
			command.extend(strings(&["--max-cost", "75"]));
		}
		command.extend(strings(&["--warn-cost", "50"]));
		command
	}
}

async fn until(mut check: impl FnMut() -> bool) {
	for _ in 0..200 {
		if check() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(25)).await;
	}
	panic!("the test's condition never held");
}

fn refusal(result: Result<Started>) -> String {
	result.expect_err("a refusal was expected").to_string()
}

fn pane_json(path: &Path) -> Value {
	json!(path.display().to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_start_makes_the_project_workspace_the_overlay_a_worktree_on_feature_slug_then_the_brief_with_the_lessons_line()
 {
	let w = world();
	let main = w.project("demo");
	let (pane, path, reply) = w.start_thread("Readme line", BRIEF).await;
	assert_eq!(
		reply,
		"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane."
	);
	let id = w.project_id("demo");
	let overlay = overlay_file(&id);
	assert!(overlay.exists());
	// The project's lessons: one skill `lessons`, its index made from the shipped template (0459).
	let lessons = crate::shared::paths::lessons_dir(&id).join("SKILL.md");
	assert_eq!(
		fs::read_to_string(lessons).unwrap(),
		fs::read_to_string(shipped_config_dir(None).join("lessons").join("SKILL.md")).unwrap()
	);
	// The project workspace: label = project name, cwd = main checkout, its pane the sleeper until a key starts its omp
	// (0676).
	let ensured = &w.t.herdr.of("ensureWorkspace");
	assert_eq!(ensured.len(), 1);
	let sleeper = w.t.ctx.panes.sleeper_command("demo · press any key to start");
	assert_eq!(ensured[0][..3], [json!("demo"), pane_json(&main), json!(sleeper)]);
	let project_env = ensured[0][3].as_object().unwrap();
	assert!(project_env.contains_key(PANE_KEY_ENV));
	let project_ws = w.t.herdr.state().workspace_list.iter().find(|ws| ws.label == "demo").unwrap().id.clone();
	// Thread panes get their own variables (0457); a new thread starts a fresh session,
	// never `--continue` (0455).
	let worktrees = w.t.herdr.of("createWorktree");
	assert_eq!(worktrees.len(), 1);
	assert_eq!(worktrees[0][0], json!(project_ws));
	let made = worktrees[0][1].as_object().unwrap();
	assert_eq!(made["repo"], pane_json(&main));
	assert_eq!(made["branch"], "feature/readme-line");
	assert_eq!(made["base"], "main");
	assert_eq!(made["path"], pane_json(&main.parent().unwrap().join("demo.wt").join("readme-line")));
	assert_eq!(made["title"], "Readme line");
	// Every thread warns at $50 (0541) and appends its task's brief and done-when (0540).
	assert_eq!(made["command"], json!(w.thread_command("demo", "Readme line", &[])));
	let env = made["env"].as_object().unwrap();
	assert!(env.contains_key(PANE_KEY_ENV));
	assert_eq!(env["JOCASTA_THREAD"], "readme-line");
	assert_eq!(env["COMPOSE_PROJECT_NAME"], "demo-readme-line");
	assert_eq!(env["JOCASTA_PORT_OFFSET"], "10");
	// Not heavy: its record has no `heavy` and its omp no heavy.yml (0590).
	let record: Value =
		serde_json::from_str(&fs::read_to_string(w.task_dir_of("demo", "Readme line").join("thread.json")).unwrap())
			.unwrap();
	assert!(record.get("heavy").is_none());
	assert_eq!(path, main.parent().unwrap().join("demo.wt").join("readme-line"));
	assert_eq!(sh(&path, &["rev-parse", "--abbrev-ref", "HEAD"]), "feature/readme-line");
	// The brief ends with the lessons line (0459); the commit step is in `prompt.md`'s When you finish (0577).
	assert!(w.prompts().contains(&(
		pane.clone(),
		"Add a README line.\n\nBefore you plan, read skill://lessons and cite each lesson your plan relies on as (lesson: <file>); a plan that goes against one says why.".to_owned()
	)));
	// The thread is registered, launched, and the last one started in.
	let thread = w.t.ctx.threads.get(&pane).expect("registered");
	assert_eq!(
		thread.with(|t| (t.branch.clone(), t.path.clone(), t.title.clone())),
		("feature/readme-line".to_owned(), path.clone(), "Readme line".to_owned())
	);
	assert!(w.t.ctx.launched.has(&pane));
	assert_eq!(w.t.ctx.starts.last_started_in().map(|p| p.name), Some("demo".to_owned()));
	// A second thread reuses the project workspace and takes the next free number (0457).
	w.start_thread("Second", "More.").await;
	assert_eq!(w.t.herdr.of("ensureWorkspace").len(), 1);
	assert_eq!(w.worktrees()[1]["env"]["JOCASTA_THREAD"], "second");
	assert_eq!(w.worktrees()[1]["env"]["COMPOSE_PROJECT_NAME"], "demo-second");
	assert_eq!(w.worktrees()[1]["env"]["JOCASTA_PORT_OFFSET"], "20");
}

#[tokio::test]
async fn a_thread_takes_its_first_state_from_herdrs_pane_list_while_the_start_waits_for_omp() {
	// `startThread`'s `late`: omp reports only once herdr made the worktree, while `thread_start` already polls (0456).
	let w = world();
	w.project("demo");
	w.t.herdr.state().omp_up_on_create = None;
	let start = w.start("demo", "Readme line", BRIEF);
	let drive = async {
		until(|| w.t.herdr.of("createWorktree").len() == 1).await;
		let pane = w.t.herdr.state().pane_list.last().unwrap().pane_id.clone();
		w.t.herdr.set_state(&pane, AgentState::Idle, Some("/sessions/a.jsonl"), false);
		pane
	};
	let (reply, pane) = tokio::join!(start, drive);
	assert!(reply.unwrap().done);
	let thread = w.t.ctx.threads.get(&pane).unwrap();
	until(|| thread.with(|t| t.state == AgentState::Idle)).await;
	assert_eq!(thread.with(|t| t.session_path.clone()), Some(PathBuf::from("/sessions/a.jsonl")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_heavy_thread_start_is_recorded_on_its_task_and_loads_heavy_yml_after_the_overlay() {
	let w = world();
	w.project("demo");
	let options = StartOptions { heavy: true, ..StartOptions::new(Door::Boss) };
	let reply = w.t.ctx.starts.start_with("demo", "Readme line", "Migrate it.", options).await.unwrap();
	assert_eq!(
		reply.text,
		"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane. Heavy: it runs on his slow role."
	);
	let heavy = ["--config", &shipped("heavy.yml"), "--max-cost", "150"];
	let record: Value =
		serde_json::from_str(&fs::read_to_string(w.task_dir_of("demo", "Readme line").join("thread.json")).unwrap())
			.unwrap();
	assert_eq!(record["heavy"], true);
	assert_eq!(w.worktrees()[0]["command"], json!(w.thread_command("demo", "Readme line", &heavy)));
	// Another door says "your".
	let cli = StartOptions { heavy: true, ..StartOptions::new(Door::Cli) };
	let reply = w.t.ctx.starts.start_with("demo", "Second", "Again.", cli).await.unwrap();
	assert_eq!(
		reply.text,
		"Started demo/Second on feature/second: it plans first; approve its plan in its pane. Heavy: it runs on your slow role."
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_budget_is_recorded_on_the_task_and_is_the_threads_max_cost_0686() {
	let w = world();
	w.project("demo");
	let options = StartOptions { budget: Some(5.0), ..StartOptions::new(Door::Boss) };
	w.t.ctx.starts.start_with("demo", "Readme line", "Migrate it.", options).await.unwrap();
	let record: Value =
		serde_json::from_str(&fs::read_to_string(w.task_dir_of("demo", "Readme line").join("thread.json")).unwrap())
			.unwrap();
	assert_eq!(record["budget"].as_f64(), Some(5.0));
	let command = w.worktrees()[0]["command"].clone();
	assert_eq!(command, json!(w.thread_command("demo", "Readme line", &["--max-cost", "5"])));
	let argv = command.as_array().unwrap();
	assert_eq!(argv[argv.len() - 4..], [json!("--max-cost"), json!("5"), json!("--warn-cost"), json!("50")]);
}

#[test]
fn a_budget_is_a_number_of_dollars_of_at_least_one_at_both_doors_0686() {
	assert_eq!(budget_of(None, Door::Boss).unwrap(), None);
	assert_eq!(budget_of(Some(&Value::Null), Door::Boss).unwrap(), None);
	assert_eq!(budget_of(Some(&json!(5)), Door::Boss).unwrap(), Some(5.0));
	assert_eq!(budget_of(Some(&json!(1)), Door::Cli).unwrap(), Some(1.0));
	let refused = |value: Value, door| budget_of(Some(&value), door).unwrap_err().to_string();
	assert_eq!(refused(json!(0.5), Door::Boss), "thread_start: budget is a number of dollars, 1 or more.");
	assert_eq!(refused(json!("5"), Door::Cli), "jocasta thread new: budget is a number of dollars, 1 or more.");
	assert_eq!(refused(json!(-3), Door::Key), "New thread: budget is a number of dollars, 1 or more.");
}

#[test]
fn a_heavy_or_done_when_that_isnt_what_it_should_be_is_refused_at_both_doors() {
	assert!(!heavy_of(None, Door::Boss).unwrap());
	assert!(heavy_of(Some(&json!(true)), Door::Boss).unwrap());
	assert!(!heavy_of(Some(&json!(false)), Door::Cli).unwrap());
	let refused = |value: Value, door| heavy_of(Some(&value), door).unwrap_err().to_string();
	assert_eq!(refused(json!("yes"), Door::Boss), "thread_start: heavy is true or false.");
	assert_eq!(refused(json!(1), Door::Cli), "jocasta thread new: heavy is true or false.");
	assert_eq!(refused(Value::Null, Door::Key), "New thread: heavy is true or false.");
	assert_eq!(done_when_of(None, Door::Boss).unwrap(), Vec::<String>::new());
	assert_eq!(done_when_of(Some(&json!(["a", "b c"])), Door::Boss).unwrap(), ["a", "b c"]);
	let listed = |value: Value, door| done_when_of(Some(&value), door).unwrap_err().to_string();
	assert_eq!(listed(json!("make"), Door::Boss), "thread_start: done_when is a list of commands.");
	assert_eq!(listed(json!(["ok", 3]), Door::Cli), "jocasta thread new: done_when is a list of commands.");
	assert_eq!(listed(Value::Null, Door::Key), "New thread: done_when is a list of commands.");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_worktree_goes_beside_the_project_in_wt_slug_and_a_taken_branch_or_folder_makes_both_2_3() {
	let w = world();
	let main = w.project("demo");
	let folder = main.parent().unwrap().join("demo.wt");
	let (_, first, _) = w.start_thread("Readme line", BRIEF).await;
	assert_eq!(first, folder.join("readme-line"));
	// The first thread moved off its branch: a second one with the same title never picks up its branch.
	sh(&first, &["checkout", "--detach"]);
	let (pane, again, reply) = w.start_thread("Readme line", "Again.").await;
	assert!(reply.starts_with("Started demo/Readme line on branch feature/readme-line-2"));
	assert_eq!(w.worktrees()[1]["repo"], pane_json(&main));
	assert_eq!(w.worktrees()[1]["branch"], "feature/readme-line-2");
	assert_eq!(w.worktrees()[1]["path"], pane_json(&folder.join("readme-line-2")));
	assert_eq!(w.t.herdr.state().pane_list.iter().find(|p| p.pane_id == pane).unwrap().cwd, Some(again.clone()));
	assert_eq!(again, folder.join("readme-line-2"));
	assert_eq!(sh(&again, &["rev-parse", "--abbrev-ref", "HEAD"]), "feature/readme-line-2");
	// The first one's folder is gone, its branch stays (a PR finish): the next takes -3 for both.
	sh(&main, &["worktree", "remove", "--force", &first.display().to_string()]);
	w.start_thread("Readme line", "Third.").await;
	assert_eq!(w.worktrees()[2]["branch"], "feature/readme-line-3");
	assert_eq!(w.worktrees()[2]["path"], pane_json(&folder.join("readme-line-3")));
}

#[tokio::test]
async fn project_panes_append_the_shipped_ponytail_prompt_and_thread_panes_append_it_with_their_task_in_one_file() {
	let w = world_with(|o| o.pane_prompt = None);
	let main = w.project("demo");
	w.start_thread("Readme line", BRIEF).await;
	// Project panes append Ponytail without its license comment (0577): a copy in <data> (PRM-6).
	let shipped_text = fs::read_to_string(shipped_config_dir(None).join("ponytail.md")).unwrap();
	assert!(shipped_text.starts_with("<!--"));
	let stripped = crate::shared::paths::data_dir().join("ponytail.md");
	let wanted = crate::shared::prompt::without_leading_comment(&shipped_text);
	assert_eq!(lf(&fs::read_to_string(&stripped).unwrap()), lf(wanted.trim_start()));
	let sleeper = w.t.ctx.panes.sleeper_command("demo · press any key to start");
	assert_eq!(w.t.herdr.of("ensureWorkspace")[0][..3], [json!("demo"), pane_json(&main), json!(sleeper)]);
	// omp 18.18 takes one --append-system-prompt (the last), so Ponytail and the task share `prompt.md`.
	let prompt = w.task_dir_of("demo", "Readme line").join("prompt.md");
	assert_eq!(w.worktrees()[0]["command"], json!(w.thread_command("demo", "Readme line", &[])));
	// Then the task's sections, ending with how to finish (0577, PRM-7).
	assert_eq!(
		lf(&fs::read_to_string(prompt).unwrap()),
		format!(
			"{}\n\n# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- No commands given; the review's project check decides.\n\n{}\n",
			lf(wanted.trim()),
			WHEN_YOU_FINISH.join("\n")
		)
	);
}

#[tokio::test]
async fn a_pane_whose_omp_never_comes_up_is_a_half_done_start_that_says_the_brief_wasnt_sent() {
	let w = world_with(|o| o.agent_wait_ms = Some(0));
	w.project("demo");
	w.t.herdr.state().omp_up_on_create = None;
	let half = w.start("demo", "Add a README line.", "Go.").await.unwrap();
	assert!(!half.done);
	assert!(
		half.text.ends_with(", but omp didn't come up in its pane within 0 s; the brief wasn't sent."),
		"{}",
		half.text
	);
	assert!(w.t.herdr.of("prompt").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn his_new_thread_key_starts_a_macedonian_title_on_a_transliterated_branch_and_another_scripts_on_thread_date_n()
{
	// UX-11, C11: the key's door answers `Started … on <branch>: it plans first; approve its plan in its pane.`
	let w = world();
	w.project("demo");
	let key = |title: &'static str| {
		let starts = &w.t.ctx.starts;
		async move { starts.start("demo", title, "Go.", Door::Key).await.unwrap() }
	};
	let mk = key("Поправи го најавувањето кога сесијата е истечена.").await;
	assert!(mk.done);
	let pattern = Regex::new(
		r"^Started demo/Поправи го најавувањето.* on feature/popravi-go-najavuvanjeto[a-z-]*: it plans first; approve its plan in its pane\.$",
	)
	.unwrap();
	assert!(pattern.is_match(&mk.text), "{}", mk.text);
	let zh = key("修复登录问题").await;
	assert_eq!(
		zh.text,
		format!(
			"Started demo/修复登录问题 on feature/{}: it plans first; approve its plan in its pane.",
			fallback_slug(chrono::Utc::now().timestamp_millis(), 1)
		)
	);
	assert!(zh.done);
}

#[tokio::test]
async fn the_keys_start_is_a_half_done_error_when_the_brief_wasnt_sent() {
	let w = world_with(|o| o.agent_wait_ms = Some(0));
	w.project("demo");
	w.t.herdr.state().omp_up_on_create = None;
	let outcome = Keys::new(w.t.ctx.clone()).start("demo", "Add a README line.").await.unwrap();
	assert!(outcome.is_error);
	assert!(outcome.text.ends_with(", but omp didn't come up in its pane within 0 s; the brief wasn't sent."));
}

#[tokio::test]
async fn the_brief_and_done_when_go_into_the_task_file_and_every_launch_of_the_threads_omp() {
	// "thread_start's brief and done_when" (0540); the plan, session and spend that follow belong to the thread
	// lifecycle and wait for the service (see the report).
	let w = world();
	let main = w.project("demo");
	let options =
		StartOptions { done_when: strings(&["bun test test/readme.test.ts"]), ..StartOptions::new(Door::Boss) };
	let reply = w.t.ctx.starts.start_with("demo", "Readme line", BRIEF, options).await.unwrap();
	assert!(reply.done);
	let dir = w.task_dir_of("demo", "Readme line");
	assert!(Regex::new(r"^t-[0-9a-f]{4}$").unwrap().is_match(&dir.file_name().unwrap().to_string_lossy()));
	let prompt = dir.join("prompt.md");
	assert_eq!(w.worktrees()[0]["command"], json!(w.thread_command("demo", "Readme line", &[])));
	assert_eq!(
		lf(&fs::read_to_string(prompt).unwrap()),
		format!(
			"# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- `bun test test/readme.test.ts` passes\n\n{}\n",
			WHEN_YOU_FINISH.join("\n")
		)
	);
	let task = || lf(&fs::read_to_string(dir.join("task.md")).unwrap());
	let base = sh(&main, &["rev-parse", "--short", "main"]);
	until(|| task().contains("- State: idle")).await;
	let id = dir.file_name().unwrap().to_string_lossy().into_owned();
	assert_eq!(
		task(),
		[
			"# Readme line (demo)".to_owned(),
			String::new(),
			format!("- Thread: {id} · session — · parent: — · base {base}"),
			"- State: idle · spend —".to_owned(),
			String::new(),
			"## Brief".to_owned(),
			BRIEF.to_owned(),
			String::new(),
			"## Done when".to_owned(),
			"- `bun test test/readme.test.ts` passes".to_owned(),
			String::new(),
			"## Plan".to_owned(),
			"Not approved yet.".to_owned(),
			String::new(),
			"## Report".to_owned(),
			"None yet.".to_owned(),
			String::new(),
			"## Reviews".to_owned(),
			"None yet.".to_owned(),
			String::new(),
		]
		.join("\n")
	);
	let record: Value = serde_json::from_str(&fs::read_to_string(dir.join("thread.json")).unwrap()).unwrap();
	assert_eq!(record["path"], w.worktrees()[0]["path"]);
	assert_eq!(record["branch"], "feature/readme-line");
	assert_eq!(record["doneWhen"], json!(["bun test test/readme.test.ts"]));
}

#[tokio::test]
async fn with_the_layer_off_a_thread_pane_gets_layer_off_its_prompt_no_done_when_and_its_brief_the_commit_line() {
	// C6's start half (0576); the report steer and the review belong to the thread lifecycle.
	let mut off = None;
	let w = world_with(|_| off = Some(Var::set(LAYER_ENV, Some("off"))));
	w.project("demo");
	let options = StartOptions { done_when: strings(&["git diff --quiet"]), ..StartOptions::new(Door::Boss) };
	let reply = w.t.ctx.starts.start_with("demo", "Readme line", BRIEF, options).await.unwrap();
	assert!(reply.done);
	let pane = w.t.herdr.state().pane_list.last().unwrap().pane_id.clone();
	assert_eq!(
		w.t.herdr.env_of(&pane).unwrap().iter().find(|(name, _)| name == LAYER_ENV).map(|(_, value)| value.as_str()),
		Some("off")
	);
	let prompt = fs::read_to_string(w.task_dir_of("demo", "Readme line").join("prompt.md")).unwrap();
	assert!(!prompt.contains("## Done when"));
	assert!(!prompt.contains(WHEN_YOU_FINISH[0]));
	assert!(w.prompts().contains(&(pane, format!("{BRIEF}\n\n{LESSONS_LINE}\n{COMMIT_LINE}"))));
	drop(off);
}

#[tokio::test]
async fn a_start_that_fails_once_herdr_made_its_worktree_takes_back_the_workspace_worktree_and_branch() {
	// SVC-7: the next start gets the same name.
	let w = world();
	let main = w.project("demo");
	w.t.herdr.state().create_worktree_fails =
		Some(crate::frontier::server::surface::SurfaceError::new("error", "herdr's layout failed"));
	let reply = refusal(w.start("demo", "Readme line", BRIEF).await);
	assert!(reply.contains("herdr's layout failed"), "{reply}");
	let folder = main.parent().unwrap().join("demo.wt").join("readme-line");
	assert!(!folder.exists());
	assert_eq!(sh(&main, &["branch", "--list", "feature/readme-line"]), "");
	assert!(!w.t.herdr.state().workspace_list.iter().any(|ws| ws.label == "Readme line"));
	assert!(w.t.ctx.threads.all().is_empty());
	// Its task record and launch prompt went with it.
	assert!(crate::frontier::server::task_file::project_tasks(&w.project_id("demo")).is_empty());
	w.t.herdr.state().create_worktree_fails = None;
	assert_eq!(w.start_thread("Readme line", BRIEF).await.1, folder);
}

#[tokio::test]
async fn a_done_when_the_guard_denies_is_refused_unasked_and_one_it_asks_about_waits_for_his_allow_which_names_it() {
	// SEC-LOC-5 (0580).
	let w = world();
	w.project("demo");
	w.allow.store(false, Ordering::SeqCst);
	let start = |command: &str| {
		let options = StartOptions { done_when: vec![command.to_owned()], ..StartOptions::new(Door::Boss) };
		w.t.ctx.starts.start_with("demo", "Ship", "Ship it.", options)
	};
	assert_eq!(
		refusal(start("git push --force origin main").await),
		"thread_start: done_when `git push --force origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started."
	);
	// Quotes don't launder a denied command: omp's own matcher sees it without them.
	assert_eq!(
		refusal(start("git push --for\"ce\" origin main").await),
		"thread_start: done_when `git push --for\"ce\" origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started."
	);
	assert!(w.asked.lock().unwrap().is_empty());
	assert_eq!(refusal(start("npm publish").await), "He chose Deny; no thread was started.");
	{
		let asked = w.asked.lock().unwrap();
		assert_eq!(asked.len(), 1);
		assert_eq!(asked[0].message, "Start the thread \"Ship\" in demo?");
		assert!(asked[0].detail.contains("It runs when the work is reviewed:  `npm publish`"));
	}
	assert!(w.t.herdr.of("createWorktree").is_empty());
	w.allow.store(true, Ordering::SeqCst);
	assert!(start("npm publish").await.unwrap().text.starts_with("Started demo/Ship on branch feature/ship."));
}

#[tokio::test]
async fn at_the_clis_door_a_done_when_the_guard_asks_about_is_refused_without_asking() {
	let w = world();
	w.project("demo");
	let options = StartOptions { done_when: strings(&["npm publish"]), ..StartOptions::new(Door::Cli) };
	let refused = refusal(w.t.ctx.starts.start_with("demo", "Ship", "Ship it.", options).await);
	assert_eq!(
		refused,
		"jocasta thread new: done_when `npm publish` needs your Allow under Jocasta's guard, and jocasta thread new asks nothing; have the boss start it. No thread was started."
	);
	assert!(w.asked.lock().unwrap().is_empty() && w.t.herdr.of("createWorktree").is_empty());
}

#[tokio::test]
async fn a_start_in_a_project_that_isnt_a_git_repository_or_with_a_multi_line_done_when_is_refused_by_door() {
	let w = world();
	w.project("demo");
	let notes = w.code.join("notes");
	fs::create_dir_all(&notes).unwrap();
	let (catalog, handle) = (w.t.ctx.catalog.clone(), tokio::runtime::Handle::current());
	tokio::task::spawn_blocking(move || {
		let db = catalog.lock().unwrap();
		handle.block_on(crate::shared::projects::add_project(&db, &notes, None)).unwrap();
	})
	.await
	.unwrap();
	let plain = refusal(w.start("notes", "Notes", BRIEF).await);
	assert_eq!(plain, "thread_start: notes isn't a git repository; threads need a worktree.");
	let unknown = refusal(w.start("nope", "Nope", BRIEF).await);
	assert!(unknown.contains("nope"), "{unknown}");
	for command in ["", "  ", "make\nmake"] {
		let options = StartOptions { done_when: vec![command.to_owned()], ..StartOptions::new(Door::Key) };
		let refused = refusal(w.t.ctx.starts.start_with("demo", "Ship", BRIEF, options).await);
		assert_eq!(refused, "New thread: each done_when is one command on one line.");
	}
	assert!(w.t.herdr.of("createWorktree").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_boss_asks_before_a_start_in_another_project_than_the_last_one_he_focused_and_not_before_one_in_it() {
	// service-allow.test.ts's thread_start half (0484, 0492).
	let w = world();
	w.project("demo");
	w.project("client-a");
	// No project pane of his focused yet in this run: a start anywhere asks.
	let (thread_pane, _, _) = w.start_thread("Readme line", BRIEF).await;
	{
		let asked = w.asked.lock().unwrap();
		assert_eq!(
			asked.iter().map(|a| a.message.as_str()).collect::<Vec<_>>(),
			["Start the thread \"Readme line\" in demo?"]
		);
		assert_eq!(
			asked[0].detail,
			"You haven't focused a project's pane yet in this run.\n\nThe boss's brief:\nAdd a README line."
		);
	}
	// He focuses the thread's pane, then herdr's home shell and the boss's (both in no project, 0492): a start in demo
	// goes ahead unasked, one in another project asks.
	let focus = |pane: &str| {
		for p in &mut w.t.herdr.state().pane_list {
			p.focused = p.pane_id == pane;
		}
	};
	focus(&thread_pane);
	w.t.ctx.starts.note_focus(&w.t.herdr.state().pane_list.clone()).unwrap();
	w.t.herdr.state().pane_list.iter_mut().for_each(|p| p.focused = false);
	Surface::ensure_workspace(&*w.t.herdr, "home", &crate::shared::paths::data_dir(), None, None).await.unwrap();
	let shell = w.t.herdr.state().pane_list.last().unwrap().pane_id.clone();
	focus(&shell);
	w.t.ctx.starts.note_focus(&w.t.herdr.state().pane_list.clone()).unwrap();
	w.t.with_boss(AgentState::Idle);
	w.t.herdr.state().pane_list.push(PaneInfo {
		pane_id: "boss-1".to_owned(),
		workspace_id: "w-boss".to_owned(),
		focused: true,
		..PaneInfo::default()
	});
	focus("boss-1");
	let second = w.start("demo", "Second", "More.").await.unwrap();
	assert!(second.done, "{}", second.text);
	assert_eq!(w.asked.lock().unwrap().len(), 1);
	w.allow.store(false, Ordering::SeqCst);
	assert_eq!(refusal(w.start("client-a", "Elsewhere", "Go.").await), "He chose Deny; no thread was started.");
	let last = w.asked.lock().unwrap().last().cloned().unwrap();
	assert_eq!(last.message, "Start the thread \"Elsewhere\" in client-a?");
	assert_eq!(last.detail, "The last project pane you focused is in demo, not client-a.\n\nThe boss's brief:\nGo.");
	assert_eq!(w.t.herdr.of("createWorktree").len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn his_own_key_and_the_clis_door_ask_nothing_whatever_the_focus() {
	let w = world();
	w.project("demo");
	w.allow.store(false, Ordering::SeqCst);
	assert!(w.t.ctx.starts.start("demo", "One", "Go.", Door::Key).await.unwrap().done);
	let cli = w.t.ctx.starts.start_with("demo", "Two", "Go.", StartOptions::new(Door::Cli)).await.unwrap();
	assert!(cli.done);
	assert!(w.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn two_starts_for_one_project_at_once_make_one_project_workspace() {
	// CO-5.
	let w = world();
	w.project("demo");
	let start = |title: &'static str| w.start("demo", title, "Go.");
	let (one, two) = tokio::join!(start("One"), start("Two"));
	let heads: Vec<String> =
		[one.unwrap(), two.unwrap()].iter().map(|r| r.text.split(". ").next().unwrap().to_owned()).collect();
	assert_eq!(heads, ["Started demo/One on branch feature/one", "Started demo/Two on branch feature/two"]);
	assert_eq!(w.t.herdr.state().workspace_list.iter().filter(|ws| ws.label == "demo").count(), 1);
	assert_eq!(w.t.herdr.of("createWorktree").len(), 2);
	// The gate is let go once no start holds it.
	assert!(w.t.ctx.starts.st().starts.is_empty());
}

#[tokio::test]
async fn a_start_can_be_spawned_onto_another_task() {
	let w = world();
	w.project("demo");
	let ctx = w.t.ctx.clone();
	let started = tokio::spawn(async move { ctx.starts.start("demo", "Spawned", "Go.", Door::Key).await });
	assert!(started.await.unwrap().unwrap().done);
}

fn focused(pane_id: &str, cwd: Option<&Path>) -> PaneInfo {
	PaneInfo {
		pane_id: pane_id.to_owned(),
		workspace_id: "w-x".to_owned(),
		cwd: cwd.map(Path::to_path_buf),
		focused: true,
		..PaneInfo::default()
	}
}

#[tokio::test]
async fn the_focused_project_is_the_project_of_the_last_focused_pane_that_belongs_to_one() {
	// 0484, 0492.
	let w = world();
	let demo = w.project("demo");
	let client = w.project("client-a");
	w.t.ctx.places.sync().await;
	let starts = &w.t.ctx.starts;
	assert!(starts.focused_project().is_none());
	// No pane focused: nothing changes.
	starts.note_focus(&[]).unwrap();
	starts.note_focus(&[PaneInfo { focused: false, ..focused("p-1", Some(&demo)) }]).unwrap();
	assert!(starts.focused_project().is_none());
	// A pane in a project's folder.
	starts.note_focus(&[focused("p-1", Some(&demo))]).unwrap();
	assert_eq!(starts.focused_project().map(|p| p.name), Some("demo".to_owned()));
	// A pane in no project (herdr's home shell) leaves it as it was.
	starts.note_focus(&[focused("p-2", Some(&crate::shared::paths::data_dir()))]).unwrap();
	starts.note_focus(&[focused("p-3", None)]).unwrap();
	assert_eq!(starts.focused_project().map(|p| p.name), Some("demo".to_owned()));
	// The boss's pane too, even in a project's folder.
	w.t.with_boss(AgentState::Idle);
	starts.note_focus(&[focused("boss-1", Some(&client))]).unwrap();
	assert_eq!(starts.focused_project().map(|p| p.name), Some("demo".to_owned()));
	// A registered thread's pane is its thread's project, wherever its cwd is.
	let thread = w.t.with_thread("t-1", "Other");
	thread.with(|t| t.project.name = "client-a".to_owned());
	starts.note_focus(&[focused("t-1", None)]).unwrap();
	assert_eq!(starts.focused_project().map(|p| p.name), Some("client-a".to_owned()));
}

#[tokio::test]
async fn the_project_of_a_pane_jocasta_started_comes_from_its_thread_or_its_recorded_folder() {
	let w = world();
	let demo = w.project("demo");
	w.t.ctx.places.sync().await;
	let starts = &w.t.ctx.starts;
	assert!(starts.pane_project("nope").unwrap().is_none());
	let thread = w.t.with_thread("t-1", "Readme line");
	assert_eq!(starts.pane_project("t-1").unwrap().map(|p| p.id), Some(thread.with(|t| t.project.id.clone())));
	w.t.with_boss(AgentState::Idle);
	w.t.ctx.launched.add("boss-1", &demo.display().to_string(), None).unwrap();
	assert!(starts.pane_project("boss-1").unwrap().is_none());
	w.t.ctx.launched.add("p-9", &demo.display().to_string(), Some("key")).unwrap();
	assert_eq!(starts.pane_project("p-9").unwrap().map(|p| p.name), Some("demo".to_owned()));
}

#[tokio::test]
async fn the_new_thread_form_picks_the_picked_threads_project_else_the_last_focused_pane_else_the_last_started_in_else_the_first()
 {
	// C10 (0581).
	let w = world();
	let mains = [("demo", w.project("demo")), ("client-a", w.project("client-a"))];
	let main_of = |name: &str| mains.iter().find(|(n, _)| *n == name).unwrap().1.clone();
	let keys = Keys::new(w.t.ctx.clone());
	w.t.ctx.places.sync().await;
	let form = keys.new_thread_form(None).await.unwrap();
	let mut listed = form.projects.clone();
	listed.sort();
	assert_eq!(listed, ["client-a", "demo"]);
	let (first, other) = (form.projects[0].clone(), form.projects[1].clone());
	assert_eq!(form.project, first);
	let (thread_pane, _, _) = {
		let reply = w.start(&other, "Readme line", BRIEF).await.unwrap();
		assert!(reply.done, "{}", reply.text);
		let pane = w.t.herdr.state().pane_list.last().cloned().unwrap();
		(pane.pane_id, pane.cwd, reply.text)
	};
	assert_eq!(keys.new_thread_form(None).await.unwrap().project, other);
	// He focuses a pane in the first project's main checkout.
	Surface::ensure_workspace(&*w.t.herdr, &first, &main_of(&first), None, None).await.unwrap();
	w.t.herdr.state().pane_list.last_mut().unwrap().focused = true;
	assert_eq!(keys.new_thread_form(None).await.unwrap().project, first);
	// The thread picked in today wins.
	assert_eq!(keys.new_thread_form(Some(&thread_pane)).await.unwrap().project, other);
}

/// An environment variable set in the test overlay until it drops.
struct Var(&'static str, Option<Option<String>>);

impl Var {
	fn set(name: &'static str, value: Option<&str>) -> Self {
		Self(name, env_overlay::set(name, value.map(str::to_owned)))
	}
}

impl Drop for Var {
	fn drop(&mut self) {
		env_overlay::restore(self.0, self.1.take());
	}
}

fn schedule(brief: &str, done_when: &[&str], heavy: bool) -> Schedule {
	Schedule {
		id: "0123456789ab".to_owned(),
		brief: brief.to_owned(),
		done_when: strings(done_when),
		heavy,
		days: vec![chrono::Weekday::Mon],
		time: (9, 0),
	}
}

#[tokio::test]
async fn a_scheduled_start_opens_a_plan_mode_thread_titled_from_its_brief_through_the_keys_door_asking_nothing() {
	let w = world();
	w.project("demo");
	w.allow.store(false, Ordering::SeqCst);
	let given = schedule("Bump dependencies. Then test.", &["cargo test"], true);
	assert_eq!(w.t.ctx.starts.start_scheduled("demo", &given).await.unwrap(), ScheduleStart::Started);
	assert!(w.asked.lock().unwrap().is_empty());
	let threads = w.t.ctx.threads.of_project(&w.project_id("demo"));
	assert_eq!(threads.len(), 1);
	assert_eq!(threads[0].with(|t| t.title.clone()), "Bump dependencies");
	let task = threads[0].with(|t| t.task.clone()).expect("its task file");
	assert_eq!(task.done_when, strings(&["cargo test"]));
	assert_eq!(task.heavy, Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scheduled_start_is_skipped_when_a_thread_with_its_title_is_open_in_the_project() {
	let w = world();
	w.project("demo");
	let given = schedule("Bump dependencies.", &[], false);
	assert_eq!(w.t.ctx.starts.start_scheduled("demo", &given).await.unwrap(), ScheduleStart::Started);
	let again = w.t.ctx.starts.start_scheduled("demo", &given).await.unwrap();
	assert!(
		matches!(&again, ScheduleStart::Skipped(why) if why.contains("Bump dependencies") && why.contains("already open")),
		"{again:?}"
	);
	assert_eq!(w.t.herdr.of("createWorktree").len(), 1);
}

#[tokio::test]
async fn a_scheduled_start_whose_done_when_the_guard_asks_about_or_denies_is_skipped_with_the_reason_and_starts_nothing()
 {
	let w = world();
	w.project("demo");
	let asks = w.t.ctx.starts.start_scheduled("demo", &schedule("Publish.", &["npm publish"], false)).await.unwrap();
	assert!(matches!(&asks, ScheduleStart::Skipped(why) if why.contains("npm publish")), "{asks:?}");
	let denied =
		w.t.ctx
			.starts
			.start_scheduled("demo", &schedule("Push.", &["git push --force origin main"], false))
			.await
			.unwrap();
	assert!(matches!(&denied, ScheduleStart::Skipped(why) if why.contains("denied")), "{denied:?}");
	assert!(w.asked.lock().unwrap().is_empty() && w.t.herdr.of("createWorktree").is_empty());
}
