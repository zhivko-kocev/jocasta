//! Tests of `day.rs` ported from `test/frontier/server/day-threads.test.ts`: what his day shows about threads and quota
//! (0456) and the weekly review's schedule (0470), on a Day with stand-in hooks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::tests::{Review, Stand, at, project, settle, thread, write_settings};
use super::*;
use crate::frontier::protocol::{
	LandedItem, NeedsYou, NeedsYouKind, PrCi, PrFacts, QuotaCapacity, QuotaSpan, RailItem, ThreadDiff,
};
use crate::shared::events::queue::EventSource;
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

/// A landing without a PR.
fn landing<'a>(pane_id: &'a str, title: &'a str, project_id: &'a str, line: &'a str) -> Landing<'a> {
	Landing { pane_id, title, project_id, line, url: None }
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
		pr: None,
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
			diff: None,
			check: None,
			stuck_for: None,
			halted: None,
			progress: None,
			budget: None,
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
	s.day.landed(&landing("w0:p1", "Old work", "p1", "merged into main as 1111111")).unwrap();
	s.set(now - 2 * HOUR);
	s.day.landed(&landing("w4:p1", "Add subtract", "p1", "merged into main as 6796653")).unwrap();
	s.set(now - HOUR);
	s.day.landed(&landing("w6:p1", "Add farewell — v2", "p2", "PR open")).unwrap();
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
async fn a_held_line_an_earlier_version_left_is_resolved_at_the_next_tick_and_never_reaches_the_boss_0695() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 10, 25, 9, 0));
	let source_id = "thread:held:2026-10-24T23:00:00Z|demo/Add divide — finished its turn";
	s.day
		.queue(|q| {
			q.add(&event(
				source_id,
				EventSource::Thread,
				"held",
				EventPriority::Normal,
				"demo/Add divide — finished its turn",
			))
		})
		.unwrap();
	s.day.tick().unwrap();
	assert!(s.day.queue(|q| q.by_source(source_id)).unwrap().unwrap().resolved_at.is_some());
	// Not even as the one "Still open" repeat.
	s.add(60 * 60_000);
	s.day.tick().unwrap();
	assert!(s.alerts().is_empty());
	assert!(toast_texts(&s).is_empty());
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
			text: "Subscription quota (account): 5h at 84%".to_owned(),
			priority: Some(ToastPriority::Urgent),
			pane_id: None,
			url: None,
		}]
	);
	*usage.lock().unwrap() = window(0.1);
	s.day.read_quota().await;
	*usage.lock().unwrap() = window(0.9);
	s.day.read_quota().await;
	assert_eq!(toast_texts(&s), ["Subscription quota (account): 5h at 84%", "Subscription quota (account): 5h at 90%"]);
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
	assert_eq!(toast_texts(&s), ["Subscription quota (account): 5h at 80%"]);
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
async fn the_weekly_review_runs_from_monday_10_outside_quiet_hours_once_per_listed_project_and_week_its_result_is_an_event()
 {
	let _j = temp_jocasta();
	// Only the projects `weekly.projects` names get the review (0695), by name in any case.
	write_settings(r#"{"weekly":{"projects":["acme-api","Client-A"]}}"#);
	let p1 = project("p1", "acme-api");
	let p2 = project("p2", "client-a");
	let p3 = project("p3", "demo");
	// Monday 26 Oct 2026, 09:30.
	let s = Stand::in_projects(at(2026, 10, 26, 9, 30), vec![p1.clone(), p2.clone(), p3]);
	let review: Review = Arc::new(|p| (p.id == "p1").then(|| "Two modules now parse dates their own way.".to_owned()));
	*s.shared.review.lock().unwrap() = Arc::clone(&review);
	s.day.tick().unwrap();
	assert!(s.shared.reviews.lock().unwrap().is_empty());
	s.set(at(2026, 10, 26, 10, 0));
	s.day.tick().unwrap();
	settle().await;
	assert_eq!(*s.shared.reviews.lock().unwrap(), ["acme-api", "client-a"]);
	assert_eq!(s.alerts(), ["Weekly review: acme-api — Two modules now parse dates their own way."]);
	// The drift agent's words are outside text for the boss (SEC-2).
	assert_eq!(s.shared.alerts.lock().unwrap()[0].1.outside, ["Two modules now parse dates their own way."]);
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

// ---- thread facts, stuck, PRs (0668) ----

const MIN: i64 = 60_000;

#[tokio::test]
async fn a_working_thread_silent_for_10_minutes_is_stuck_a_row_of_its_own_with_its_silence_on_the_underway_row() {
	let _j = temp_jocasta();
	let now = at(2026, 11, 3, 14, 0);
	let s = Stand::new(now);
	*s.shared.threads.lock().unwrap() = vec![
		thread(|t| {
			t.pane_id = "a".to_owned();
			t.title = "Fix login".to_owned();
			t.changed_at = now - HOUR;
			t.last_write_ms = Some(now - 11 * MIN);
		}),
		thread(|t| {
			t.pane_id = "b".to_owned();
			t.title = "Fresh".to_owned();
			t.last_write_ms = Some(now - 9 * MIN);
		}),
		thread(|t| {
			t.pane_id = "c".to_owned();
			t.title = "Idle one".to_owned();
			t.state = ThreadState::Idle;
			t.last_write_ms = Some(now - HOUR);
		}),
		thread(|t| {
			t.pane_id = "d".to_owned();
			t.title = "No file yet".to_owned();
		}),
	];
	let today = s.day.today().unwrap();
	let stuck: Vec<&NeedsYou> = today.needs_you.iter().filter(|n| n.kind == NeedsYouKind::Stuck).collect();
	assert_eq!(stuck.len(), 1);
	assert_eq!(stuck[0].pane_id.as_deref(), Some("a"));
	assert_eq!(stuck[0].text, "Fix login — stuck, no writes for 11 min");
	assert_eq!(stuck[0].since, Some(iso(now - 11 * MIN)));
	let silent: Vec<Option<i64>> = today.underway.iter().map(|r| r.stuck_for).collect();
	assert_eq!(silent, [Some(11 * MIN), None, None, None]);
	// Ten minutes exactly is stuck; it ends once the thread writes.
	s.set(now + MIN);
	assert_eq!(
		s.day.today().unwrap().underway.iter().map(|r| r.stuck_for).collect::<Vec<_>>(),
		[Some(12 * MIN), Some(10 * MIN), None, None]
	);
	s.shared.threads.lock().unwrap()[0].last_write_ms = Some(now + MIN);
	assert_eq!(s.day.today().unwrap().underway[0].stuck_for, None);
}

#[tokio::test]
async fn the_underway_rows_carry_the_diff_and_the_check_the_service_found() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 3, 14, 0));
	*s.shared.threads.lock().unwrap() = vec![thread(|t| {
		t.diff = Some(ThreadDiff { added: 84, removed: 12 });
		t.check = Some(false);
	})];
	let row = s.day.today().unwrap().underway.remove(0);
	assert_eq!(row.diff, Some(ThreadDiff { added: 84, removed: 12 }));
	assert_eq!(row.check, Some(false));
}

#[tokio::test]
async fn a_red_pipeline_keeps_its_needs_you_row_until_the_poller_resolves_it() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 3, 14, 0));
	let mut pipeline = event(
		"github:pipeline:acme/web#7",
		EventSource::Github,
		"pipeline",
		EventPriority::Urgent,
		"Pipeline red — web on main",
	);
	pipeline.url = Some("https://example.org/acme/web/pull/7".to_owned());
	s.day.raise(&pipeline).unwrap();
	let rows = s.day.today().unwrap().needs_you;
	assert_eq!(rows.len(), 1);
	assert_eq!(rows[0].kind, NeedsYouKind::Pipeline);
	s.add(HOUR);
	assert_eq!(s.day.today().unwrap().needs_you.len(), 1, "it stays while the pipeline is red");
	s.day.queue(|queue| queue.resolve("github:pipeline:acme/web#7")).unwrap();
	assert!(s.day.today().unwrap().needs_you.is_empty());
}

#[tokio::test]
async fn a_thread_sent_as_a_pr_is_landed_with_its_prs_number_and_ci_read_back_from_its_url_across_a_restart() {
	let _j = temp_jocasta();
	let now = at(2026, 11, 3, 14, 0);
	let url = "https://example.org/acme/api/pull/7";
	let s = Stand::new(now);
	s.day.landed(&Landing { url: Some(url), ..landing("w6:p1", "Add farewell", "p1", "PR open") }).unwrap();
	s.day.landed(&landing("w7:p1", "Merged by hand", "p1", "merged into main as 1111111")).unwrap();
	assert_eq!(s.day.today().unwrap().landed.iter().map(|l| l.pr.clone()).collect::<Vec<_>>(), [None, None]);
	s.shared.prs.lock().unwrap().insert(url.to_owned(), PrFacts { number: 7, ci: PrCi::Failing });
	let landed = s.day.today().unwrap().landed;
	assert_eq!(landed[0].pr, Some(PrFacts { number: 7, ci: PrCi::Failing }));
	assert_eq!(landed[1].pr, None);
	let again = Stand::new(now);
	again.shared.prs.lock().unwrap().insert(url.to_owned(), PrFacts { number: 7, ci: PrCi::Passing });
	assert_eq!(again.day.today().unwrap().landed[0].pr, Some(PrFacts { number: 7, ci: PrCi::Passing }));
}

// ---- the 7-day average (0668) ----

#[tokio::test]
async fn the_average_is_the_mean_of_the_recorded_prior_days_and_none_with_none() {
	let _j = temp_jocasta();
	let now = at(2026, 11, 3, 14, 0);
	let s = Stand::new(now);
	assert_eq!(s.day.spend_avg_7d(), None);
	s.day.record_spend(9.0);
	assert_eq!(s.day.spend_avg_7d(), None, "today is live");
	let file = data_dir().join("spend-days.json");
	std::fs::write(&file, r#"{"2026-11-01": 2.0, "2026-11-02": 4.0}"#).unwrap();
	let again = Stand::new(now);
	assert_eq!(again.day.spend_avg_7d(), Some(3.0));
	// A new local day: yesterday's total counts from then on.
	again.day.record_spend(9.0);
	again.set(at(2026, 11, 4, 9, 0));
	again.day.record_spend(1.0);
	assert_eq!(again.day.spend_avg_7d(), Some(5.0));
}

// ---- quota for every account (0669) ----

fn two_accounts(first_5h: f64) -> String {
	let limit = |id: &str, used: f64| serde_json::json!({"id": format!("anthropic:{id}"), "window": {"id": id}, "amount": {"usedFraction": used}});
	serde_json::json!({
		"reports": [
			{"provider": "anthropic", "metadata": {"email": "ab*"}, "limits": [limit("5h", first_5h), limit("7d", 0.2)]},
			{"provider": "anthropic", "metadata": {"email": "cd*"}, "limits": [limit("5h", 0.3)]},
		],
		"capacity": {"anthropic": [{"window": "5h", "accounts": 2, "usedAccounts": 1, "remainingAccounts": 1}]},
	})
	.to_string()
}

#[tokio::test]
async fn every_account_keeps_its_windows_and_its_own_toast_the_header_window_is_the_most_used_across_them() {
	let _j = temp_jocasta();
	let (s, usage) = quota_stand(at(2026, 10, 23, 14, 0));
	*usage.lock().unwrap() = two_accounts(0.5);
	s.day.read_quota().await;
	let (accounts, capacity) = s.day.quota_accounts();
	let used = |w: &Option<QuotaWindow>| w.as_ref().map(|w| w.used);
	assert_eq!(accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["ab*", "cd*"]);
	assert_eq!(accounts.iter().map(|a| used(&a.five_h)).collect::<Vec<_>>(), [Some(0.5), Some(0.3)]);
	assert_eq!(accounts.iter().map(|a| used(&a.seven_d)).collect::<Vec<_>>(), [Some(0.2), None]);
	assert_eq!(capacity, [QuotaCapacity { window: QuotaSpan::FiveHours, remaining: 1, accounts: 2 }]);
	assert_eq!(s.day.quota().iter().map(|w| w.used).collect::<Vec<_>>(), [0.5, 0.2]);
	assert!(s.toasts().is_empty());
	// One account passes 80%: one toast naming it, not repeated while it stays high.
	*usage.lock().unwrap() = two_accounts(0.85);
	s.day.read_quota().await;
	s.day.read_quota().await;
	assert_eq!(toast_texts(&s), ["Subscription quota (ab*): 5h at 85%"]);
	// Back under, then over again: a second toast.
	*usage.lock().unwrap() = two_accounts(0.1);
	s.day.read_quota().await;
	*usage.lock().unwrap() = two_accounts(0.9);
	s.day.read_quota().await;
	assert_eq!(toast_texts(&s), ["Subscription quota (ab*): 5h at 85%", "Subscription quota (ab*): 5h at 90%"]);
	// A failing read is quiet and keeps the last good accounts.
	*usage.lock().unwrap() = "not json".to_owned();
	s.day.read_quota().await;
	assert_eq!(s.day.quota_accounts().0.len(), 2);
	assert_eq!(s.toasts().len(), 2);
	assert!(s.notes().is_empty());
}

// ---- scheduled threads (0673) ----

fn write_schedules(project_id: &str, text: &str) {
	let dir = crate::shared::paths::project_dir(project_id);
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("schedules.yml"), text).unwrap();
}

const BUMPS: &str = r#"[{"brief": "dependency bumps", "days": ["mon", "tue", "wed"], "time": "09:00"}]"#;

async fn tick(s: &Stand) {
	s.day.tick().unwrap();
	s.day.schedules_settled().await;
}

fn started(s: &Stand) -> Vec<(String, String)> {
	s.shared.starts.lock().unwrap().clone()
}

#[tokio::test]
async fn a_missed_run_starts_once_however_many_slots_passed_and_the_same_slot_never_repeats() {
	let _j = temp_jocasta();
	// Monday 2 Nov 2026, 08:00: the schedule is first seen now.
	let s = Stand::new(at(2026, 11, 2, 8, 0));
	write_schedules("p1", BUMPS);
	tick(&s).await;
	assert!(started(&s).is_empty());
	// Thursday noon: Mon, Tue and Wed 09:00 passed unseen; one run.
	s.set(at(2026, 11, 5, 12, 0));
	tick(&s).await;
	assert_eq!(started(&s), [("acme-api".to_owned(), "dependency bumps".to_owned())]);
	tick(&s).await;
	s.add(MIN);
	tick(&s).await;
	assert_eq!(started(&s).len(), 1, "the same slot never repeats");
	// The next Monday's slot runs again, once, also across a restart.
	let again = Stand::new(at(2026, 11, 9, 9, 5));
	tick(&again).await;
	assert_eq!(started(&again).len(), 1);
	tick(&again).await;
	assert_eq!(started(&again).len(), 1);
}

#[tokio::test]
async fn a_new_schedules_baseline_is_its_first_seen_time_so_past_slots_dont_fire_and_editing_one_resets_it() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 2, 10, 0));
	write_schedules("p1", BUMPS);
	tick(&s).await;
	s.add(MIN);
	tick(&s).await;
	assert!(started(&s).is_empty(), "Monday 09:00 passed before it was seen");
	// Edited: a new schedule, first seen now (Tuesday 10:00), so Tuesday 09:00 stays past.
	s.set(at(2026, 11, 3, 10, 0));
	write_schedules("p1", r#"[{"brief": "dependency bumps v2", "days": ["mon", "tue"], "time": "09:00"}]"#);
	tick(&s).await;
	assert!(started(&s).is_empty());
	s.set(at(2026, 11, 9, 9, 1));
	tick(&s).await;
	assert_eq!(started(&s), [("acme-api".to_owned(), "dependency bumps v2".to_owned())]);
}

#[tokio::test]
async fn a_clock_moved_backwards_does_not_run_a_slot_again() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 2, 8, 0));
	write_schedules("p1", BUMPS);
	tick(&s).await;
	s.set(at(2026, 11, 3, 9, 30));
	tick(&s).await;
	assert_eq!(started(&s).len(), 1);
	s.set(at(2026, 11, 2, 12, 0));
	tick(&s).await;
	s.set(at(2026, 11, 3, 12, 0));
	tick(&s).await;
	assert_eq!(started(&s).len(), 1);
}

#[tokio::test]
async fn a_start_error_is_reported_once_and_not_retried_for_that_slot() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 2, 8, 0));
	*s.shared.start.lock().unwrap() = Arc::new(|_, _| Err(Error::other("no herdr")));
	write_schedules("p1", BUMPS);
	tick(&s).await;
	s.set(at(2026, 11, 2, 9, 30));
	tick(&s).await;
	tick(&s).await;
	assert_eq!(started(&s).len(), 1);
	assert_eq!(s.notes(), ["Scheduled thread \"dependency bumps\" in acme-api didn't start: no herdr"]);
}

#[tokio::test]
async fn a_skipped_start_tells_him_why() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 11, 2, 8, 0));
	*s.shared.start.lock().unwrap() =
		Arc::new(|_, _| Ok(ScheduleStart::Skipped("a thread titled dependency bumps is already open".to_owned())));
	write_schedules("p1", BUMPS);
	tick(&s).await;
	s.set(at(2026, 11, 2, 9, 30));
	tick(&s).await;
	assert_eq!(
		s.notes(),
		["Scheduled thread \"dependency bumps\" in acme-api skipped: a thread titled dependency bumps is already open"]
	);
}

#[tokio::test]
async fn a_bad_schedules_file_is_reported_once_and_skipped_and_the_others_still_run() {
	let _j = temp_jocasta();
	let projects = vec![project("p1", "acme-api"), project("p2", "client-a")];
	let s = Stand::in_projects(at(2026, 11, 2, 8, 0), projects);
	write_schedules("p1", "[{\"brief\": \"x\", \"days\": [\"someday\"], \"time\": \"09:00\"}]");
	write_schedules("p2", BUMPS);
	tick(&s).await;
	tick(&s).await;
	assert_eq!(s.notes().len(), 1);
	assert!(s.notes()[0].starts_with("Schedules of acme-api: "), "{:?}", s.notes());
	s.set(at(2026, 11, 2, 9, 30));
	tick(&s).await;
	assert_eq!(started(&s), [("client-a".to_owned(), "dependency bumps".to_owned())]);
	assert_eq!(s.notes().len(), 1, "said once");
}

#[tokio::test]
async fn next_shows_the_soonest_three_runs_across_projects() {
	let _j = temp_jocasta();
	let projects = vec![project("p1", "acme-api"), project("p2", "client-a")];
	let s = Stand::in_projects(at(2026, 11, 2, 10, 0), projects.clone());
	write_schedules(
		"p1",
		r#"[{"brief": "dependency bumps", "days": ["mon", "tue"], "time": "09:00"},
		    {"brief": "weekly report\nmore", "days": ["fri"], "time": "16:00"}]"#,
	);
	write_schedules("p2", r#"[{"brief": "audit", "days": ["tue"], "time": "07:30"}]"#);
	let next = s.day.scheduled_next(&projects);
	let shown: Vec<(&str, &str, i64)> = next.iter().map(|n| (n.project.as_str(), n.brief.as_str(), n.at_ms)).collect();
	assert_eq!(
		shown,
		[
			("client-a", "audit", at(2026, 11, 3, 7, 30)),
			("acme-api", "dependency bumps", at(2026, 11, 3, 9, 0)),
			("acme-api", "weekly report", at(2026, 11, 6, 16, 0)),
		]
	);
	assert_eq!(s.day.today().unwrap().scheduled, next);
}
