//! `surface.rs`'s tests: the text cleaners, and the in-memory fake the service's tests rely on.

use super::*;
use crate::shared::git::git;
use crate::shared::testing::temp_dir;
use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use serde_json::json;
#[test]
fn pane_text_strips_esc_and_other_control_characters_so_outside_text_cant_end_the_paste() {
	assert_eq!(
		pane_text("title\u{1b}[201~\r!echo pwned\u{7}\u{9b}2J\u{7f}\nnext\tline «ok» é"),
		"title[201~!echo pwned2J\nnext\tline «ok» é"
	);
	assert_eq!(pane_text("a\u{1b}b"), "ab");
	assert_eq!(pane_text("plain"), "plain");
}

// The in-memory `FakeHerdr` (`test/frontier/server/fake-herdr.ts`) has no test of its own in the TypeScript; these keep
// its behaviour, which the service's tests rely on, from drifting.

fn in_memory() -> Arc<fake_surface::FakeSurface> {
	Arc::new(fake_surface::FakeSurface::new())
}

#[tokio::test]
async fn the_in_memory_fake_lists_then_creates_so_two_calls_at_once_both_create() {
	let fake = in_memory();
	let cwd = PathBuf::from("C:\\src\\demo");
	let (first, second) =
		tokio::join!(fake.ensure_workspace("demo", &cwd, None, None), fake.ensure_workspace("demo", &cwd, None, None));
	assert_ne!(first.unwrap(), second.unwrap());
	assert_eq!(fake.state().workspace_list.len(), 2);
	let again = fake.ensure_workspace("demo", &cwd, None, None).await.unwrap();
	assert_eq!(fake.state().workspace_list.first().map(|w| w.id.clone()), Some(again));
	assert_eq!(fake.of("ensureWorkspace").len(), 3);
}

#[tokio::test]
async fn the_in_memory_fake_reports_state_changes_to_a_watch_restores_panes_as_shells_and_forgets_a_stopped_watch() {
	let fake = in_memory();
	let command = strings(&["omp"]);
	let id = fake
		.ensure_workspace("demo", Path::new("C:\\src\\demo"), Some(&command), Some(&env(&[("K", "V")])))
		.await
		.unwrap();
	let pane_id = fake.panes().await.unwrap()[0].pane_id.clone();
	assert_eq!(fake.env_of(&pane_id), Some(env(&[("K", "V")])));
	let seen = Arc::new(Mutex::new(Vec::<WatchEvent>::new()));
	let sink = seen.clone();
	let stop = fake.watch(Box::new(Vec::new), Box::new(move |event| locked(&sink).push(event)));
	fake.set_state(&pane_id, AgentState::Idle, Some("C:\\s\\a.jsonl"), true);
	fake.set_state(&pane_id, AgentState::Working, None, false);
	assert_eq!(*locked(&seen), [WatchEvent { pane_id: pane_id.clone(), workspace_id: id, state: AgentState::Idle }]);
	let pane = fake.panes().await.unwrap().remove(0);
	assert_eq!((pane.agent_state, pane.agent.as_deref()), (AgentState::Working, Some("omp")));
	assert_eq!(pane.session_path, Some(PathBuf::from("C:\\s\\a.jsonl")));
	fake.restore();
	let pane = fake.panes().await.unwrap().remove(0);
	assert_eq!(
		(pane.agent_state, pane.agent, pane.session_path, pane.label.as_deref()),
		(AgentState::Unknown, None, None, Some("shell"))
	);
	stop();
	fake.set_state(&pane_id, AgentState::Done, None, true);
	assert_eq!(locked(&seen).len(), 1);
}

#[tokio::test]
async fn the_in_memory_fake_makes_a_real_linked_worktree_and_a_workspace_and_pane_for_it() {
	let repo = temp_dir("herdr-fake-repo");
	let identity = ["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	let git_in = |args: &[&str]| {
		let mut all = identity.to_vec();
		all.extend(args);
		git(repo.path(), &all).unwrap().expect("git succeeded")
	};
	git_in(&["init", "-q", "-b", "main"]);
	git_in(&["commit", "--allow-empty", "-q", "-m", "init"]);
	git_in(&["branch", "spare"]);
	let fake = in_memory();
	fake.state().omp_up_on_create = Some(AgentState::Idle);
	let path = repo.path().join("wt").join("fix-login");
	let prepare: Prepare = Box::new(|path| {
		async move {
			assert!(path.exists(), "the checkout exists before prepare runs");
			Ok(env(&[("B", "2")]))
		}
		.boxed()
	});
	let created = fake
		.create_worktree(
			"w1",
			WorktreeOptions {
				repo: repo.path().to_path_buf(),
				branch: "fix-login".into(),
				base: Some("main".into()),
				path: path.clone(),
				title: "fix login".into(),
				command: strings(&["omp"]),
				env: Some(env(&[("A", "1")])),
				prepare: Some(prepare),
			},
		)
		.await
		.unwrap();
	assert_eq!(created.path, path);
	assert!(path.join(".git").exists());
	assert_eq!(fake.env_of(&created.pane_id), Some(env(&[("A", "1"), ("B", "2")])));
	assert_eq!(fake.state().workspace_list.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["fix login"]);
	assert_eq!(fake.panes().await.unwrap()[0].agent_state, AgentState::Idle);
	assert_eq!(fake.of("createWorktree")[0][1]["env"], json!({"A": "1", "B": "2"}));
	// An existing branch is checked out as it is.
	let other = repo.path().join("wt").join("again");
	let failing = WorktreeOptions {
		repo: repo.path().to_path_buf(),
		branch: "spare".into(),
		path: other,
		title: "again".into(),
		command: strings(&["omp"]),
		..WorktreeOptions::default()
	};
	fake.state().create_worktree_fails = Some(SurfaceError::new("boom", "layout failed"));
	assert_eq!(fake.create_worktree("w1", failing).await.unwrap_err().code, "boom");
	assert_eq!(fake.state().workspace_list.len(), 2);
}

#[tokio::test]
async fn the_in_memory_fake_respawns_in_the_same_tab_closes_workspaces_with_their_panes_and_stops_its_server() {
	let fake = in_memory();
	let cwd = Path::new("C:\\src\\demo");
	let first = fake.ensure_workspace("one", cwd, None, None).await.unwrap();
	let second = fake.ensure_workspace("two", cwd, None, None).await.unwrap();
	fake.move_workspace(&second, 0).await.unwrap();
	assert_eq!(fake.workspaces().await.unwrap().iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["two", "one"]);
	let old = fake.panes().await.unwrap().into_iter().find(|p| p.workspace_id == first).unwrap();
	let respawned = fake
		.respawn(&old.pane_id, &RespawnOptions { cwd: cwd.to_path_buf(), command: strings(&["omp"]), env: None })
		.await
		.unwrap();
	assert_eq!(respawned.tab_id, old.tab_id);
	assert_ne!(respawned.pane_id, old.pane_id);
	fake.state().roots.insert(respawned.pane_id.clone(), 7);
	assert_eq!(fake.pane_root(&respawned.pane_id, None).await.unwrap(), Some(7));
	fake.close_workspace(&first).await.unwrap();
	assert!(fake.panes().await.unwrap().iter().all(|p| p.workspace_id != first));
	fake.stop_server().await.unwrap();
	assert!(fake.snapshot().await.unwrap().workspaces.is_empty());
	assert_eq!(fake.methods().last().map(String::as_str), Some("stopServer"));
	assert_eq!(
		fake.notify(&Note { title: "t".into(), body: None, sound: Sound::None }).await.unwrap(),
		NotificationResult::Shown
	);
}

fn strings(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

fn env(pairs: &[(&str, &str)]) -> Env {
	pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
