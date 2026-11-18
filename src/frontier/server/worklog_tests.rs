//! Tests of `worklog.rs`: from `day.test.ts` (the worklog, `mergeWorklog`) and `sessions.test.ts` (SRV-B-4's 25 minutes).

use std::fs;
use std::path::Path;

use super::*;
use crate::shared::day::day_start;
use crate::shared::git::git;
use crate::shared::testing::{temp_dir, temp_jocasta};

/// Runs git in `dir` with a throwaway identity; fails the test on a non-zero exit.
fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn project(name: &str, main_path: &Path, git: bool) -> Project {
	Project {
		id: format!("p-{name}"),
		identity: format!("root:p-{name}"),
		name: name.to_owned(),
		main_path: main_path.to_path_buf(),
		git,
		paths: Vec::new(),
	}
}

fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
	pairs.iter().map(|(id, body)| ((*id).to_owned(), (*body).to_owned())).collect()
}

#[tokio::test]
async fn the_worklog_has_one_entry_per_project_from_the_day_s_omp_sessions_and_his_commits_with_time_and_links_his_edits_are_kept_0302()
 {
	let _j = temp_jocasta();
	let repo = temp_dir("worklog-demo");
	let dir = repo.path();
	sh(dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("README.md"), "demo\n").unwrap();
	sh(dir, &["add", "."]);
	sh(dir, &["commit", "-q", "-m", "init demo"]);
	sh(dir, &["config", "user.email", "test@example.com"]);
	sh(dir, &["remote", "add", "origin", "git@github.com:acme/demo.git"]);
	let demo = project("demo", dir, true);
	// The day of the project's commit in this process's time zone, the one the worklog's window uses (SH-20).
	let committed: i64 = sh(dir, &["log", "-1", "--format=%ct"]).parse().unwrap();
	let today = local_day(committed * 1000);
	let start = day_start(&today, 0) + 3_600_000;
	let at = |minutes: i64| start + minutes * 60_000;
	// 10 minutes of work, a 2-hour break, then 5 more: 15 minutes.
	let sessions = |_: &Project, _: &str| {
		vec![WorklogSession { label: "Readme line".to_owned(), times: vec![at(0), at(10), at(130), at(135)] }]
	};

	let written = write_worklog(std::slice::from_ref(&demo), &today, sessions, None, None).await.unwrap();
	assert_eq!(written, WorklogWritten { path: worklog_path(&today), projects: vec!["demo".to_owned()] });
	let text = fs::read_to_string(&written.path).unwrap();
	assert!(text.contains("## demo · 15m"), "{text}");
	assert!(text.contains("- Readme line · 15m"), "{text}");
	let link =
		regex::Regex::new(r"- \[[0-9a-f]{7}\]\(https://github\.com/acme/demo/commit/[0-9a-f]{40}\) init demo").unwrap();
	assert!(link.is_match(&text), "{text}");

	// He edits the entry and adds his own line: both stay when it is written again.
	let edited = text.replace("## demo · 15m", "## demo · 15m (pairing with A.)");
	fs::write(&written.path, format!("{edited}\nMy own note.\n")).unwrap();
	write_worklog(std::slice::from_ref(&demo), &today, sessions, None, None).await.unwrap();
	let again = fs::read_to_string(&written.path).unwrap();
	assert!(again.contains("(pairing with A.)"));
	assert!(again.contains("My own note."));
}

#[test]
fn merge_worklog_replaces_only_untouched_entries_and_appends_new_projects() {
	let first = merge_worklog(None, "# Worklog", &entries(&[("a", "## a · 1m"), ("b", "## b · 1m")]));
	let touched = first.replace("## b · 1m", "## b · 1m, mine");
	let next = merge_worklog(
		Some(&touched),
		"# Worklog",
		&entries(&[("a", "## a · 9m"), ("b", "## b · 9m"), ("c", "## c · 2m")]),
	);
	assert!(next.contains("## a · 9m"));
	assert!(!next.contains("## a · 1m"));
	assert!(next.contains("## b · 1m, mine"));
	assert!(next.contains("## c · 2m"));
	assert!(next.find("## c").unwrap() > next.find("## b").unwrap());
}

#[test]
fn merge_worklog_keeps_text_outside_the_blocks_and_a_block_without_a_close_marker() {
	let first = merge_worklog(None, "# Worklog", &entries(&[("a", "## a · 1m")]));
	assert!(first.starts_with("# Worklog\n\n<!-- jocasta a "));
	assert!(first.ends_with("## a · 1m\n<!-- /jocasta a -->\n"));
	let with_notes = format!("intro\n<!-- jocasta x deadbeef -->\nunclosed\n{first}outro\n");
	let next = merge_worklog(Some(&with_notes), "# Worklog", &entries(&[("a", "## a · 2m")]));
	assert!(next.starts_with("intro\n<!-- jocasta x deadbeef -->\nunclosed\n# Worklog"));
	assert!(next.contains("## a · 2m"));
	assert!(next.ends_with("outro\n"));
}

#[tokio::test]
async fn an_older_day_s_message_times_give_the_worklog_its_25_minutes_srv_b_4() {
	let _j = temp_jocasta();
	let old_day = local_day(day_start(&local_day(chrono::Utc::now().timestamp_millis()), -4));
	let old = day_start(&old_day, 0) + 9 * 3_600_000;
	// Six messages five minutes apart, as the session file of that test holds.
	let times: Vec<i64> = (0..6).map(|i| old + i * 5 * 60_000).collect();
	let demo = project("demo", Path::new("C:\\demo"), false);
	let sessions = |_: &Project, _: &str| vec![WorklogSession { label: "demo".to_owned(), times: times.clone() }];
	let written = write_worklog(std::slice::from_ref(&demo), &old_day, sessions, None, None).await.unwrap();
	assert!(fs::read_to_string(written.path).unwrap().contains("## demo · 25m"));
}

#[tokio::test]
async fn a_worklog_stopped_meanwhile_writes_nothing() {
	let _j = temp_jocasta();
	let demo = project("demo", Path::new("C:\\demo"), false);
	let day = "2026-11-03";
	let sessions =
		|_: &Project, _: &str| vec![WorklogSession { label: "x".to_owned(), times: vec![day_start(day, 0) + 1] }];
	let written = write_worklog(std::slice::from_ref(&demo), day, sessions, None, Some(&|| true)).await.unwrap();
	assert_eq!(written, WorklogWritten { path: worklog_path(day), projects: Vec::new() });
	assert!(!written.path.exists());
}

#[test]
fn durations_gaps_and_the_dashboard_line() {
	assert_eq!(duration(0), "0m");
	assert_eq!(duration(59 * 60_000), "59m");
	assert_eq!(duration(80 * 60_000), "1h 20m");
	assert_eq!(duration(65 * 60_000), "1h 05m");
	// A gap of exactly 15 minutes counts; one second more is a break.
	assert_eq!(active_ms(&[0, 15 * 60_000, 30 * 60_000 + 1_000]), 15 * 60_000);
	let day = ProjectDay {
		threads: vec![ThreadTime { label: "t".to_owned(), ms: 80 * 60_000 }],
		commits: vec![Commit { sha: "a".to_owned(), subject: "s".to_owned(), url: None }; 3],
		total_ms: 80 * 60_000,
	};
	assert_eq!(project_day_line("acme-api", &day), "acme-api · 1h 20m · 3 commits");
	let one = ProjectDay { commits: day.commits[..1].to_vec(), ..day };
	assert_eq!(project_day_line("acme-api", &one), "acme-api · 1h 20m · 1 commit");
}

#[test]
fn a_commit_s_page_comes_from_github_or_azure_devops_origins_only() {
	let sha = "abc";
	let gh = Some("https://github.com/acme/demo/commit/abc".to_owned());
	assert_eq!(commit_url(Some("git@github.com:acme/demo.git"), sha), gh);
	assert_eq!(commit_url(Some("https://github.com/acme/demo/"), sha), gh);
	let az = Some("https://dev.azure.com/org/proj/_git/repo/commit/abc".to_owned());
	assert_eq!(commit_url(Some("https://user@dev.azure.com/org/proj/_git/repo"), sha), az);
	assert_eq!(commit_url(Some("git@ssh.dev.azure.com:v3/org/proj/repo"), sha), az);
	assert_eq!(commit_url(Some("https://example.com/a/b.git"), sha), None);
	assert_eq!(commit_url(None, sha), None);
}
