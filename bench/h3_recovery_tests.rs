//! Ported from the `H3` block of `test/bench/scripts.test.ts`.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::{
	FOLDER_SPACING_MS, FolderState, Picked, closed_itself, end_restart, folder_key, folders_intact, herdr_recovery,
	omp_back, pane_back, pick_pane, same_pane_ids, session_parses,
};
use crate::lib::proctree::ProcessInfo;
use jocasta::frontier::server::herdr::{AgentState, PaneInfo};
use jocasta::shared::errors::Error;

fn pane(pane_id: &str, agent: Option<&str>, status: AgentState, workspace_id: &str, cwd: Option<&str>) -> PaneInfo {
	PaneInfo {
		pane_id: pane_id.to_owned(),
		workspace_id: workspace_id.to_owned(),
		cwd: cwd.map(PathBuf::from),
		agent: agent.map(str::to_owned),
		agent_state: status,
		..PaneInfo::default()
	}
}

/// `pane(id, agent)` with the TypeScript helper's defaults: unknown state, workspace `w1`, no folder.
fn plain(pane_id: &str, agent: Option<&str>) -> PaneInfo {
	pane(pane_id, agent, AgentState::Unknown, "w1", None)
}

#[test]
fn same_panes_intact_folders_and_the_pane_to_kill() {
	assert!(same_pane_ids(&[plain("a", None), plain("b", None)], &[plain("b", None), plain("a", None)]));
	assert!(!same_pane_ids(&[plain("a", None)], &[plain("a", None), plain("c", None)]));
	let state = FolderState { status: " M a.ts\n".to_owned(), diff_hash: "x".to_owned() };
	let other = FolderState { diff_hash: "y".to_owned(), ..state.clone() };
	assert!(folders_intact(&[(PathBuf::from("/r"), state.clone())], &[(PathBuf::from("/r"), state.clone())]));
	assert!(!folders_intact(&[(PathBuf::from("/r"), state.clone())], &[(PathBuf::from("/r"), other)]));
	let picked = pick_pane(
		&[
			pane("a", Some("omp"), AgentState::Idle, "w1", None),
			pane("b", Some("omp"), AgentState::Working, "w1", None),
			plain("c", None),
		],
		&HashMap::new(),
		0.0,
	);
	assert_eq!(picked.map(|picked| picked.pane.pane_id), Some("b".to_owned()));
}

#[test]
fn the_pane_faults_pane_is_back_only_when_omp_runs_in_its_workspace_under_another_pane_id_0521() {
	let killed = pane("w2:p1", Some("omp"), AgentState::Working, "w2", None);
	// The dying omp, still listed under its old id, isn't back.
	assert!(!pane_back(&killed, &[pane("w2:p1", Some("omp"), AgentState::Working, "w2", None)]));
	assert!(!pane_back(
		&killed,
		&[
			pane("w2:p2", None, AgentState::Unknown, "w2", None),
			pane("w1:p1", Some("omp"), AgentState::Idle, "w1", None)
		]
	));
	assert!(pane_back(&killed, &[pane("w2:p2", Some("omp"), AgentState::Idle, "w2", None)]));
}

#[test]
fn a_run_counts_as_jocasta_closing_itself_only_with_no_service_and_no_panes_0530() {
	let listed = [plain("w1:p1", Some("omp"))];
	// Herdr gone with Jocasta, or answering with nothing: closed itself.
	assert!(closed_itself(false, false, None));
	assert!(closed_itself(false, false, Some(&[])));
	// The service fault's own kill leaves herdr's panes: not a close, so no second restart.
	assert!(!closed_itself(false, false, Some(&listed)));
	// A slow relaunch with the service up, or a recovered run: not a close.
	assert!(!closed_itself(false, true, Some(&[])));
	assert!(!closed_itself(true, false, None));
}

#[test]
fn the_pane_fault_kills_each_folders_omp_at_most_once_in_10_minutes_the_longest_rested_folder_first_0528() {
	let boss = pane("boss", Some("omp"), AgentState::Working, "w1", Some("/r/boss"));
	let t1 = pane("t1", Some("omp"), AgentState::Idle, "w2", Some("/r/t1"));
	let list = [boss.clone(), t1.clone(), pane("sh", None, AgentState::Unknown, "w3", Some("/r/sh"))];
	// A folder never killed goes before one at work that was.
	let ended = HashMap::from([(folder_key(&boss), 1_000.0)]);
	assert_eq!(pick_pane(&list, &ended, 2_000.0), Some(Picked { pane: t1.clone(), wait_ms: 0.0 }));
	// Both killed: the one whose last run ended first, waiting until it is FOLDER_SPACING_MS past that.
	let both = HashMap::from([(folder_key(&boss), 5_000.0), (folder_key(&t1), 3_000.0)]);
	assert_eq!(
		pick_pane(&list, &both, 10_000.0),
		Some(Picked { pane: t1.clone(), wait_ms: 3_000.0 + FOLDER_SPACING_MS - 10_000.0 })
	);
	assert_eq!(pick_pane(&list, &both, 3_000.0 + FOLDER_SPACING_MS + 1.0).map(|picked| picked.wait_ms), Some(0.0));
	assert_eq!(pick_pane(&[plain("sh", None)], &HashMap::new(), 0.0), None);
	// 0521 keys folders without case or slash differences on Windows, so the bench does too.
	let windows = cfg!(windows);
	let key = |id: &str, cwd: &str| folder_key(&pane(id, Some("omp"), AgentState::Unknown, "w1", Some(cwd)));
	assert_eq!(key("a", "/r/Demo") == key("b", "/r/demo"), windows);
	assert_eq!(key("a", "C:\\Users\\T\\demo") == key("b", "c:/users/t/demo"), windows);
}

#[test]
fn a_restarts_client_ends_root_first_and_never_the_new_services_tree_0528() {
	let proc = |pid: u32, ppid: u32, name: &str| ProcessInfo {
		pid,
		ppid,
		name: name.to_owned(),
		command_line: name.to_owned(),
		working_set: 0,
		private_bytes: None,
	};
	let processes = [
		proc(10, 1, "cmd.exe"),
		proc(11, 10, "jocasta.exe"),
		proc(12, 11, "herdr.exe"),
		proc(13, 11, "bun.exe"),
		proc(14, 13, "node.exe"),
		proc(20, 1, "launcher.exe"),
	];
	let mut killed: Vec<u32> = Vec::new();
	let kill = |pid: u32| {
		if pid == 12 {
			return Err(Error::other("gone"));
		}
		killed.push(pid);
		Ok(())
	};
	// jocasta.exe goes before its herdr client, so it can't see the client exit and ask to close; the kept tree stays.
	assert_eq!(end_restart(10, &HashSet::from([13, 14]), &processes, kill), vec![10, 11]);
	assert_eq!(killed, vec![10, 11]);
}

#[test]
fn recovered_when_every_workspace_that_had_omp_has_omp_again_under_any_pane_id_0522() {
	let before = [
		pane("a", Some("omp"), AgentState::Idle, "w1", None),
		pane("b", Some("omp"), AgentState::Working, "w2", None),
		pane("c", None, AgentState::Unknown, "w3", None),
	];
	// Relaunched panes with new ids count; the shell-only workspace needs nothing.
	assert!(omp_back(
		&before,
		&[pane("x", Some("omp"), AgentState::Idle, "w2", None), pane("y", Some("omp"), AgentState::Idle, "w1", None)]
	));
	// w2's pane is back but without omp.
	assert!(!omp_back(
		&before,
		&[pane("x", Some("omp"), AgentState::Idle, "w1", None), pane("b", None, AgentState::Unknown, "w2", None)]
	));
	// w2 lists no pane at all.
	assert!(!omp_back(
		&before,
		&[pane("a", Some("omp"), AgentState::Idle, "w1", None), pane("c", None, AgentState::Unknown, "w3", None)]
	));
}

#[test]
fn the_herdr_fault_notes_when_the_old_pane_ids_were_first_listed_beside_omp_being_back_0522() {
	let before =
		[pane("a", Some("omp"), AgentState::Working, "w1", None), pane("b", Some("omp"), AgentState::Idle, "w2", None)];
	let clock = Cell::new(1_000.0);
	let mut watch = herdr_recovery(&before, 1_000.0, || clock.get());
	clock.set(1_250.0);
	assert!(!watch.check(None));
	clock.set(3_300.0);
	// herdr is back with the old panes, not yet running omp.
	assert!(!watch.check(Some(&[
		pane("a", None, AgentState::Unknown, "w1", None),
		pane("b", None, AgentState::Unknown, "w2", None)
	])));
	clock.set(3_600.0);
	assert!(!watch.check(Some(&[
		pane("x", Some("omp"), AgentState::Idle, "w1", None),
		pane("b", None, AgentState::Unknown, "w2", None)
	])));
	clock.set(8_800.0);
	assert!(watch.check(Some(&[
		pane("x", Some("omp"), AgentState::Idle, "w1", None),
		pane("y", Some("omp"), AgentState::Idle, "w2", None)
	])));
	assert_eq!(watch.old_pane_ids_ms(), Some(2_300.0));

	let mut missed = herdr_recovery(&before, 0.0, || 500.0);
	assert!(missed.check(Some(&[
		pane("x", Some("omp"), AgentState::Idle, "w1", None),
		pane("y", Some("omp"), AgentState::Idle, "w2", None)
	])));
	assert_eq!(missed.old_pane_ids_ms(), None);
}

#[test]
fn a_session_file_parses_when_every_line_is_json_and_a_session_entry_is_anywhere_in_it_0523() {
	assert!(session_parses("{\"type\":\"session\"}\n{\"type\":\"message\"}\n"));
	// omp writes a title entry first (seen on 18.13.1).
	assert!(session_parses(
		"{\"type\":\"title\",\"title\":\"acme\"}\r\n{\"type\":\"session\"}\r\n{\"type\":\"message\"}\r\n"
	));
	assert!(!session_parses("{\"type\":\"title\"}\n{\"type\":\"session\"}\n{\"type\":\"mess"));
	assert!(!session_parses("{\"type\":\"title\"}\nnot json\n{\"type\":\"session\"}\n"));
	assert!(!session_parses("{\"type\":\"title\"}\n{\"type\":\"message\",\"text\":\"session\"}\n"));
	assert!(!session_parses(""));
}
