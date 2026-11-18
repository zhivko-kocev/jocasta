//! Ported from `test/evals/a5.test.ts`.

use chrono::DateTime;
use serde_json::{Map, Value, json};

use super::a5::{A5Input, ThreadInput, ThreadReport, a5_report};
use crate::lib::service_feed::FeedEntry;
use crate::lib::sessions::plan_approval_times;

fn start() -> f64 {
	DateTime::parse_from_rfc3339("2026-10-24T09:00:00Z").unwrap().timestamp_millis() as f64
}

fn min(m: f64) -> f64 {
	start() + m * 60_000.0
}

fn iso(at: f64) -> String {
	DateTime::from_timestamp_millis(at as i64).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn header(at: f64) -> String {
	json!({ "type": "session", "version": 3, "id": "s", "timestamp": iso(at) }).to_string()
}

fn user(at: f64, text: &str, synthetic: bool) -> String {
	let mut message = json!({ "role": "user", "content": [{ "type": "text", "text": text }] });
	if synthetic {
		message["synthetic"] = json!(true);
	}
	json!({ "type": "message", "timestamp": iso(at), "message": message }).to_string()
}

fn plan_ready(at: f64) -> String {
	json!({
		"type": "message",
		"timestamp": iso(at),
		"message": { "role": "toolResult", "toolName": "write", "details": { "xdev": { "tool": "propose", "mode": "execute" } } },
	})
	.to_string()
}

fn mode_change(at: f64, mode: &str, data: Option<Value>) -> String {
	let mut line = json!({ "type": "mode_change", "timestamp": iso(at), "mode": mode });
	if let Some(data) = data {
		line["data"] = data;
	}
	line.to_string()
}

fn entry(at: f64, message: Value) -> FeedEntry {
	let message: Map<String, Value> = message.as_object().cloned().unwrap();
	FeedEntry { at, message }
}

#[test]
fn interventions_attention_minutes_and_batch_time_from_the_feed_and_the_sessions() {
	let feed = vec![
		entry(
			min(5.0),
			json!({ "type": "toast", "reason": "blocked", "text": "acme/a — plan ready, awaiting you", "paneId": "p1" }),
		),
		entry(
			min(20.0),
			json!({ "type": "toast", "reason": "blocked", "text": "acme/b — awaiting you", "paneId": "p2" }),
		),
		// Not urgent: no attention counted.
		entry(min(30.0), json!({ "type": "toast", "reason": "event", "priority": "normal", "text": "noted" })),
		entry(min(50.0), json!({ "type": "toast", "reason": "done", "text": "acme/a — done" })),
		entry(
			min(90.0),
			json!({ "type": "toast", "reason": "blocked", "text": "acme/b — awaiting you", "paneId": "p2" }),
		),
		entry(min(1.0), json!({ "type": "hud" })),
	];
	let boss = [
		header(min(0.0)),
		user(min(0.0), "start a and b", false),
		user(min(6.0), "[event] acme/a — plan ready, awaiting you", false),
		user(min(22.0), "tell b to use the old name", false),
	]
	.join("\n");
	let thread_a = [
		header(min(1.0)),
		user(min(1.0), "the brief", false),
		plan_ready(min(4.0)),
		mode_change(min(8.0), "default", None),
		user(min(40.0), "Check this task's work. First run the tests", false),
		user(min(45.0), "/handoff Keep what the next task needs", false),
		user(min(46.0), "a synthetic nudge", true),
	]
	.join("\n");
	let thread_b =
		[header(min(2.0)), user(min(2.0), "the brief", false), user(min(25.0), "use the old name", false)].join("\n");
	let report = a5_report(&A5Input {
		feed: &feed,
		boss: &boss,
		threads: &[
			ThreadInput { name: "a".to_owned(), session: thread_a },
			ThreadInput { name: "b".to_owned(), session: thread_b },
		],
		cap_ms: 60.0 * 60_000.0,
	});
	assert_eq!(
		report.threads,
		vec![
			ThreadReport { name: "a".to_owned(), interventions: 1, messages: 0, approvals: 1 },
			ThreadReport { name: "b".to_owned(), interventions: 1, messages: 1, approvals: 0 },
		]
	);
	assert_eq!([report.boss_messages, report.interventions, report.urgent_toasts, report.answered], [2, 4, 3, 2]);
	// The plan toast at 5 is answered by the approval at 8; the question at 20 by the boss message at 22; the toast at
	// 90 has no answer.
	assert!((report.attention_minutes - 5.0).abs() < 1e-10);
	assert!((report.batch_minutes.unwrap() - 49.0).abs() < 1e-10);
}

#[test]
fn an_approval_that_keeps_the_context_counts_once_though_omp_18_20_2_writes_two_mode_changes_to_none_0589() {
	// "Approve and keep context": omp leaves plan mode (`none`), then records the approved plan with a second `none`
	// (`interactive-mode.ts:6932`), both in the planning session.
	let plan = json!({ "planFilePath": "local://PLAN.md" });
	let approved = json!({ "approvedPlanFilePath": "local://PLAN.md" });
	let session = [
		header(min(0.0)),
		mode_change(min(0.0), "plan", Some(plan.clone())),
		user(min(0.0), "the brief", false),
		plan_ready(min(4.0)),
		mode_change(min(8.0), "none", None),
		mode_change(min(8.0), "none", Some(approved.clone())),
		user(min(9.0), "Execute the approved plan.", true),
		// A second round of planning, approved again: a second approval.
		mode_change(min(20.0), "plan", Some(plan)),
		plan_ready(min(25.0)),
		mode_change(min(30.0), "none", None),
		mode_change(min(30.0), "none", Some(approved)),
	]
	.join("\n");
	assert_eq!(plan_approval_times(&session), vec![min(8.0), min(30.0)]);
	let report = a5_report(&A5Input {
		feed: &[],
		boss: &header(min(0.0)),
		threads: &[ThreadInput { name: "a".to_owned(), session }],
		cap_ms: 60.0 * 60_000.0,
	});
	assert_eq!(
		report.threads,
		vec![ThreadReport { name: "a".to_owned(), interventions: 2, messages: 0, approvals: 2 }]
	);
}
