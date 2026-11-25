//! Ported from the `H8` block of `test/bench/scripts.test.ts`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::DateTime;
use regex::Regex;
use serde_json::json;

use super::{
	GitRun, LayerCounters, ScoreInputs, assignments, finished, fixture_files, merged_tests_fail, read_layer_counters,
	score, thread_session_files,
};
use crate::lib::threads::{ThreadLine, parse_thread_line};
use jocasta::frontier::server::surface::PaneInfo;
use jocasta::shared::errors::Result;
use jocasta::shared::process::{RunOptions, run_program_sync};
use jocasta::shared::testing::temp_dir;

fn worktree(n: usize) -> PathBuf {
	std::env::temp_dir().join("repo.wt").join(format!("h8-r0-n4-t{n}"))
}

fn header(id: &str, at: &str) -> String {
	format!("{}\n", json!({ "type": "session", "id": id, "cwd": worktree(0), "timestamp": at }))
}

#[test]
fn k11_a_threads_sessions_since_the_run_started_its_planning_session_included_layer_on_counters_are_the_services_hit_log()
 {
	let dir = temp_dir("h8");
	let dir = dir.path();
	let before = dir.join("2026-11-09T09-00_old.jsonl");
	let planning = dir.join("2026-11-09T10-00_plan.jsonl");
	let execution = dir.join("2026-11-09T10-05_exec.jsonl");
	fs::write(&before, header("old", "2026-11-09T09:00:00Z")).unwrap();
	fs::write(&planning, header("plan", "2026-11-09T10:00:00Z")).unwrap();
	// "Approve and execute" starts a new session with no parentSession (omp's /clear).
	fs::write(&execution, header("exec", "2026-11-09T10:05:00Z")).unwrap();
	// the panes list's form of the folder: a trailing separator, and on Windows (0521) other case and slashes.
	let listed_cwd = if cfg!(windows) {
		format!("{}/", worktree(0).to_string_lossy().to_uppercase().replace('\\', "/"))
	} else {
		format!("{}/", worktree(0).to_string_lossy())
	};
	let panes = [
		PaneInfo { cwd: Some(PathBuf::from(listed_cwd)), session_path: Some(execution.clone()), ..PaneInfo::default() },
		PaneInfo {
			cwd: Some(std::env::temp_dir().join("elsewhere")),
			session_path: Some(dir.join("other.jsonl")),
			..PaneInfo::default()
		},
	];
	let worktrees = [
		("h8-r0-n4-t0".to_owned(), worktree(0).to_string_lossy().into_owned()),
		("h8-r0-n4-t1".to_owned(), worktree(1).to_string_lossy().into_owned()),
	];
	let since = DateTime::parse_from_rfc3339("2026-11-09T09:30:00Z").unwrap().timestamp_millis();
	let files = thread_session_files(&panes, &worktrees, since, None);
	assert_eq!(files, vec![("h8-r0-n4-t0".to_owned(), vec![planning, execution])]);
	assert_eq!(read_layer_counters(dir), LayerCounters { check_hits: 0 });
	fs::create_dir_all(dir.join("checks")).unwrap();
	fs::write(dir.join("checks").join("hits.log"), "1\n2\n3\n").unwrap();
	assert_eq!(read_layer_counters(dir), LayerCounters { check_hits: 3 });
}

#[test]
fn pairs_share_a_file_or_an_interface_status_lines_parse_the_score_counts() {
	let work = assignments(4, 0);
	let shape: Vec<(usize, Vec<String>)> = work.iter().map(|a| (a.pair, a.files.clone())).collect();
	assert_eq!(
		shape,
		vec![
			(0, vec!["src/mod0.ts".to_owned()]),
			(0, vec!["src/mod0.ts".to_owned()]),
			(1, vec!["src/shared.ts".to_owned()]),
			(1, vec!["src/use1.ts".to_owned()]),
		]
	);
	let mut names: Vec<String> = fixture_files(4).into_iter().map(|(path, _)| path).collect();
	names.sort();
	assert_eq!(
		names,
		[
			"package.json",
			"src/mod0.ts",
			"src/mod1.ts",
			"src/shared.ts",
			"src/use0.ts",
			"src/use1.ts",
			"test/pair1.test.ts",
		]
	);
	let line = parse_thread_line(
		"acme/h8 r0 n4 t0 [h8-r0-n4-t0] idle — finished its turn · overlaps h8 r0 n4 t1: src/mod0.ts; other: a.ts, b.ts · reported done",
	);
	assert_eq!(
		line,
		Some(ThreadLine {
			title: "h8 r0 n4 t0".to_owned(),
			branch: "h8-r0-n4-t0".to_owned(),
			state: "idle".to_owned(),
			overlaps: vec!["h8 r0 n4 t1".to_owned(), "other".to_owned()],
			reported: Some("done".to_owned()),
		})
	);
	assert_eq!(parse_thread_line("No threads."), None);
	assert_eq!(parse_thread_line("  spend $1.84 · peak context 41% (82k of 200k)"), None);
	let lines: Vec<ThreadLine> = work
		.iter()
		.map(|a| ThreadLine {
			title: a.title.clone(),
			branch: a.title.replace(' ', "-"),
			state: "idle".to_owned(),
			overlaps: if a.thread == 0 { vec![work[1].title.clone()] } else { Vec::new() },
			reported: None,
		})
		.collect();
	let changed: HashMap<String, Vec<String>> = work
		.iter()
		.map(|a| {
			let mut files = a.files.clone();
			if a.thread == 3 {
				files.push("src/shared.ts".to_owned());
			}
			(a.title.clone(), files)
		})
		.collect();
	let conflicts = HashMap::from([(0, true), (1, false)]);
	let semantic = HashMap::from([(1, true)]);
	let result = score(&ScoreInputs {
		work: &work,
		lines: &lines,
		changed: &changed,
		conflicts: &conflicts,
		main_changes: 0,
		semantic_failures: &semantic,
	});
	assert_eq!(result.threads, 4);
	assert_eq!(result.wrong_thread_edits, 1);
	assert_eq!(result.flagged_before_merge, 1);
	assert_eq!(result.conflicts_at_merge, 1);
	assert_eq!(result.semantic_failures, 1);
	assert_eq!(result.found_only_at_merge, 1);
}

fn git(cwd: &Path, args: &[&str]) -> Result<GitRun> {
	let mut argv = vec!["git", "-c", "user.name=Bench", "-c", "user.email=bench@example.org"];
	argv.extend_from_slice(args);
	let options = RunOptions { cwd: Some(cwd.to_path_buf()), ..RunOptions::default() };
	let result = run_program_sync(&argv, options)?;
	Ok(GitRun { status: result.code, stdout: result.stdout })
}

#[test]
fn an_interface_pair_passes_alone_and_fails_together_the_merged_trees_bun_test_catches_it_0576() {
	let repo = temp_dir("h8");
	let repo = repo.path();
	for (path, text) in fixture_files(4) {
		let file = repo.join(&path);
		fs::create_dir_all(file.parent().unwrap()).unwrap();
		fs::write(file, text).unwrap();
	}
	for args in [["init", "-q", "-b", "main"].as_slice(), &["add", "-A"], &["commit", "-q", "-m", "fixture"]] {
		git(repo, args).unwrap();
	}
	let shared = fs::read_to_string(repo.join("src").join("shared.ts")).unwrap();
	let used = fs::read_to_string(repo.join("src").join("use1.ts")).unwrap();
	let side = |branch: &str, file: &str, text: String| {
		git(repo, &["checkout", "-q", "-b", branch, "main"]).unwrap();
		fs::write(repo.join(file), text).unwrap();
		git(repo, &["commit", "-q", "-am", branch]).unwrap();
	};
	// The rename side changes Item1 and makeItem1 in shared.ts; the user side upper-cases the label in use1.ts.
	let renamed = Regex::new(r"(?s)(Item1 \{[^}]*?)label").unwrap().replace(&shared, "${1}title").into_owned();
	side("rename", "src/shared.ts", renamed.replacen("label: \"item 1\"", "title: \"item 1\"", 1));
	side("upper", "src/use1.ts", used.replacen("return item.label;", "return item.label.toUpperCase();", 1));
	git(repo, &["checkout", "-q", "main"]).unwrap();
	let tests_pass = |branch: &str| {
		git(repo, &["checkout", "-q", branch]).unwrap();
		let options = RunOptions { cwd: Some(repo.to_path_buf()), ..RunOptions::default() };
		run_program_sync(&["bun", "test"], options).unwrap().code == Some(0)
	};
	assert_eq!([tests_pass("rename"), tests_pass("upper")], [true, true]);
	git(repo, &["checkout", "-q", "main"]).unwrap();
	assert_eq!(git(repo, &["merge-tree", "--write-tree", "rename", "upper"]).unwrap().status, Some(0));
	assert!(merged_tests_fail(&git, repo, "rename", "upper", "bun").unwrap());
	// An unrelated clean pair keeps the tests passing.
	assert!(!merged_tests_fail(&git, repo, "rename", "main", "bun").unwrap());
}

#[test]
fn a_thread_is_finished_only_idle_or_done_with_a_commit_on_its_branch_0529() {
	let at = |state: &str| ThreadLine {
		title: "h8 r0 n8 t0".to_owned(),
		branch: "h8-r0-n8-t0".to_owned(),
		state: state.to_owned(),
		overlaps: Vec::new(),
		reported: None,
	};
	// A fresh thread reads idle before its brief lands, and a waiting plan reads done: neither has a commit.
	assert!(!finished(&at("idle"), 0));
	assert!(!finished(&at("done"), 0));
	assert!(finished(&at("idle"), 1));
	assert!(finished(&at("done"), 2));
	// At work, or waiting on him, isn't finished even with a commit.
	for state in ["working", "plan ready", "awaiting you", "blocked"] {
		assert!(!finished(&at(state), 1));
	}
}
