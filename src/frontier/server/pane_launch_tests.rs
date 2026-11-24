//! Tests of `pane_launch.rs`: `pane-launch.test.ts` and `pane-hook-env.test.ts` (both whole), and the pane and backoff
//! parts of `service-relaunch.test.ts` and `service-regressions.test.ts` (SVC-3's backoff) that don't need the service.

use super::*;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::herdr::Herdr;
use crate::frontier::server::hunk::fake_hunk::fake_hunk;
use crate::frontier::server::mise::tests::captured_shim;
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::task_file::{NewTask, new_task, task_dir};
use crate::frontier::server::thread::Thread;
use crate::shared::bridge_wire::BRIDGE_EXTENSION;
use crate::shared::env::overlay;
use crate::shared::fsx::free_port;
use crate::shared::log::logs_dir;
use crate::shared::proctree::{kill_tree, pid_alive};
use crate::shared::push_hook::PUSH_HOOK_ROLE;
use crate::shared::testing::temp_dir;

const PROJECT: &str = "p-0123456789abcdef";

/// An environment variable set in the test overlay until it drops.
struct Var(&'static str, Option<Option<String>>);

impl Var {
	fn set(name: &'static str, value: Option<&str>) -> Self {
		Self(name, overlay::set(name, value.map(str::to_owned)))
	}
}

impl Drop for Var {
	fn drop(&mut self) {
		overlay::restore(self.0, self.1.take());
	}
}

/// A context over a fake herdr with no shipped prompt and no mise.
fn world() -> TestContext {
	test_context(|options| {
		options.omp = Some(vec!["omp".to_owned()]);
		options.pane_prompt = Some(None);
		options.mise = Some(None);
		options.boss_key = Some("boss-key-of-this-run".to_owned());
	})
}

fn strings(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

/// The guard's `--config` and the bridge extension: what every pane's omp starts with after `--continue` (0450, 0675).
fn guard() -> Vec<String> {
	strings(&["--config", &shipped("guard.yml"), "-e", &shipped(BRIDGE_EXTENSION)])
}

fn overlay_args() -> Vec<String> {
	vec!["--config".to_owned(), overlay_file(PROJECT).display().to_string()]
}

fn join(parts: &[&[String]]) -> Vec<String> {
	parts.iter().flat_map(|part| part.iter().cloned()).collect()
}

/// What herdr's last `respawn` got: its argv and env.
struct Given {
	command: Vec<String>,
	env: Vec<(String, String)>,
}

fn last_respawn(t: &TestContext) -> Given {
	let calls = t.herdr.of("respawn");
	let given = calls.last().expect("a respawn")[1].clone();
	let command =
		given["command"].as_array().expect("an argv").iter().map(|word| word.as_str().unwrap().to_owned()).collect();
	let env = given["env"]
		.as_object()
		.map(|env| env.iter().map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned())).collect())
		.unwrap_or_default();
	Given { command, env }
}

fn env_of(env: &[(String, String)], name: &str) -> Option<String> {
	env.iter().find(|(have, _)| have == name).map(|(_, value)| value.clone())
}

/// A new pane in a workspace of `cwd`; its id.
async fn new_pane(t: &TestContext, cwd: &Path) -> String {
	let label = format!("w{}", t.herdr.methods().len());
	t.herdr.ensure_workspace(&label, cwd, None, None).await.unwrap();
	t.herdr.state().pane_list.last().expect("a pane").pane_id.clone()
}

fn thread_of(task: Option<TaskRecord>) -> ThreadRef {
	let project = Project {
		id: PROJECT.to_owned(),
		identity: String::new(),
		name: "demo".to_owned(),
		main_path: "/code/demo".into(),
		git: true,
		paths: Vec::new(),
	};
	let mut thread = Thread::new("1-1", "w-1", project, "Migrate", "feature/migrate");
	thread.task = task;
	Shared::new(thread)
}

fn migrate(heavy: Option<bool>) -> TaskRecord {
	new_task(
		NewTask {
			project_id: PROJECT.to_owned(),
			project: "demo".to_owned(),
			title: "Migrate".to_owned(),
			brief: "Migrate it.".to_owned(),
			heavy,
			..NewTask::default()
		},
		None,
	)
}

/// Starts omp again in a new pane of `cwd` as `how` says; what herdr was given.
async fn respawned(t: &TestContext, how: Respawn<'_>) -> Given {
	let pane = new_pane(t, how.cwd).await;
	t.ctx.panes.respawn(&pane, how).await.unwrap();
	last_respawn(t)
}

fn plain<'a>(cwd: &'a Path, env: Option<Env>) -> Respawn<'a> {
	Respawn { cwd, project_id: PROJECT, env, restore: None, thread: None }
}

#[tokio::test]
async fn a_heavy_threads_omp_starts_again_with_heavy_yml_after_the_overlay_resuming_or_restoring_a_thread_that_isnt_heavy_without()
 {
	let t = world();
	let cwd = t.ctx.options.sessions_dir.clone().unwrap().join("demo.wt").join("migrate");
	let task = migrate(Some(true));
	let thread = thread_of(Some(task.clone()));
	let prompt = task_dir(&task.project_id, &task.id).join("prompt.md").display().to_string();
	let heavy = strings(&["--config", &shipped("heavy.yml")]);
	let flags = strings(&THREAD_FLAGS);
	let resuming = respawned(&t, Respawn { thread: Some(&thread), ..plain(&cwd, Some(Vec::new())) }).await;
	assert_eq!(
		resuming.command,
		join(&[
			&strings(&["omp", "--continue", "--append-system-prompt", &prompt]),
			&guard(),
			&flags,
			&overlay_args(),
			&heavy,
		])
	);
	let session = t.ctx.options.sessions_dir.clone().unwrap().join("s.jsonl");
	let restoring =
		respawned(&t, Respawn { thread: Some(&thread), restore: Some(&session), ..plain(&cwd, Some(Vec::new())) })
			.await;
	assert_eq!(
		restoring.command,
		join(&[
			&strings(&["omp", "--append-system-prompt", &prompt]),
			&guard(),
			&flags,
			&strings(&["--restore", &session.display().to_string()]),
			&overlay_args(),
			&heavy,
		])
	);
	let light = thread_of(Some(migrate(None)));
	let given = respawned(&t, Respawn { thread: Some(&light), ..plain(&cwd, Some(Vec::new())) }).await;
	assert!(!given.command.contains(&heavy[1]));
}

#[tokio::test]
async fn every_panes_omp_starts_again_with_the_bridge_extension_a_threads_with_its_flags_too_and_the_env_has_no_check_argv()
 {
	let t = world();
	let cwd = t.ctx.options.sessions_dir.clone().unwrap().join("demo");
	let project = respawned(&t, plain(&cwd, Some(Vec::new()))).await;
	assert_eq!(project.command, join(&[&strings(&["omp", "--continue"]), &guard(), &overlay_args()]));
	assert!(env_of(&project.env, PANE_KEY_ENV).is_some());
	let session = cwd.join("s.jsonl");
	let restored = respawned(&t, Respawn { restore: Some(&session), ..plain(&cwd, Some(Vec::new())) }).await;
	assert_eq!(
		restored.command,
		join(&[
			&strings(&["omp"]),
			&guard(),
			&strings(&["--restore", &session.display().to_string()]),
			&overlay_args(),
		])
	);
	let thread = thread_of(None);
	let given = respawned(&t, Respawn { thread: Some(&thread), ..plain(&cwd, Some(Vec::new())) }).await;
	assert_eq!(
		given.command,
		join(&[&strings(&["omp", "--continue"]), &guard(), &strings(&THREAD_FLAGS), &overlay_args()])
	);
}

fn hook_command() -> String {
	let words: Vec<String> =
		self_command(None).iter().map(|word| format!("'{}'", word.display().to_string().replace('\\', "/"))).collect();
	format!("{} {PUSH_HOOK_ROLE}", words.join(" "))
}

#[tokio::test]
async fn a_project_or_thread_pane_starts_with_the_pre_push_hook_in_gits_environment_the_check_env_has_none() {
	let t = world();
	let _count = Var::set("GIT_CONFIG_COUNT", None);
	let env = t.ctx.panes.pane_env().clone();
	let expected = [
		("GIT_CONFIG_COUNT", "3".to_owned()),
		("GIT_CONFIG_KEY_0", "hook.jocasta.event".to_owned()),
		("GIT_CONFIG_VALUE_0", "pre-push".to_owned()),
		("GIT_CONFIG_KEY_1", "hook.jocasta.command".to_owned()),
		("GIT_CONFIG_VALUE_1", hook_command()),
		("GIT_CONFIG_KEY_2", "hook.jocasta.enabled".to_owned()),
		("GIT_CONFIG_VALUE_2", "true".to_owned()),
	];
	for (name, value) in expected {
		assert_eq!(env_of(&env, name), Some(value), "{name}");
	}
	assert_eq!(t.ctx.panes.check_env().get("GIT_CONFIG_COUNT"), None);
}

#[tokio::test]
async fn an_environment_that_already_holds_git_config_count_keeps_its_entries_the_hooks_follow() {
	let t = world();
	let _count = Var::set("GIT_CONFIG_COUNT", Some("2"));
	let env = t.ctx.panes.pane_env().clone();
	assert_eq!(env_of(&env, "GIT_CONFIG_COUNT").as_deref(), Some("5"));
	assert_eq!(env_of(&env, "GIT_CONFIG_KEY_2").as_deref(), Some("hook.jocasta.event"));
	assert_eq!(env_of(&env, "GIT_CONFIG_KEY_3").as_deref(), Some("hook.jocasta.command"));
	assert_eq!(env_of(&env, "GIT_CONFIG_KEY_4").as_deref(), Some("hook.jocasta.enabled"));
	assert_eq!(env_of(&env, "GIT_CONFIG_KEY_0"), None);
}

#[tokio::test]
async fn a_pane_started_again_gets_the_hook_and_its_own_pane_key() {
	let t = world();
	let _count = Var::set("GIT_CONFIG_COUNT", None);
	let cwd = t.ctx.options.sessions_dir.clone().unwrap();
	let given = respawned(&t, plain(&cwd, None)).await;
	assert_eq!(env_of(&given.env, "GIT_CONFIG_COUNT").as_deref(), Some("3"));
	assert_eq!(env_of(&given.env, "GIT_CONFIG_VALUE_1"), Some(hook_command()));
	assert!(env_of(&given.env, PANE_KEY_ENV).is_some_and(|key| !key.is_empty()));
}

#[tokio::test]
async fn the_boss_pane_starts_with_the_hook_too() {
	let t = world();
	let _count = Var::set("GIT_CONFIG_COUNT", None);
	t.ctx.panes.ensure_boss(true).await.unwrap();
	let boss = t.ctx.boss().expect("the boss").with(|boss| boss.pane_id.clone());
	let given = t.herdr.env_of(&boss).expect("the boss pane's env");
	assert_eq!(env_of(&given, "GIT_CONFIG_COUNT").as_deref(), Some("3"));
	assert_eq!(env_of(&given, "GIT_CONFIG_VALUE_2").as_deref(), Some("true"));
	assert!(env_of(&given, PANE_KEY_ENV).is_some_and(|key| !key.is_empty()));
	assert_eq!(env_of(&given, BOSS_KEY_ENV).as_deref(), Some("boss-key-of-this-run"));
}

#[test]
fn after_overlay_is_heavy_yml_for_a_heavy_task_and_nothing_for_another_or_none() {
	let heavy = migrate(Some(true));
	assert_eq!(after_overlay(Some(&heavy)), strings(&["--config", &shipped("heavy.yml")]));
	assert!(after_overlay(Some(&migrate(None))).is_empty());
	assert!(after_overlay(Some(&migrate(Some(false)))).is_empty());
	assert!(after_overlay(None).is_empty());
}

#[test]
fn mises_env_loses_herdrs_jocastas_and_omps_keys_in_any_case() {
	let kept = without_jocasta_keys(
		[("PATH", "a"), ("HERDR_PANE", "b"), ("jocasta_x", "c"), ("Pi_Config", "d"), ("PIPE", "e"), ("API_PORT", "f")]
			.map(|(name, value)| (name.to_owned(), value.to_owned())),
	);
	assert_eq!(
		kept,
		vec![
			("PATH".to_owned(), "a".to_owned()),
			("PIPE".to_owned(), "e".to_owned()),
			("API_PORT".to_owned(), "f".to_owned())
		]
	);
}

#[tokio::test]
async fn the_panes_omp_is_omp_resuming_with_the_guard_and_the_options_say_how_omp_hunk_and_the_boss_key_are() {
	let t = world();
	let panes = &t.ctx.panes;
	assert_eq!(panes.omp(), ["omp"]);
	assert_eq!(panes.pane_omp(), join(&[&strings(&["omp", "--continue"]), &guard()]));
	assert_eq!(panes.hunk_port(), HUNK_PORT);
	assert_eq!(panes.boss_key(), "boss-key-of-this-run");
	let names: Vec<&str> = panes.pane_env().iter().map(|(name, _)| name.as_str()).collect();
	assert_eq!(
		names,
		[
			"HUNK_MCP_PORT",
			"HUNK_DISABLE_UPDATE_NOTICE",
			"GIT_CONFIG_COUNT",
			"GIT_CONFIG_KEY_0",
			"GIT_CONFIG_VALUE_0",
			"GIT_CONFIG_KEY_1",
			"GIT_CONFIG_VALUE_1",
			"GIT_CONFIG_KEY_2",
			"GIT_CONFIG_VALUE_2",
		][..names.len()]
	);
	drop(t);
	let other = test_context(|options| {
		options.omp = Some(strings(&["bun", "omp.ts"]));
		options.hunk_port = Some(4242);
		options.pane_prompt = Some(None);
	});
	assert_eq!(other.ctx.panes.omp(), ["bun", "omp.ts"]);
	assert_eq!(other.ctx.panes.hunk_port(), 4242);
	assert_eq!(env_of(other.ctx.panes.pane_env(), "HUNK_MCP_PORT").as_deref(), Some("4242"));
	// Without a key in the options, a run's key is 32 random bytes in hex, the same every time it is asked.
	let key = other.ctx.panes.boss_key().to_owned();
	assert_eq!(key.len(), 64);
	assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
	assert_eq!(other.ctx.panes.boss_key(), key);
}

#[tokio::test]
async fn the_panes_prompt_is_appended_without_its_leading_comment_as_a_copy_in_data() {
	let folder = temp_dir("pane-prompt");
	let with = folder.path().join("ponytail.md");
	std::fs::write(&with, "<!-- license -->\n\n  Be brief.\n").unwrap();
	let plain_file = folder.path().join("plain.md");
	std::fs::write(&plain_file, "Be brief.\n").unwrap();
	let t = test_context(|options| options.pane_prompt = Some(Some(with.clone())));
	let copy = data_dir().join("ponytail.md");
	assert_eq!(
		t.ctx.panes.pane_omp(),
		join(&[&strings(&["omp", "--continue", "--append-system-prompt"]), &[copy.display().to_string()], &guard()])
	);
	assert_eq!(std::fs::read_to_string(&copy).unwrap(), "Be brief.\n");
	// No comment: the file itself; unreadable: the file itself.
	assert_eq!(stripped_prompt(&plain_file), plain_file);
	assert_eq!(stripped_prompt(&folder.path().join("missing.md")), folder.path().join("missing.md"));
}

#[tokio::test]
async fn a_boss_pane_starts_with_this_runs_key_the_hook_and_its_own_pane_key_and_is_keyed() {
	let t = world();
	t.ctx.panes.ensure_boss(true).await.unwrap();
	let boss = t.ctx.boss().expect("the boss").get();
	assert_eq!(boss.workspace_id, t.herdr.state().workspace_list[0].id);
	let given = t.herdr.env_of(&boss.pane_id).expect("an env");
	assert_eq!(env_of(&given, BOSS_KEY_ENV).as_deref(), Some("boss-key-of-this-run"));
	assert!(env_of(&given, PANE_KEY_ENV).is_some_and(|key| key.len() == 64));
	assert_eq!(t.ctx.panes.boss_keyed(), Some(boss.pane_id.clone()));
	assert_eq!(t.ctx.launched.key_of(&boss.pane_id), env_of(&given, PANE_KEY_ENV));
	// A boss pane running omp whose key is this run's isn't stale; another is, once it is idle.
	let pane = |id: &str, state| PaneInfo {
		pane_id: id.to_owned(),
		agent: Some("omp".to_owned()),
		agent_state: state,
		..PaneInfo::default()
	};
	let panes = &t.ctx.panes;
	assert!(!panes.unkeyed_boss(&pane(&boss.pane_id, AgentState::Idle)));
	assert!(panes.unkeyed_boss(&pane("other", AgentState::Idle)));
	assert!(panes.unkeyed_boss(&pane("other", AgentState::Done)));
	assert!(!panes.unkeyed_boss(&pane("other", AgentState::Working)));
	assert!(!panes.unkeyed_boss(&pane("other", AgentState::Blocked)));
	assert!(!panes.unkeyed_boss(&PaneInfo { agent: None, ..pane("other", AgentState::Idle) }));
}

#[tokio::test]
async fn a_boss_pane_ensured_again_keeps_its_state() {
	let t = world();
	t.ctx.panes.ensure_boss(true).await.unwrap();
	let boss = t.ctx.boss().expect("the boss");
	boss.with(|boss| {
		boss.state = AgentState::Working;
		boss.event_at = Some(7);
		boss.prompted_at = Some(8);
	});
	let before = boss.get();
	t.ctx.panes.ensure_boss(false).await.unwrap();
	let again = t.ctx.boss().expect("the boss").get();
	assert_eq!((again.pane_id, again.workspace_id), (before.pane_id, before.workspace_id));
	assert_eq!((again.state, again.event_at, again.prompted_at), (AgentState::Working, Some(7), Some(8)));
	// The boss's own start isn't repeated.
	assert_eq!(t.herdr.of("ensureWorkspace").len(), 1);
	let workspace = t.ctx.boss().expect("the boss").with(|boss| boss.workspace_id.clone());
	t.ctx.herdr.close_workspace(&workspace).await.unwrap();
	t.ctx.panes.ensure_boss(false).await.unwrap();
	assert_eq!(t.herdr.of("ensureWorkspace").len(), 2);
}

#[tokio::test]
async fn a_pane_started_again_records_the_new_pane_with_its_folder_and_key_and_takes_the_env_it_is_given() {
	let t = world();
	let cwd = t.ctx.options.sessions_dir.clone().unwrap();
	let pane = new_pane(&t, &cwd).await;
	let env = vec![("MY_VAR".to_owned(), "1".to_owned()), (PANE_KEY_ENV.to_owned(), "stale".to_owned())];
	let again = t.ctx.panes.respawn(&pane, plain(&cwd, Some(env))).await.unwrap();
	let given = last_respawn(&t);
	let key = env_of(&given.env, PANE_KEY_ENV).unwrap();
	assert_ne!(key, "stale");
	assert_eq!(env_of(&given.env, "MY_VAR").as_deref(), Some("1"));
	assert_eq!(given.env.iter().filter(|(name, _)| name == PANE_KEY_ENV).count(), 1);
	assert_eq!(t.ctx.launched.cwd_of(&again), Some(cwd.display().to_string()));
	assert_eq!(t.ctx.launched.key_of(&again), Some(key));
	// Without an env of its own, the hunk variables and the hook are the pane's, and mise's none (`mise: null`).
	let bare = t.ctx.panes.respawn(&again, plain(&cwd, None)).await.unwrap();
	let given = last_respawn(&t);
	assert_eq!(env_of(&given.env, "HUNK_MCP_PORT").as_deref(), Some(HUNK_PORT.to_string().as_str()));
	assert!(t.ctx.launched.has(&bare));
}

#[tokio::test]
async fn a_task_with_a_brief_gives_its_thread_pane_the_launch_prompt_in_the_shipped_prompts_place() {
	let t = world();
	let panes = &t.ctx.panes;
	let task = migrate(None);
	let command = join(&[panes.pane_omp(), &strings(&THREAD_FLAGS)]);
	// Unchanged without a recorded brief, or a blank one.
	assert_eq!(panes.task_prompted(&command, None).unwrap(), command);
	let blank = TaskRecord { brief: " \n".to_owned(), ..task.clone() };
	assert_eq!(panes.task_prompted(&command, Some(&blank)).unwrap(), command);
	let prompt = task_dir(&task.project_id, &task.id).join("prompt.md");
	// No prompt in the argv yet: it goes in before the guard's `--config`.
	let put = panes.task_prompted(&command, Some(&task)).unwrap();
	assert_eq!(
		put,
		join(&[
			&strings(&["omp", "--continue", "--append-system-prompt", &prompt.display().to_string()]),
			&guard(),
			&strings(&THREAD_FLAGS)
		])
	);
	let text = std::fs::read_to_string(&prompt).unwrap();
	assert!(text.starts_with("# Your task: Migrate (demo)\n"), "{text}");
	assert!(text.contains("Migrate it."));
	// A prompt in the argv is replaced by the file.
	let with = strings(&["omp", "--append-system-prompt", "ponytail.md", "--config", "g.yml"]);
	assert_eq!(
		panes.task_prompted(&with, Some(&task)).unwrap(),
		strings(&["omp", "--append-system-prompt", &prompt.display().to_string(), "--config", "g.yml"])
	);
	// No `--config` either: `slice(0, -1)` puts it before the last word.
	assert_eq!(
		panes.task_prompted(&strings(&["omp", "x"]), Some(&task)).unwrap(),
		strings(&["omp", "--append-system-prompt", &prompt.display().to_string(), "x"])
	);
}

#[tokio::test]
async fn the_check_env_is_the_services_own_without_jocastas_herdrs_and_omps_keys_and_with_the_hunk_variables() {
	let t = world();
	let _a = Var::set("JOCASTA_SOMETHING", Some("1"));
	let _b = Var::set("herdr_pane_id", Some("2"));
	let _c = Var::set("PI_CONFIG_DIR", Some("3"));
	let _d = Var::set("JOCASTA_TEST_PLAIN", Some("4"));
	let _e = Var::set("MY_PLAIN_VARIABLE", Some("5"));
	let _f = Var::set("GIT_CONFIG_COUNT", Some("7"));
	let checks = t.ctx.panes.check_env();
	assert_eq!(checks.get("MY_PLAIN_VARIABLE").map(String::as_str), Some("5"));
	assert_eq!(checks.get("GIT_CONFIG_COUNT").map(String::as_str), Some("7"));
	for gone in ["JOCASTA_SOMETHING", "herdr_pane_id", "PI_CONFIG_DIR", "JOCASTA_TEST_PLAIN", "JOCASTA_DATA_DIR"] {
		assert!(!checks.contains_key(gone), "{gone}");
	}
	assert_eq!(checks.get("HUNK_MCP_PORT").map(String::as_str), Some(HUNK_PORT.to_string().as_str()));
	assert_eq!(checks.get("HUNK_DISABLE_UPDATE_NOTICE").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn a_thread_panes_env_is_the_hunk_variables_the_threads_own_and_with_the_layer_off_the_layers_switch() {
	let t = world();
	let project = thread_of(None).with(|thread| thread.project.clone());
	let path = t.ctx.options.sessions_dir.clone().unwrap().join("demo.wt").join("Readme Line");
	let env = t.ctx.panes.thread_pane_env(&project, &path).await.unwrap();
	assert_eq!(env_of(&env, "HUNK_MCP_PORT").as_deref(), Some(HUNK_PORT.to_string().as_str()));
	assert_eq!(env_of(&env, "JOCASTA_THREAD").as_deref(), Some("Readme Line"));
	assert_eq!(env_of(&env, "COMPOSE_PROJECT_NAME").as_deref(), Some("demo-readme-line"));
	assert_eq!(env_of(&env, "JOCASTA_PORT_OFFSET").as_deref(), Some("10"));
	assert_eq!(env_of(&env, LAYER_ENV), None);
	// With the layer off in the service's environment when it was built.
	let mut off = None;
	drop(t);
	let t = test_context(|options| {
		options.mise = Some(None);
		off = Some(Var::set(LAYER_ENV, Some("off")));
	});
	let _off = off;
	assert!(!t.ctx.layer);
	let env = t.ctx.panes.thread_pane_env(&project, &path).await.unwrap();
	assert_eq!(env_of(&env, LAYER_ENV).as_deref(), Some("off"));
}

#[tokio::test]
async fn mises_env_is_merged_into_a_panes_without_herdrs_jocastas_or_omps_keys_and_none_without_mise() {
	let t = world();
	let env = vec![("MINE".to_owned(), "1".to_owned())];
	assert_eq!(t.ctx.panes.with_mise(PROJECT, Path::new("/w"), &env).await, env);
	let folder = temp_dir("pane-mise");
	let shim = captured_shim(folder.path(), "mise", "frontier::server::mise::tests::fake_mise");
	let argv = vec![shim.to_string_lossy().into_owned()];
	drop(t);
	let t = test_context(|options| options.mise = Some(Some(argv)));
	let answer =
		serde_json::json!({ "API_PORT": "3010", "JOCASTA_X": "1", "HERDR_Y": "2", "pi_z": "3", "MINE": "from-mise" });
	let env = vec![
		("MINE".to_owned(), "1".to_owned()),
		("FAKE_MISE_PARANOID".to_owned(), "[\"config\"]\n".to_owned()),
		("FAKE_MISE_TRUST".to_owned(), "~/src/demo/mise.toml: trusted\n".to_owned()),
		("FAKE_MISE_ENV_JSON".to_owned(), answer.to_string()),
	];
	let merged = t.ctx.panes.with_mise(PROJECT, folder.path(), &env).await;
	assert_eq!(env_of(&merged, "API_PORT").as_deref(), Some("3010"));
	assert_eq!(env_of(&merged, "MINE").as_deref(), Some("from-mise"));
	assert_eq!(merged[0].0, "MINE");
	assert!(env_of(&merged, "MISE_ENV").is_some_and(|value| value.contains("jocasta-")));
	for gone in ["JOCASTA_X", "HERDR_Y", "pi_z"] {
		assert_eq!(env_of(&merged, gone), None, "{gone}");
	}
}

#[tokio::test]
async fn hunk_is_the_seams_argv_else_the_shipped_or_npm_binary() {
	let t = test_context(|options| options.hunk = Some(Some(strings(&["hunk-fake", "--x"]))));
	assert_eq!(t.ctx.panes.hunk(), Some(strings(&["hunk-fake", "--x"])));
	drop(t);
	let bare = world();
	assert_eq!(bare.ctx.panes.hunk(), find_hunk().map(|found| vec![found.display().to_string()]));
}

#[tokio::test]
async fn jocastas_hunk_daemon_is_started_when_down_remembered_by_pid_and_start_and_ended_once() {
	let port = free_port().unwrap();
	let t = test_context(move |options| options.hunk_port = Some(port));
	let folder = temp_dir("pane-hunk");
	let fake = fake_hunk(folder.path());
	assert!(t.ctx.panes.state().hunk_daemon.is_none());
	t.ctx.panes.ensure_hunk_daemon(&fake).await;
	let daemon = t.ctx.panes.state().hunk_daemon.expect("the daemon was remembered");
	assert!(pid_alive(daemon.pid));
	// Up already: nothing starts, nothing changes.
	t.ctx.panes.ensure_hunk_daemon(&fake).await;
	assert_eq!(t.ctx.panes.state().hunk_daemon.map(|d| d.pid), Some(daemon.pid));
	t.ctx.panes.end_hunk_daemon().await;
	let ended = crate::shared::poll::wait_for(|| async { Ok(!pid_alive(daemon.pid)) }, 10_000, 50).await.unwrap();
	assert!(ended, "the daemon is still running");
	assert!(t.ctx.panes.state().hunk_daemon.is_none());
	// Nothing to end the second time.
	t.ctx.panes.end_hunk_daemon().await;
	kill_tree(daemon.pid).await;
}

fn ended_pane(t: &TestContext, folder: &Path) -> PaneInfo {
	let pane = |agent: Option<&str>| PaneInfo {
		pane_id: "p1".to_owned(),
		tab_id: "t1".to_owned(),
		workspace_id: "w1".to_owned(),
		cwd: Some(folder.to_path_buf()),
		terminal_id: Some("term1".to_owned()),
		agent: agent.map(str::to_owned),
		agent_state: if agent.is_some() { AgentState::Idle } else { AgentState::Unknown },
		..PaneInfo::default()
	};
	t.ctx.launched.add("p1", &folder.display().to_string(), None).unwrap();
	t.ctx.launched.seen(&[pane(Some("omp"))]).unwrap();
	pane(None)
}

fn logged() -> String {
	std::fs::read_dir(logs_dir())
		.map(|dir| dir.flatten().filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect())
		.unwrap_or_default()
}

#[test]
fn the_wait_before_omp_starts_again_doubles_from_10_seconds_to_5_minutes() {
	let waits: Vec<i64> = (1..=7).map(again_wait).collect();
	assert_eq!(waits, [10_000, 20_000, 40_000, 80_000, 160_000, 300_000, 300_000]);
}

#[tokio::test]
async fn an_omp_that_keeps_ending_in_a_pane_starts_again_at_once_then_after_a_doubling_wait_said_once_and_afresh_after_10_minutes()
 {
	let t = world();
	let folder = temp_dir("backoff");
	let ended = ended_pane(&t, folder.path());
	let backoff = OmpBackoff::new(t.ctx.panes.link.clone());
	let t0 = 1_000_000;
	assert!(backoff.omp_ended_at(&ended, t0));
	assert!(!backoff.omp_ended_at(&ended, t0 + 1_000));
	assert!(!backoff.omp_ended_at(&ended, t0 + 2_000));
	let said = format!("omp ended in pane p1 ({}) 2 times in a row; it starts again in 9 s.", folder.path().display());
	assert_eq!(logged().matches(&said).count(), 1, "{}", logged());
	assert!(backoff.omp_ended_at(&ended, t0 + 10_000));
	assert!(!backoff.omp_ended_at(&ended, t0 + 11_000));
	assert!(logged().contains(&format!(
		"omp ended in pane p1 ({}) 3 times in a row; it starts again in 19 s.",
		folder.path().display()
	)));
	assert!(backoff.omp_ended_at(&ended, t0 + 30_000));
	// It ran 10 minutes since: afresh, no wait.
	assert!(backoff.omp_ended_at(&ended, t0 + 30_000 + OMP_AGAIN_RESET_MS));
	assert!(!backoff.omp_ended_at(&ended, t0 + 31_000 + OMP_AGAIN_RESET_MS));
	// A pane without an ended omp, or without a folder, never starts again here.
	let running = PaneInfo { pane_id: "p9".to_owned(), cwd: Some(folder.path().to_path_buf()), ..PaneInfo::default() };
	assert!(!backoff.omp_ended_at(&running, t0));
	assert!(!backoff.omp_ended_at(&PaneInfo { cwd: None, ..ended.clone() }, t0));
	assert!(!backoff.omp_ended_at(&PaneInfo { cwd: Some(PathBuf::new()), ..ended }, t0));
	assert!(!backoff.omp_ended(&running));
}

#[tokio::test]
async fn a_boss_pane_that_keeps_closing_opens_again_at_once_then_after_the_doubling_wait_said_once() {
	let t = world();
	let backoff = OmpBackoff::new(t.ctx.panes.link.clone());
	let t0 = 2_000_000;
	assert!(backoff.boss_closed_at(t0));
	assert!(!backoff.boss_closed_at(t0 + 1_000));
	assert!(!backoff.boss_closed_at(t0 + 2_000));
	assert_eq!(logged().matches("The boss pane closed 2 times in a row; it opens again in 9 s.").count(), 1);
	assert!(backoff.boss_closed_at(t0 + 10_000));
	assert!(!backoff.boss_closed_at(t0 + 29_000));
	assert!(backoff.boss_closed_at(t0 + 30_000));
}

#[tokio::test]
async fn a_pane_whose_respawn_failed_waits_the_doubling_wait_its_failure_said_once_per_reason_and_a_success_clears_it()
{
	let t = world();
	let folder = temp_dir("failing");
	let cwd = folder.path();
	let backoff = OmpBackoff::new(t.ctx.panes.link.clone());
	assert!(backoff.due(Some(cwd)));
	assert!(backoff.due(None));
	backoff.respawning(cwd, async { Err::<(), _>(Error::other("no relay")) }).await;
	assert!(!backoff.due(Some(cwd)));
	// A pane in another folder is due.
	assert!(backoff.due(Some(&folder.path().join("elsewhere"))));
	let said = format!("omp didn't start again in {}: no relay", cwd.display());
	assert_eq!(logged().matches(&said).count(), 1);
	// The same failure again: waited longer, not said again.
	let key = path_key(&real_path(cwd));
	backoff.settled(&key, cwd, Some(Error::other("no relay")), 1_000);
	assert_eq!(logged().matches(&said).count(), 1);
	assert!(!backoff.due_at(Some(cwd), 1_000 + 19_999));
	assert!(backoff.due_at(Some(cwd), 1_000 + 20_000));
	// Another reason is said.
	backoff.settled(&key, cwd, Some(Error::other("no herdr")), 2_000);
	assert_eq!(logged().matches(&format!("omp didn't start again in {}: no herdr", cwd.display())).count(), 1);
	assert!(!backoff.due_at(Some(cwd), 2_000 + 39_999));
	assert!(backoff.due_at(Some(cwd), 2_000 + 40_000));
	// A respawn that works clears the failures and what was said: the same reason is said again.
	backoff.respawning(cwd, async { Ok(()) }).await;
	assert!(backoff.due(Some(cwd)));
	backoff.respawning(cwd, async { Err::<(), _>(Error::other("no herdr")) }).await;
	assert_eq!(logged().matches(&format!("omp didn't start again in {}: no herdr", cwd.display())).count(), 2);
}
