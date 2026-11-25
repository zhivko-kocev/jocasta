use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::shared::testing::temp_jocasta;
use crate::shared::yaml::parse_value;

/// A boss pane's env with its pane key (0578), whatever key it got: the key checked and replaced by `KEY`.
fn keyed(value: &Value) -> Value {
	match value {
		Value::Object(map) => Value::Object(
			map.iter()
				.map(|(name, inner)| {
					if name == PANE_KEY_ENV {
						let key = inner.as_str().expect("a string key");
						assert!(key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()), "{key}");
						(name.clone(), json!("KEY"))
					} else {
						(name.clone(), keyed(inner))
					}
				})
				.collect(),
		),
		Value::Array(items) => Value::Array(items.iter().map(keyed).collect()),
		other => other.clone(),
	}
}

fn expected_env(vars: &[(&str, &str)]) -> Value {
	let mut map: Map<String, Value> = vars.iter().map(|(name, value)| ((*name).to_owned(), json!(value))).collect();
	map.insert(PANE_KEY_ENV.to_owned(), json!("KEY"));
	Value::Object(map)
}

fn env_of(vars: &[(&str, &str)]) -> Env {
	vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

fn command() -> Vec<String> {
	boss_command(&["omp".to_owned()], None)
}

fn home_string() -> String {
	boss_home().display().to_string()
}

fn pane_key(herdr: &FakeSurface, pane_id: &str) -> String {
	let env = herdr.env_of(pane_id).expect("the pane has an env");
	env.iter().find(|(name, _)| name == PANE_KEY_ENV).map(|(_, key)| key.clone()).expect("its pane key")
}

fn labels(herdr: &FakeSurface) -> Vec<String> {
	herdr.state().workspace_list.iter().map(|w| w.label.clone()).collect()
}

fn ensure<'a>(launched: &'a LaunchedPanes) -> EnsureBossOptions<'a> {
	EnsureBossOptions::new(command(), launched)
}

fn same_text(a: &Path, b: &Path) {
	assert_eq!(fs::read_to_string(a).unwrap(), fs::read_to_string(b).unwrap());
}

fn recorded(calls: &[Vec<Value>]) -> Vec<Vec<Value>> {
	calls.iter().map(|args| args.iter().map(keyed).collect()).collect()
}

#[tokio::test]
async fn workspace_jocasta_is_made_first_made_first_again_after_he_closes_it_with_the_guard_its_manual_and_its_skills()
{
	let j = temp_jocasta();
	let herdr = FakeSurface::new();
	herdr.ensure_workspace("demo", j.root(), None, None).await.unwrap();
	let command = command();
	let shipped = shipped_config_dir(None);
	assert_eq!(
		command,
		vec![
			"omp".to_owned(),
			"--continue".to_owned(),
			"--config".to_owned(),
			shipped.join("boss.yml").display().to_string(),
			"--config".to_owned(),
			shipped.join("guard.yml").display().to_string(),
			"-e".to_owned(),
			shipped.join("bridge-extension.ts").display().to_string(),
		]
	);
	let launched = LaunchedPanes::new(None);
	let pane = ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	assert_eq!(labels(&herdr), vec!["Jocasta", "demo"]);
	let last = herdr.of("ensureWorkspace").pop().unwrap();
	assert_eq!(
		recorded(&[last]),
		vec![vec![json!("Jocasta"), json!(home_string()), json!(command), expected_env(&[])]]
	);
	// Its pane key is kept with the pane, in panes.json, so a later run knows it (0578).
	let key = pane_key(&herdr, &pane.pane_id);
	assert!(key.len() == 64 && key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
	assert_eq!(launched.key_of(&pane.pane_id), Some(key.clone()));
	assert_eq!(LaunchedPanes::new(None).pane_of_key(Some(&key)), Some(pane.pane_id.clone()));
	// One character off: the last one changed, whatever it was.
	let forged = format!("{}{}", &key[..63], if key.ends_with('0') { "1" } else { "0" });
	assert_eq!(LaunchedPanes::new(None).pane_of_key(Some(&forged)), None);
	assert_eq!(LaunchedPanes::new(None).pane_of_key(None), None);
	let omp = boss_home().join(".omp");
	same_text(&omp.join("APPEND_SYSTEM.md"), &shipped.join("boss-manual.md"));
	assert!(launched.has(&pane.pane_id));
	assert!(data_dir().join("panes.json").exists());
	// The rare procedures are skills in the boss's folder; save-lesson carries the shipped TDD lesson (0461).
	for name in ["save-lesson", "mise-setup", "leftover-worktrees"] {
		same_text(
			&omp.join("skills").join(name).join("SKILL.md"),
			&shipped.join("boss-skills").join(name).join("SKILL.md"),
		);
	}
	same_text(&omp.join("skills").join("save-lesson").join("tdd.md"), &shipped.join("lessons").join("tdd.md"));

	// Already there and first: nothing moves, nothing is made.
	let calls = herdr.methods().len();
	assert_eq!(ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap().pane_id, pane.pane_id);
	assert_eq!(herdr.methods()[calls..], Vec::<String>::new());

	herdr.close_workspace(&pane.workspace_id).await.unwrap();
	let again = ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	assert_ne!(again.pane_id, pane.pane_id);
	assert_eq!(labels(&herdr), vec!["Jocasta", "demo"]);
	assert_eq!(herdr.of("moveWorkspace").len(), 2);
}

#[tokio::test]
async fn the_boss_panes_omp_starts_with_its_env_and_a_stale_one_starts_again_with_it() {
	let _j = temp_jocasta();
	let herdr = FakeSurface::new();
	let command = command();
	let launched = LaunchedPanes::new(None);
	let env1 = [("JOCASTA_BOSS_KEY", "run-1")];
	let started = Arc::new(Mutex::new(Vec::<String>::new()));
	let starts = |started: &Arc<Mutex<Vec<String>>>| {
		let started = started.clone();
		Box::new(move |id: &str| started.lock().unwrap().push(id.to_owned())) as Box<dyn Fn(&str) + Send + Sync>
	};
	let mut options = ensure(&launched);
	options.env = env_of(&env1);
	options.started = Some(starts(&started));
	let pane = ensure_boss_pane(&herdr, options).await.unwrap();
	let last = herdr.of("ensureWorkspace").pop().unwrap();
	assert_eq!(
		recorded(&[last]),
		vec![vec![json!("Jocasta"), json!(home_string()), json!(command), expected_env(&env1)]]
	);
	assert_eq!(*started.lock().unwrap(), vec![pane.pane_id.clone()]);
	herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);

	// A later run: the pane's omp lacks the new key; it isn't stale while working, and starts again once idle.
	let env2 = [("JOCASTA_BOSS_KEY", "run-2")];
	let again = |started: &Arc<Mutex<Vec<String>>>| {
		let mut options = ensure(&launched);
		options.env = env_of(&env2);
		options.stale = Some(Box::new(|p| p.agent_state == AgentState::Idle));
		options.started = Some(starts(started));
		options
	};
	herdr.set_state(&pane.pane_id, AgentState::Working, None, true);
	let kept = ensure_boss_pane(&herdr, again(&started)).await.unwrap();
	assert_eq!(kept.pane_id, pane.pane_id);
	assert!(herdr.of("respawn").is_empty());
	herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
	let again_pane = ensure_boss_pane(&herdr, again(&started)).await.unwrap();
	assert_eq!(
		recorded(&herdr.of("respawn")),
		vec![vec![json!(pane.pane_id), json!({"cwd": home_string(), "command": command, "env": expected_env(&env2)})]]
	);
	// A new start, a new key (0578).
	assert_ne!(launched.key_of(&again_pane.pane_id), Some(pane_key(&herdr, &pane.pane_id)));
	assert_eq!(*started.lock().unwrap(), vec![pane.pane_id.clone(), again_pane.pane_id.clone()]);
}

fn shipped_yaml(name: &str) -> Value {
	parse_value(&fs::read_to_string(shipped_config_dir(None).join(name)).unwrap()).unwrap()
}

#[test]
fn the_boss_settings_carry_the_privacy_settings_whatever_his_global_config_says() {
	let settings = shipped_yaml("boss.yml");
	assert_eq!(settings["dev"], json!({"autoqa": false}));
	assert_eq!(settings["disabledProviders"], json!(["claude-plugins"]));
	// The boss runs on Fable 5.2 at high, falling back to Opus 5.5 then Opus 5 at high (0590).
	assert_eq!(
		settings["modelRoles"],
		json!({
			"default": "anthropic/claude-fable-5-2:high",
			"web": "web/duckduckgo",
			"dictation": "local/parakeet-tdt-0.6b-v3",
		})
	);
	// Dictation through omp's own speech-to-text, sent on release, never leaving this machine (0556).
	assert_eq!(settings["stt"], json!({"enabled": true, "submitTrigger": "release"}));
	assert_eq!(
		settings["retry"],
		json!({"fallbackChains": {"default": ["anthropic/claude-opus-5-5:high", "anthropic/claude-opus-5:high"], "dictation": []}})
	);
}

#[test]
fn a_heavy_threads_heavy_yml_makes_its_default_role_his_slow_role_and_sets_nothing_else() {
	assert_eq!(shipped_yaml("heavy.yml"), json!({"modelRoles": {"default": "@slow"}}));
}

#[test]
fn the_boss_session_has_no_collab_relay_and_its_share_stays_on_a_dead_loopback_port() {
	let settings = shipped_yaml("boss.yml");
	assert!(settings.get("collab").is_none(), "{settings}");
	assert_eq!(settings["share"], json!({"serverUrl": "http://127.0.0.1:47499/s"}));
}

#[tokio::test]
async fn bare_jocasta_and_the_service_making_workspace_jocasta_at_once_leave_one_the_oldest_the_other_closed() {
	let j = temp_jocasta();
	let herdr = FakeSurface::new();
	let cli = LaunchedPanes::new(Some(&j.root().join("cli.json")));
	let service = LaunchedPanes::new(None);
	let (a, b) = tokio::join!(ensure_boss_pane(&herdr, ensure(&cli)), ensure_boss_pane(&herdr, ensure(&service)));
	let (a, b) = (a.unwrap(), b.unwrap());
	assert_eq!(herdr.of("ensureWorkspace").len(), 2);
	assert_eq!(labels(&herdr), vec!["Jocasta"]);
	assert_eq!(herdr.of("closeWorkspace").len(), 1);
	assert_eq!(a.pane_id, b.pane_id);
	assert_eq!(herdr.state().pane_list.iter().filter(|p| p.workspace_id == a.workspace_id).count(), 1);
}

#[tokio::test]
async fn a_boss_pane_herdr_restored_as_a_shell_runs_omp_again_one_whose_omp_ended_is_the_services_to_start_and_one_jocasta_didnt_start_is_left_alone()
 {
	let j = temp_jocasta();
	let herdr = FakeSurface::new();
	let command = command();
	let launched = LaunchedPanes::new(None);
	let pane = ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	launched.seen(&herdr.panes().await.unwrap()).unwrap();

	// omp ended in it: the same terminal, no agent. ensure_boss_pane alone leaves it; the service decides (0521).
	herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
	herdr.set_state(&pane.pane_id, AgentState::Unknown, None, true);
	ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	assert!(herdr.of("respawn").is_empty());

	// herdr restarted: a new terminal in the same folder.
	herdr.restore();
	let respawned = ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	assert_eq!(
		recorded(&herdr.of("respawn")),
		vec![vec![json!(pane.pane_id), json!({"cwd": home_string(), "command": command, "env": expected_env(&[])})]]
	);
	assert!(launched.has(&respawned.pane_id));

	// A record from another run (a fresh LaunchedPanes reads <data>/panes.json) knows it too; a pane it lacks isn't touched.
	launched.seen(&herdr.panes().await.unwrap()).unwrap();
	herdr.restore();
	let other = LaunchedPanes::new(Some(&j.root().join("other-panes.json")));
	ensure_boss_pane(&herdr, ensure(&other)).await.unwrap();
	assert_eq!(herdr.of("respawn").len(), 1);
	let fresh = LaunchedPanes::new(None);
	ensure_boss_pane(&herdr, ensure(&fresh)).await.unwrap();
	assert_eq!(herdr.of("respawn").len(), 2);
}

#[test]
fn a_panes_omp_ended_in_its_own_terminal_only_once_omp_was_seen_running_there() {
	let j = temp_jocasta();
	let file = j.root().join("ended-panes.json");
	let launched = LaunchedPanes::new(Some(&file));
	let cwd = j.root().join("work");
	fs::create_dir_all(&cwd).unwrap();
	let cwd_text = cwd.display().to_string();
	let pane = |terminal: &str, folder: &Path| PaneInfo {
		pane_id: "p1".to_owned(),
		tab_id: "t1".to_owned(),
		workspace_id: "w1".to_owned(),
		cwd: Some(folder.to_path_buf()),
		terminal_id: Some(terminal.to_owned()),
		agent_state: AgentState::Unknown,
		..PaneInfo::default()
	};
	let running = PaneInfo { agent: Some("omp".to_owned()), agent_state: AgentState::Idle, ..pane("term1", &cwd) };
	launched.add("p1", &cwd_text, None).unwrap();
	// omp is still starting: no agent seen in this terminal yet.
	launched.seen(&[pane("term1", &cwd)]).unwrap();
	assert!(!launched.ended(&pane("term1", &cwd)));
	// omp ran, then ended in the same terminal.
	launched.seen(std::slice::from_ref(&running)).unwrap();
	assert!(!launched.ended(&running));
	assert!(launched.ended(&pane("term1", &cwd)));
	assert!(!launched.restored(&pane("term1", &cwd)));
	// Another folder, or a new terminal (herdr's restore), isn't its omp ending there.
	assert!(!launched.ended(&pane("term1", j.root())));
	assert!(!launched.ended(&pane("term2", &cwd)));
	// Jocasta started omp there again: one ending, one start.
	launched.add("p1", &cwd_text, None).unwrap();
	launched.seen(&[pane("term1", &cwd)]).unwrap();
	assert!(!launched.ended(&pane("term1", &cwd)));
	// A later run reads the record: it never saw that omp run.
	launched.seen(&[running]).unwrap();
	assert!(!LaunchedPanes::new(Some(&file)).ended(&pane("term1", &cwd)));
}

#[tokio::test]
async fn at_the_services_start_a_boss_pane_with_no_omp_runs_omp_even_without_a_record_one_running_omp_is_left_alone() {
	let j = temp_jocasta();
	let herdr = FakeSurface::new();
	let command = command();
	// herdr restored workspace "Jocasta" from its own saved session: a shell Jocasta has no record of.
	let earlier = LaunchedPanes::new(Some(&j.root().join("earlier.json")));
	let pane = ensure_boss_pane(&herdr, ensure(&earlier)).await.unwrap();
	herdr.restore();
	let launched = LaunchedPanes::new(Some(&j.root().join("this-run.json")));
	ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	assert!(herdr.of("respawn").is_empty());
	let mut options = ensure(&launched);
	options.adopt = true;
	let adopted = ensure_boss_pane(&herdr, options).await.unwrap();
	assert_eq!(
		recorded(&herdr.of("respawn")),
		vec![vec![json!(pane.pane_id), json!({"cwd": home_string(), "command": command, "env": expected_env(&[])})]]
	);
	assert!(launched.has(&adopted.pane_id));

	// omp runs in it: a later start leaves it alone.
	herdr.set_state(&adopted.pane_id, AgentState::Idle, None, true);
	let next = LaunchedPanes::new(Some(&j.root().join("next.json")));
	let mut options = ensure(&next);
	options.adopt = true;
	ensure_boss_pane(&herdr, options).await.unwrap();
	assert_eq!(herdr.of("respawn").len(), 1);
}

#[tokio::test]
async fn his_own_shell_tab_in_workspace_jocasta_is_never_taken_for_the_boss_jocastas_or_omps_pane_is_else_a_new_tab() {
	let j = temp_jocasta();
	let herdr = FakeSurface::new();
	let launched = LaunchedPanes::new(None);
	let boss = ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap();
	// He opened a shell tab there, before the boss's in herdr's list.
	let tab = |title: &str| TabSpec {
		title: title.to_owned(),
		cwd: j.root().to_path_buf(),
		command: vec!["pwsh".to_owned()],
		env: None,
	};
	let shell = herdr.create_tab(&boss.workspace_id, &tab("shell")).await.unwrap();
	{
		let mut state = herdr.state();
		let at = state.pane_list.iter().position(|p| p.pane_id == shell.pane_id).unwrap();
		let moved = state.pane_list.remove(at);
		state.pane_list.insert(0, moved);
	}
	assert_eq!(ensure_boss_pane(&herdr, ensure(&launched)).await.unwrap().pane_id, boss.pane_id);

	// A record from another run lacks the boss pane, but omp runs in it.
	herdr.set_state(&boss.pane_id, AgentState::Idle, None, true);
	let fresh = LaunchedPanes::new(Some(&j.root().join("fresh.json")));
	assert_eq!(ensure_boss_pane(&herdr, ensure(&fresh)).await.unwrap().pane_id, boss.pane_id);

	// The boss pane is gone; only his shell is left among others: a new tab, his shell untouched.
	herdr.create_tab(&boss.workspace_id, &tab("logs")).await.unwrap();
	{
		let mut state = herdr.state();
		let at = state.pane_list.iter().position(|p| p.pane_id == boss.pane_id).unwrap();
		state.pane_list.remove(at);
	}
	let tabs = herdr.of("createTab").len();
	let none = LaunchedPanes::new(Some(&j.root().join("none.json")));
	let made = ensure_boss_pane(&herdr, ensure(&none)).await.unwrap();
	assert_eq!(herdr.of("createTab").len(), tabs + 1);
	assert_ne!(made.pane_id, shell.pane_id);
	assert!(herdr.of("respawn").is_empty());
}

#[test]
fn panes_json_keeps_only_well_formed_entries_a_broken_one_never_throws_on_a_refresh() {
	let j = temp_jocasta();
	let file = j.root().join("broken-panes.json");
	let root = j.root().display().to_string();
	fs::write(
		&file,
		json!({"w1:p1": null, "p2": {"cwd": 3}, "p3": {"cwd": root, "terminal": 4}, "p4": {"cwd": root}}).to_string(),
	)
	.unwrap();
	let launched = LaunchedPanes::new(Some(&file));
	let has: Vec<bool> = ["w1:p1", "p2", "p3", "p4"].iter().map(|id| launched.has(id)).collect();
	assert_eq!(has, vec![false, false, false, true]);
	let pane = PaneInfo {
		pane_id: "p4".to_owned(),
		tab_id: "t".to_owned(),
		workspace_id: "w".to_owned(),
		agent_state: AgentState::Unknown,
		terminal_id: Some("x".to_owned()),
		..PaneInfo::default()
	};
	launched.seen(&[pane]).unwrap();
}

fn workspace(id: &str, label: &str) -> WorkspaceInfo {
	WorkspaceInfo { id: id.to_owned(), label: label.to_owned(), cwd: None }
}

#[test]
fn workspace_ids_sort_in_the_order_herdr_made_them_and_a_fresh_session_has_no_jocasta() {
	let mut ids = vec!["w_10", "w_9", "w_2", "w_1"];
	ids.sort_by(|a, b| by_creation(a, b));
	assert_eq!(ids, vec!["w_1", "w_2", "w_9", "w_10"]);
	assert!(fresh_session(&[]));
	assert!(fresh_session(&[workspace("w_1", "default")]));
	assert!(!fresh_session(&[workspace("w_1", "default"), workspace("w_2", "other")]));
	assert!(!fresh_session(&[workspace("w_1", BOSS_WORKSPACE)]));
}
