use super::*;
use crate::shared::log::logs_dir;
use crate::shared::testing::temp_jocasta;

fn project(name: &str) -> Project {
	Project {
		id: format!("p-{name}"),
		identity: String::new(),
		name: name.to_owned(),
		main_path: PathBuf::from("/code").join(name),
		git: true,
		paths: Vec::new(),
	}
}

/// A thread in `phase`, as far as a phase move reads it.
fn thread(phase: Phase) -> Thread {
	Thread { phase, ..Thread::new("1-1", "w1", project("demo"), "Readme line", "feature/readme") }
}

/// Everything Jocasta's log holds so far in this test's data dir.
fn logged() -> String {
	let Ok(entries) = std::fs::read_dir(logs_dir()) else { return String::new() };
	entries.flatten().filter_map(|entry| std::fs::read_to_string(entry.path()).ok()).collect()
}

const PHASES: [Phase; 4] = [Phase::Task, Phase::Review, Phase::Handoff, Phase::Finish];

/// The moves the task cycle makes (SVC-15): the review, its handoff, Finish's PR turn, and back to the task.
const MOVES: [(Phase, Phase); 9] = [
	(Phase::Task, Phase::Review),
	(Phase::Task, Phase::Handoff),
	(Phase::Task, Phase::Finish),
	(Phase::Review, Phase::Handoff),
	(Phase::Review, Phase::Task),
	(Phase::Handoff, Phase::Task),
	(Phase::Handoff, Phase::Review),
	(Phase::Handoff, Phase::Finish),
	(Phase::Finish, Phase::Task),
];

#[test]
fn each_phase_move_of_the_task_cycle_passes_and_is_logged() {
	let _j = temp_jocasta();
	for (from, to) in MOVES {
		let mut t = thread(from);
		assert!(set_phase(&mut t, to, "a test"));
		assert_eq!(t.phase, to);
		let line = format!("INFO [phase] demo/Readme line: {} → {} (a test)", from.as_str(), to.as_str());
		assert!(logged().contains(&line), "{line}");
	}
}

#[test]
fn a_phase_move_outside_the_task_cycle_is_refused_and_logged_as_a_warning_staying_put_is_no_move() {
	let _j = temp_jocasta();
	let refused: Vec<(Phase, Phase)> = PHASES
		.into_iter()
		.flat_map(|from| PHASES.into_iter().map(move |to| (from, to)))
		.filter(|(from, to)| from != to && !MOVES.contains(&(*from, *to)))
		.collect();
	assert_eq!(
		refused,
		[(Phase::Review, Phase::Finish), (Phase::Finish, Phase::Review), (Phase::Finish, Phase::Handoff)]
	);
	for (from, to) in refused {
		let mut t = thread(from);
		assert!(!set_phase(&mut t, to, "a test"));
		assert_eq!(t.phase, from);
		let line = format!("WARN [phase] demo/Readme line: {} → {} refused (a test)", from.as_str(), to.as_str());
		assert!(logged().contains(&line), "{line}");
	}
	let before = logged();
	let mut t = thread(Phase::Handoff);
	assert!(set_phase(&mut t, Phase::Handoff, "a test"));
	assert_eq!(logged(), before);
}

fn named(pane: &str, title: &str, branch: &str, project_name: &str) -> ThreadRef {
	Shared::new(Thread::new(pane, "w", project(project_name), title, branch))
}

#[test]
fn the_registry_finds_by_pane_title_or_branch_and_refuses_an_unknown_thread() {
	let registry = ThreadRegistry::new();
	registry.add(named("1-1", "Readme line", "feature/readme", "demo"));
	registry.add(named("1-2", "Other", "feature/other", "demo"));
	assert_eq!(registry.find("1-2").unwrap().with(|t| t.title.clone()), "Other");
	assert_eq!(registry.find("  README LINE ").unwrap().with(|t| t.pane_id.clone()), "1-1");
	assert_eq!(registry.find("Feature/Other").unwrap().with(|t| t.pane_id.clone()), "1-2");
	let Err(Error::User { message, .. }) = registry.find("nope") else { panic!("an unknown thread is a refusal") };
	assert_eq!(message, "no thread nope.");
}

#[test]
fn the_registry_knows_which_thread_is_current_keeps_order_and_drops_by_pane() {
	let registry = ThreadRegistry::new();
	let (a, b) = (named("1-1", "A", "a", "demo"), named("1-2", "B", "b", "web"));
	registry.add(a.clone());
	registry.add(b.clone());
	assert!(registry.current(&a) && registry.includes(&b) && registry.has("1-2"));
	// A thread relaunched in the same pane replaces the old one in its place; the old one isn't current.
	let again = named("1-1", "A again", "a", "demo");
	registry.add(again.clone());
	assert!(!registry.current(&a) && registry.current(&again) && !registry.includes(&a));
	let titles: Vec<String> = registry.all().iter().map(|t| t.with(|t| t.title.clone())).collect();
	assert_eq!(titles, ["A again", "B"]);
	assert_eq!(registry.of_project("p-web").len(), 1);
	registry.drop_pane("1-1");
	assert_eq!(registry.pane_ids(), ["1-2"]);
	assert!(registry.get("1-1").is_none());
}

#[test]
fn a_threads_state_as_thread_status_says_it() {
	let mut t = thread(Phase::Task);
	t.state = AgentState::Done;
	assert_eq!(thread_state(&t), "idle");
	t.state = AgentState::Blocked;
	assert_eq!(thread_state(&t), "awaiting you");
	plan_of(&mut t).awaiting = Some("A plan".to_owned());
	assert_eq!(thread_state(&t), "plan ready");
	show(&mut t, ThreadState::Done);
	assert_eq!(thread_state(&t), "done");
	assert!(is_idle(AgentState::Done) && !is_idle(AgentState::Working));
	assert_eq!(settled(AgentState::Done), AgentState::Idle);
	assert_eq!(subject(&t), "demo/Readme line");
}
