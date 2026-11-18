//! Ported from the H2 cases of `test/bench/scripts.test.ts` and `test/bench/build-and-threads.test.ts`.

use std::path::{MAIN_SEPARATOR, PathBuf};

use chrono::DateTime;
use serde_json::{Value, json};

use super::*;
use crate::lib::sessions::ask_times;
use crate::lib::threads::started_branch;

fn t0() -> f64 {
	DateTime::parse_from_rfc3339("2026-10-24T10:00:00Z").unwrap().timestamp_millis() as f64
}

fn iso(at: f64) -> String {
	DateTime::from_timestamp_millis(at as i64).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn pane(pane_id: &str, cwd: PathBuf) -> PaneInfo {
	PaneInfo { pane_id: pane_id.to_owned(), cwd: Some(cwd), ..PaneInfo::default() }
}

fn note(at: f64, pane_id: &str, reason: &str, result: &str) -> NotificationEntry {
	let row =
		json!({ "at": at, "paneId": pane_id, "reason": reason, "title": "acme/a — awaiting you", "result": result });
	parse_notifications(&row.to_string()).remove(0)
}

fn rows(entries: &[NotificationEntry]) -> Vec<Value> {
	entries.iter().map(|entry| serde_json::to_value(entry).unwrap()).collect()
}

#[test]
fn pairs_each_ask_with_herdrs_change_and_the_panes_blocked_notification_that_herdr_answered_shown_t3_0552() {
	let t0 = t0();
	let session = [
		json!({ "type": "session", "timestamp": iso(t0 - 60_000.0) }).to_string(),
		json!({
			"type": "message",
			"timestamp": iso(t0),
			"message": { "role": "assistant", "content": [{ "type": "toolCall", "name": "ask", "arguments": {} }] },
		})
		.to_string(),
	]
	.join("\n");
	let sessions = vec![("p1".to_owned(), session)];
	let trials = correlate(
		&sessions,
		&[
			HerdrEvent { at: t0 + 100.0, pane_id: "p1".to_owned(), state: AgentState::Working },
			HerdrEvent { at: t0 + 250.0, pane_id: "p1".to_owned(), state: AgentState::Blocked },
		],
		&[
			note(t0 + 300.0, "p2", "blocked", "shown"),
			note(t0 + 500.0, "p1", "done", "shown"),
			note(t0 + 700.0, "p1", "blocked", "busy"),
			note(t0 + 900.0, "p1", "blocked", "shown"),
		],
	);
	assert_eq!(
		trials,
		vec![H2Trial {
			pane_id: "p1".to_owned(),
			kind: "ask",
			t0,
			herdr_ms: Some(250.0),
			notification_ms: Some(900.0),
			notification_result: Some("busy".to_owned()),
		}]
	);
	// No foreground client: herdr didn't show it, so there is no t3, and the row says why.
	assert_eq!(
		correlate(&sessions, &[], &[note(t0 + 400.0, "p1", "blocked", "no_foreground_client")]),
		vec![H2Trial {
			pane_id: "p1".to_owned(),
			kind: "ask",
			t0,
			herdr_ms: None,
			notification_ms: None,
			notification_result: Some("no_foreground_client".to_owned()),
		}]
	);
}

#[test]
fn an_asks_t0_is_when_omp_completed_the_message_not_when_it_started_the_request_omp_18_20_2s_shape() {
	let start = DateTime::parse_from_rfc3339("2026-11-09T10:00:00Z").unwrap().timestamp_millis() as f64;
	let ask = |message: Value, at: f64| {
		let mut base =
			json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": "ask", "arguments": {} }] });
		for (key, value) in message.as_object().unwrap() {
			base[key] = value.clone();
		}
		json!({ "type": "message", "timestamp": iso(at), "message": base }).to_string()
	};
	// omp stamps `timestamp` before the request and `completedAt` at message_end, when the entry is written.
	assert_eq!(
		ask_times(&ask(json!({ "timestamp": start, "completedAt": start + 4_000.0 }), start + 4_010.0)),
		vec![start + 4_000.0]
	);
	// Without completedAt, the entry's time (written at message_end), never the request's start.
	assert_eq!(ask_times(&ask(json!({ "timestamp": start }), start + 4_010.0)), vec![start + 4_010.0]);
}

#[test]
fn a_started_threads_pane_is_the_one_in_its_worktree_project_wt_slug_ben_23() {
	// herdr reports a pane's folder in the host's own form.
	let src = std::env::temp_dir().join("src");
	let with_separator = format!("{}{MAIN_SEPARATOR}", src.join("acme.wt").join("h2-trial-1").display());
	let panes = [
		pane("boss", std::env::temp_dir().join("data").join("home")),
		pane("other", src.join("acme.wt").join("h2-trial-10")),
		pane("mine", PathBuf::from(with_separator)),
	];
	assert_eq!(pane_of_branch(&panes, "feature/h2-trial-1"), Some("mine"));
	assert_eq!(pane_of_branch(&panes, "feature/h2-trial-2"), None);
	assert_eq!(
		started_branch("Started h2 trial 1 in acme on branch feature/h2-trial-1. It opens in plan mode…").as_deref(),
		Some("feature/h2-trial-1")
	);
	assert_eq!(
		started_branch("Started x in acme on branch feature/x, but omp didn't come up").as_deref(),
		Some("feature/x")
	);
	assert_eq!(started_branch("no project acme."), None);
}

#[test]
fn the_services_notification_log_parses_partial_and_malformed_lines_dont() {
	let lines = [
		r#"{"at":1761300000000,"paneId":"p1","reason":"blocked","title":"acme/a — awaiting you","result":"shown"}"#,
		r#"{"at":"soon","paneId":"p1","reason":"blocked","title":"x","result":"shown"}"#,
		"not JSON",
		r#"{"at":1761300000500,"paneId":null,"reason":"event","title":"noted","body":"b","result":"error"}"#,
		r#"{"at":1761300000900,"paneId":"p1","rea"#,
	]
	.join("\r\n");
	assert_eq!(
		rows(&parse_notifications(&lines)),
		vec![
			json!({ "at": 1_761_300_000_000_i64, "paneId": "p1", "reason": "blocked", "title": "acme/a — awaiting you", "result": "shown" }),
			json!({ "at": 1_761_300_000_500_i64, "paneId": null, "reason": "event", "title": "noted", "body": "b", "result": "error" }),
		]
	);
}

#[test]
fn a_started_thread_gets_its_worktrees_pane_only_once_the_list_shows_its_branch_and_keeps_it() {
	// herdr reports a pane's folder in the host's own form.
	let wt = std::env::temp_dir().join("src").join("acme.wt");
	let panes = vec![
		pane("boss", std::env::temp_dir().join("data").join("home")),
		pane("p1", wt.join("h2-trial-1")),
		pane("p2", PathBuf::from(format!("{}{MAIN_SEPARATOR}", wt.join("h2-trial-2").display()))),
	];
	let thread = |branch: &str, pane_id: Option<&str>| OurThread {
		branch: branch.to_owned(),
		started_at: 0.0,
		pane_id: pane_id.map(str::to_owned),
	};
	let line = |branch: &str| ThreadLine {
		title: String::new(),
		branch: branch.to_owned(),
		state: "idle".to_owned(),
		overlaps: Vec::new(),
		reported: None,
	};
	let mut ours = vec![thread("feature/h2-trial-1", None), thread("feature/h2-trial-2", None)];
	find_listed_panes(&mut ours, &[line("feature/h2-trial-1"), line("feature/other")], &panes);
	assert_eq!(ours, vec![thread("feature/h2-trial-1", Some("p1")), thread("feature/h2-trial-2", None)]);
	let mut moved = vec![pane("moved", wt.join("h2-trial-1"))];
	moved.extend(panes);
	find_listed_panes(&mut ours, &[line("feature/h2-trial-2")], &moved);
	assert_eq!(ours, vec![thread("feature/h2-trial-1", Some("p1")), thread("feature/h2-trial-2", Some("p2"))]);
}
