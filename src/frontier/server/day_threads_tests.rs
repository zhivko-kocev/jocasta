//! Tests of `day.rs` ported from `test/frontier/server/day-threads.test.ts`: what his day shows about threads and quota
//! (0456) and the weekly review's schedule (0470), on a Day with stand-in hooks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::tests::{Review, Stand, at, project, settle, thread, write_settings};
use super::*;
use crate::frontier::protocol::{LandedItem, NeedsYou, NeedsYouKind, QuotaSpan, RailItem};
use crate::shared::testing::temp_jocasta;

const HOUR: i64 = 3_600_000;

fn toast_texts(s: &Stand) -> Vec<String> {
	s.toasts()
		.into_iter()
		.filter_map(|m| match m {
			ServerMessage::Toast { text, .. } => Some(text),
			_ => None,
		})
		.collect()
}

fn reviewed(kind: NeedsYouKind, text: &str, since: i64, pane: &str) -> NeedsYou {
	NeedsYou {
		kind,
		text: text.to_owned(),
		url: None,
		pane_id: Some(pane.to_owned()),
		project: Some("acme-api".to_owned()),
		time: None,
		since: Some(iso(since)),
		overlaps: None,
	}
}

fn landed(pane: Option<&str>, title: &str, project: &str, line: &str) -> LandedItem {
	LandedItem {
		pane_id: pane.map(str::to_owned),
		title: title.to_owned(),
		project: Some(project.to_owned()),
		line: line.to_owned(),
		overlaps: None,
		state: None,
		since: None,
		cost: None,
		context: None,
	}
}

#[tokio::test]
async fn awaiting_you_lists_reviewed_unfinished_threads_oldest_first_with_their_wait_past_a_day_too_landed_keeps_only_unreviewed_ones()
 {
	let _j = temp_jocasta();
	let now = at(2026, 10, 23, 14, 0);
	let s = Stand::new(now);
	*s.shared.threads.lock().unwrap() = vec![
		thread(|t| {
			t.pane_id = "a".to_owned();
			t.title = "Export v1".to_owned();
			t.state = ThreadState::Done;
			t.line = "done and reviewed: no notes".to_owned();
			t.reviewed_at = Some(now - 3 * HOUR);
			t.changed_at = now - 3 * HOUR;
		}),
		thread(|t| {
			t.pane_id = "b".to_owned();
			t.title = "Docs pass".to_owned();
			t.state = ThreadState::Done;
			t.line = "done and reviewed: 2 notes".to_owned();
			t.reviewed_at = Some(now - 50 * HOUR);
			t.changed_at = now - 50 * HOUR;
		}),
		thread(|t| {
			t.pane_id = "c".to_owned();
			t.title = "Retry".to_owned();
			t.state = ThreadState::Done;
			t.line = "in review".to_owned();
			t.changed_at = now - HOUR;
		}),
		thread(|t| {
			t.pane_id = "d".to_owned();
			t.title = "Export v2".to_owned();
			t.line = "writing tests".to_owned();
			t.overlaps = Some(vec!["Export v1".to_owned()]);
		}),
	];
	let today = s.day.today().unwrap();
	assert_eq!(
		today.needs_you,
		vec![
			reviewed(NeedsYouKind::Reviewed, "Docs pass — done and reviewed: 2 notes", now - 50 * HOUR, "b"),
			reviewed(NeedsYouKind::Reviewed, "Export v1 — done and reviewed: no notes", now - 3 * HOUR, "a"),
		]
	);
	assert_eq!(today.landed.iter().map(|t| t.pane_id.as_deref()).collect::<Vec<_>>(), [Some("c")]);
	// Overlaps travel with the thread wherever it appears; Underway carries its state and since when (0555).
	assert_eq!(
		today.underway,
		vec![RailItem {
			pane_id: "d".to_owned(),
			title: "Export v2".to_owned(),
			project: Some("acme-api".to_owned()),
			line: "writing tests".to_owned(),
			overlaps: Some(vec!["Export v1".to_owned()]),
			state: Some(ThreadState::Working),
			since: Some(0),
			cost: None,
			context: None,
		}]
	);
	let text = s.day.today_text().unwrap();
	assert!(
		text.contains(
			"Awaiting you acme-api/Docs pass — done and reviewed: 2 notes (waiting 2 d) · acme-api/Export v1 — done and reviewed: no notes (waiting 3 h)"
		),
		"{text}"
	);
	assert!(text.contains("Underway     acme-api/Export v2 (overlaps Export v1)"), "{text}");
}

#[tokio::test]
async fn a_thread_merged_or_sent_as_a_pr_stays_in_landed_since_yesterday_saying_how_it_landed_across_a_restart() {
	let _j = temp_jocasta();
	let now = at(2026, 11, 24, 17, 0);
	let projects = vec![project("p1", "demo"), project("p2", "client-a")];
	let s = Stand::in_projects(now - 50 * HOUR, projects.clone());
	s.day.landed("w0:p1", "Old work", "p1", "merged into main as 1111111").unwrap();
	s.set(now - 2 * HOUR);
	s.day.landed("w4:p1", "Add subtract", "p1", "merged into main as 6796653").unwrap();
	s.set(now - HOUR);
	s.day.landed("w6:p1", "Add farewell — v2", "p2", "PR open").unwrap();
	s.set(now);
	*s.shared.threads.lock().unwrap() = vec![thread(|t| {
		t.pane_id = "w5:p1".to_owned();
		t.title = "Add multiply".to_owned();
		t.project = Some("demo".to_owned());
		t.state = ThreadState::Done;
		t.line = "in review".to_owned();
		t.changed_at = now - HOUR;
	})];
	let all = vec![
		landed(Some("w5:p1"), "Add multiply", "demo", "in review"),
		landed(None, "Add subtract", "demo", "merged into main as 6796653"),
		landed(None, "Add farewell — v2", "client-a", "PR open"),
	];
	assert_eq!(s.day.today().unwrap().landed, all);
	assert!(
		s.day.today_text().unwrap().contains(
			"Landed       demo/Add multiply · demo/Add subtract (merged into main as 6796653) · client-a/Add farewell — v2 (PR open)"
		)
	);
	// The page heard of each as it landed.
	let pushed = s
		.sent()
		.into_iter()
		.filter_map(|m| if let ServerMessage::Today { today } = m { Some(today) } else { None })
		.next_back();
	assert_eq!(pushed.map(|today| today.landed), Some(all[1..].to_vec()));
	// A restart: the catalog keeps them.
	let again = Stand::in_projects(now, projects);
	assert_eq!(again.day.today().unwrap().landed, all[1..].to_vec());
}

#[tokio::test]
async fn thread_lines_held_in_quiet_hours_wait_in_the_queue_across_a_restart_and_go_to_the_boss_oldest_first_once_they_end()
 {
	let _j = temp_jocasta();
	write_settings(r#"{"service":{"quietHours":"22:00-08:00"}}"#);
	let night = Stand::new(at(2026, 10, 24, 23, 0));
	night
		.day
		.hold("client-a/Add farewell — he committed by hand on its branch (1 commit) · offer once to save a lesson")
		.unwrap();
	night.set(at(2026, 10, 24, 23, 30));
	night.day.hold("demo/Add divide — finished its turn").unwrap();
	night.day.tick().unwrap();
	assert!(night.alerts().is_empty());
	// Restarted before morning: the queue kept them.
	let morning = Stand::new(at(2026, 10, 25, 7, 59));
	morning.day.tick().unwrap();
	assert!(morning.alerts().is_empty());
	morning.set(at(2026, 10, 25, 8, 0));
	morning.day.tick().unwrap();
	morning.day.tick().unwrap();
	assert_eq!(
		morning.alerts(),
		[
			"client-a/Add farewell — he committed by hand on its branch (1 commit) · offer once to save a lesson",
			"demo/Add divide — finished its turn",
		]
	);
}

/// A stand whose `omp usage --json` answers what the returned cell holds.
fn quota_stand(time: i64) -> (Stand, Arc<Mutex<String>>) {
	let usage = Arc::new(Mutex::new(serde_json::json!({ "reports": [] }).to_string()));
	let seen = Arc::clone(&usage);
	let s = Stand::with(
		time,
		vec![project("p1", "acme-api")],
		DayOptions {
			pollers: Some(false),
			usage: Some(Arc::new(move || {
				let text = seen.lock().unwrap().clone();
				Box::pin(async move { Ok(text) })
			})),
			..DayOptions::default()
		},
	);
	(s, usage)
}

fn window(used: f64) -> String {
	serde_json::json!({ "reports": [{ "limits": [{ "window": { "id": "5h" }, "amount": { "usedFraction": used } }] }] })
		.to_string()
}

#[tokio::test]
async fn the_quota_one_toast_when_a_window_passes_80_percent_again_only_after_it_went_back_under_a_failing_omp_usage_is_quiet()
 {
	let _j = temp_jocasta();
	let (s, usage) = quota_stand(at(2026, 10, 23, 14, 0));
	let five_hours = |used: f64| vec![QuotaWindow { window: QuotaSpan::FiveHours, used, resets_at: None }];
	*usage.lock().unwrap() = window(0.5);
	s.day.read_quota().await;
	assert_eq!(s.day.quota(), five_hours(0.5));
	assert!(s.toasts().is_empty());
	*usage.lock().unwrap() = window(0.84);
	s.day.read_quota().await;
	s.day.read_quota().await;
	assert_eq!(
		s.toasts(),
		vec![ServerMessage::Toast {
			reason: ToastReason::Event,
			level: ToastLevel::Warning,
			text: "Subscription quota: 5h at 84%".to_owned(),
			priority: Some(ToastPriority::Urgent),
			pane_id: None,
			url: None,
		}]
	);
	*usage.lock().unwrap() = window(0.1);
	s.day.read_quota().await;
	*usage.lock().unwrap() = window(0.9);
	s.day.read_quota().await;
	assert_eq!(toast_texts(&s), ["Subscription quota: 5h at 84%", "Subscription quota: 5h at 90%"]);
	// omp missing or logged out: the last windows stay, nothing is toasted.
	*usage.lock().unwrap() = "not json".to_owned();
	s.day.read_quota().await;
	assert_eq!(s.day.quota(), five_hours(0.9));
	assert_eq!(s.toasts().len(), 2);
	assert!(s.notes().is_empty());
}

#[tokio::test]
async fn a_quota_that_reads_80_percent_is_past_the_threshold_0_796_gets_its_toast_saying_80_percent() {
	let _j = temp_jocasta();
	let (s, usage) = quota_stand(at(2026, 10, 23, 14, 0));
	*usage.lock().unwrap() = window(0.796);
	s.day.read_quota().await;
	assert_eq!(toast_texts(&s), ["Subscription quota: 5h at 80%"]);
}

#[tokio::test]
async fn a_due_reminder_filed_for_a_project_keeps_the_project_on_its_awaiting_you_row_one_without_stays_bare() {
	let _j = temp_jocasta();
	let acme = project("p1", "acme-api");
	let s = Stand::in_projects(at(2026, 10, 23, 9, 0), vec![acme.clone()]);
	s.day.remind("Stretch and check the build", "09:30", Some(&acme)).unwrap();
	s.day.remind("call the bank", "09:45", None).unwrap();
	s.set(at(2026, 10, 23, 10, 0));
	let row = |text: &str, time: &str, project: Option<&str>| NeedsYou {
		kind: NeedsYouKind::Reminder,
		text: text.to_owned(),
		url: None,
		pane_id: None,
		project: project.map(str::to_owned),
		time: Some(time.to_owned()),
		since: None,
		overlaps: None,
	};
	assert_eq!(
		s.day.today().unwrap().needs_you,
		vec![row("Stretch and check the build", "09:30", Some("acme-api")), row("call the bank", "09:45", None)]
	);
}

#[tokio::test]
async fn leftover_worktrees_reach_the_dashboard_as_the_service_lists_them() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 10, 23, 14, 0));
	assert!(s.day.dashboard().leftovers.is_empty());
	let leftovers = vec![Leftover {
		project: "acme-api".to_owned(),
		folder: PathBuf::from("/code/acme-api.wt/retry"),
		branch: Some("retry".to_owned()),
		ahead: 2,
		dirty: 1,
	}];
	*s.shared.leftovers.lock().unwrap() = leftovers.clone();
	s.day.refresh_leftovers().await;
	assert_eq!(s.day.dashboard().leftovers, leftovers);
	let pushed = s.sent().into_iter().rfind(|m| matches!(m, ServerMessage::Dashboard { .. }));
	assert!(matches!(pushed, Some(ServerMessage::Dashboard { dashboard }) if dashboard.leftovers == leftovers));
}

#[tokio::test]
async fn the_weekly_review_runs_from_monday_10_outside_quiet_hours_once_per_project_and_week_its_result_is_an_event() {
	let _j = temp_jocasta();
	let p1 = project("p1", "acme-api");
	let p2 = project("p2", "client-a");
	// Monday 26 Oct 2026, 09:30.
	let s = Stand::in_projects(at(2026, 10, 26, 9, 30), vec![p1.clone(), p2.clone()]);
	let review: Review = Arc::new(|p| (p.id == "p1").then(|| "Two modules now parse dates their own way.".to_owned()));
	*s.shared.review.lock().unwrap() = Arc::clone(&review);
	s.day.tick().unwrap();
	assert!(s.shared.reviews.lock().unwrap().is_empty());
	s.set(at(2026, 10, 26, 10, 0));
	s.day.tick().unwrap();
	settle().await;
	assert_eq!(*s.shared.reviews.lock().unwrap(), ["acme-api", "client-a"]);
	assert_eq!(s.alerts(), ["Weekly review: acme-api — Two modules now parse dates their own way."]);
	// Delivered once: never reminded of, and not run again this week, even by a new Day (a restart).
	s.set(at(2026, 10, 27, 11, 0));
	s.day.tick().unwrap();
	settle().await;
	assert_eq!(*s.shared.reviews.lock().unwrap(), ["acme-api", "client-a"]);
	assert_eq!(s.alerts().len(), 1);
	assert!(s.day.queue(|q| q.open()).unwrap().iter().all(|r| r.kind != "weekly-review"));
	let again = Stand::in_projects(at(2026, 10, 28, 12, 0), vec![p1]);
	*again.shared.review.lock().unwrap() = review;
	again.day.tick().unwrap();
	settle().await;
	assert!(again.shared.reviews.lock().unwrap().is_empty());
	// The next week runs again.
	s.set(at(2026, 11, 2, 10, 5));
	s.day.tick().unwrap();
	settle().await;
	assert_eq!(*s.shared.reviews.lock().unwrap(), ["acme-api", "client-a", "acme-api", "client-a"]);
}

#[tokio::test]
async fn a_day_whose_end_the_machine_slept_through_gets_its_worklog_at_the_first_tick_of_the_next_day() {
	let _j = temp_jocasta();
	let p1 = Project { git: false, ..project("p1", "acme-api") };
	// Monday 9 Nov 2026, 21:30: the lid closes before the day's end (23:00) and opens at 08:00 the next day.
	let s = Stand::in_projects(at(2026, 11, 9, 21, 30), vec![p1]);
	let day = "2026-11-09";
	let worked = vec![at(2026, 11, 9, 10, 0), at(2026, 11, 9, 10, 10)];
	*s.shared.sessions.lock().unwrap() = Arc::new(move |asked| {
		if asked == day {
			std::collections::HashMap::from([(
				"p1".to_owned(),
				vec![WorklogSession { label: "Retry flake".to_owned(), times: worked.clone() }],
			)])
		} else {
			std::collections::HashMap::new()
		}
	});
	s.day.tick().unwrap();
	s.day.worklog_writing().await;
	assert!(!worklog_path(day).exists());
	s.set(at(2026, 11, 10, 8, 0));
	s.day.tick().unwrap();
	s.day.worklog_writing().await;
	let text = std::fs::read_to_string(worklog_path(day)).unwrap();
	assert!(text.contains("## acme-api · 10m"), "{text}");
	assert!(text.contains("- Retry flake · 10m"), "{text}");
}
