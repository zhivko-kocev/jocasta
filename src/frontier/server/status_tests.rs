//! Tests of `status.rs`. The TypeScript cases drive the whole service over HTTP; these run on the part itself with a
//! context whose day reads its thread rows through `StatusViews::day_thread`, as the service's hooks do. Cases that need
//! the real service's flow (a refresh tick, a thread's start, a stop) are on the list in the report.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use super::*;
use crate::frontier::protocol::{QuotaSpan, ServiceHealth};
use crate::frontier::server::day::{AlertDetails, Day, DayHooks, DayOptions, SharedCatalog};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::report::{Outcome, ThreadReport};
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::sessions::TestsResult;
use crate::frontier::server::task_file::{NewTask, TaskReport, TaskReview, new_task, task_dir};
use crate::frontier::server::thread::{Phase, PlanProposed, ReviewResult, ThreadPlan, ThreadReview};
use crate::frontier::server::worklog::WorklogSession;
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::projects_file;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// The day's hooks as the service wires them: thread rows come from `StatusViews::day_thread`.
struct StatusHooks {
	link: Link,
}

impl DayHooks for StatusHooks {
	fn send(&self, message: ServerMessage) {
		self.link.ctx().events.send(message);
	}

	fn alert(&self, _: &str, _: AlertDetails) {}

	fn notify(&self, _: &str, _: ToastLevel) {}

	fn projects(&self) -> Vec<Project> {
		Vec::new()
	}

	fn threads(&self) -> Vec<DayThread> {
		let c = self.link.ctx();
		c.threads.all().iter().map(|t| c.status.day_thread(t)).collect()
	}

	fn sessions_by_project(&self, _: &str) -> HashMap<String, Vec<WorklogSession>> {
		HashMap::new()
	}

	fn prepare_sessions(&self, _: &str) -> BoxFuture<'_, crate::shared::errors::Result<()>> {
		Box::pin(async { Ok(()) })
	}

	fn services(&self) -> BoxFuture<'_, crate::shared::errors::Result<Vec<ServiceHealth>>> {
		Box::pin(async { Ok(Vec::new()) })
	}

	fn project_name(&self, _: &Path) -> Option<String> {
		None
	}

	fn leftovers(&self) -> BoxFuture<'_, crate::shared::errors::Result<Vec<Leftover>>> {
		Box::pin(async { Ok(Vec::new()) })
	}

	fn weekly_review(&self, _: &Project) -> BoxFuture<'_, crate::shared::errors::Result<Option<String>>> {
		Box::pin(async { Ok(None) })
	}
}

/// A context over a temp Jocasta, a [`FakeHerdr`] and a day wired like the service's.
pub(crate) struct World {
	pub ctx: Arc<ServiceContext>,
	pub herdr: Arc<FakeHerdr>,
	pub sent: Arc<Mutex<Vec<ServerMessage>>>,
	pub j: TempJocasta,
}

pub(crate) fn world(options: impl FnOnce(&mut ServiceOptions)) -> World {
	let j = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	let sent = Arc::new(Mutex::new(Vec::new()));
	let broadcast = Arc::clone(&sent);
	let mut service = ServiceOptions::new(
		herdr.clone(),
		Arc::new(move |message| broadcast.lock().unwrap_or_else(PoisonError::into_inner).push(message)),
	);
	service.sessions_dir = Some(j.root().join("sessions"));
	service.day = Some(DayOptions { pollers: Some(false), ..DayOptions::default() });
	options(&mut service);
	let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog().expect("the catalog")));
	let link = Link::new();
	let hooks = Arc::new(StatusHooks { link: link.clone() });
	let day = Day::new(catalog.clone(), hooks, service.day.take().unwrap_or_default()).expect("the day");
	let ctx = ServiceContext::new(&link, catalog, service, day);
	World { ctx, herdr, sent, j }
}

/// The HUD's awaiting count and today's spend as last sent to the page and the client shell.
fn last_hud(w: &World) -> Option<(u32, Option<f64>)> {
	w.sent.lock().unwrap().iter().rev().find_map(|message| match message {
		ServerMessage::Hud { hud } => Some((hud.awaiting, hud.spend_today)),
		_ => None,
	})
}

pub(crate) fn demo(w: &World) -> Project {
	let main = w.j.root().join("demo");
	fs::create_dir_all(&main).unwrap();
	Project {
		id: "p-demo".to_owned(),
		identity: String::new(),
		name: "demo".to_owned(),
		main_path: main,
		git: true,
		paths: vec![],
	}
}

/// A thread `title` of project demo in `pane`, registered, in a worktree folder that is no repository.
fn thread(w: &World, pane: &str, title: &str) -> ThreadRef {
	let slug = title.to_lowercase().replace(' ', "-");
	let path = w.j.root().join("wt").join(&slug);
	fs::create_dir_all(&path).unwrap();
	let mut t = Thread::new(pane, "w-1", demo(w), title, format!("feature/{slug}"));
	t.path = path;
	t.state = AgentState::Idle;
	let t = crate::frontier::server::context::Shared::new(t);
	w.ctx.threads.add(t.clone());
	t
}

fn now() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

/// An omp session file in omp's shape: its folder is `sessions/--demo--`, each file its own session and message ids.
fn session(w: &World, name: &str, messages: &[(&str, f64)]) -> PathBuf {
	session_with(w, name, &messages.iter().map(|(text, cost)| (*text, *cost, None)).collect::<Vec<_>>())
}

fn session_with(w: &World, name: &str, messages: &[(&str, f64, Option<&str>)]) -> PathBuf {
	let dir = w.j.root().join("sessions").join("--demo--");
	fs::create_dir_all(&dir).unwrap();
	let file = dir.join(format!("2026-10-13T10-00-00-000Z_{name}.jsonl"));
	let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
	let mut lines = vec![
		json!({"type": "title", "v": 1, "title": "Readme line"}).to_string(),
		json!({"type": "session", "id": format!("s-{name}"), "cwd": "/wt/x", "timestamp": stamp}).to_string(),
	];
	for (i, (text, cost, ask)) in messages.iter().enumerate() {
		let mut content = vec![json!({"type": "text", "text": text})];
		if let Some(ask) = ask {
			let html = format!(
				"<form><p>{ask}</p><label><input type=\"radio\" name=\"answer\" value=\"yes\" checked> Yes</label></form>"
			);
			content.push(json!({"type": "toolCall", "id": "c1", "name": "ask", "arguments": {"html": html}}));
		}
		let message = json!({"role": "assistant", "provider": "anthropic", "model": "claude-opus-5-5", "timestamp": now(),
			"stopReason": "stop", "usage": {"cost": {"total": cost}}, "content": content});
		lines.push(json!({"type": "message", "id": format!("{name}-m{i}"), "message": message}).to_string());
	}
	fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
	file
}

fn with_session(t: &ThreadRef, file: &Path) {
	t.with(|t| t.session_path = Some(file.to_path_buf()));
}

fn busy(w: &World) -> (usize, usize) {
	let busy = w.ctx.status.busy_threads().unwrap();
	(busy.working, busy.awaiting)
}

fn reviewed(at: i64) -> ThreadReview {
	let result = ReviewResult {
		notes: Some(2),
		blocking: None,
		tests: TestsResult::Pass,
		proof: None,
		checks: vec![],
		at: Some(at),
	};
	ThreadReview { result: Some(result), ..ThreadReview::default() }
}

fn waiting_plan() -> ThreadPlan {
	ThreadPlan {
		awaiting: Some("Readme line plan".to_owned()),
		proposed: Some(PlanProposed { title: "Readme line plan".to_owned(), file: None }),
		..ThreadPlan::default()
	}
}

fn pane_reports(w: &World) -> Vec<Vec<Value>> {
	w.herdr.of("reportPaneMetadata")
}

fn workspace_reports(w: &World) -> Vec<Vec<Value>> {
	w.herdr.of("reportWorkspaceMetadata")
}

fn project_workspace(w: &World, id: &str) {
	let info = WorkspaceInfo { id: id.to_owned(), label: "demo".to_owned(), cwd: None };
	w.herdr.state().workspace_list.push(info.clone());
	let mut known = w.ctx.status.workspaces();
	known.push(info);
	w.ctx.status.set_workspaces(known);
}

// ---- busyThreads and the HUD ----

#[tokio::test]
async fn a_thread_awaiting_him_counts_as_awaiting_not_working_in_the_hud_and_busy_threads() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.set_warmed();
	let asked = session_with(&w, "ask", &[("Which format?", 0.1, Some("CSV or JSON?"))]);
	with_session(&t, &asked);
	t.with(|t| {
		t.state = AgentState::Blocked;
		t.shown = ThreadState::Blocked;
	});
	assert_eq!(w.ctx.status.hud().unwrap().blocked, 1);
	assert_eq!(busy(&w), (0, 1));
	assert_eq!(w.ctx.status.hud().unwrap().awaiting, 1);
	let today = w.ctx.day.today().unwrap();
	assert_eq!(today.needs_you[0].text, "Readme line — CSV or JSON?");
	// He dismisses its question: nothing awaits him any more.
	t.with(|t| {
		t.state = AgentState::Idle;
		t.shown = ThreadState::Idle;
	});
	assert_eq!(w.ctx.status.hud().unwrap().blocked, 0);
	assert_eq!(busy(&w), (0, 0));
}

#[tokio::test]
async fn a_turn_running_or_started_and_not_yet_reported_is_work_and_an_idle_thread_is_not() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	assert_eq!(busy(&w), (0, 0));
	t.with(|t| t.state = AgentState::Working);
	assert_eq!(busy(&w), (1, 0));
	assert_eq!(w.ctx.status.hud().unwrap().running, 1);
	// herdr says idle, a turn was started and not yet reported (`shown`): still at work (0434, 0549).
	t.with(|t| {
		t.state = AgentState::Idle;
		t.shown = ThreadState::Working;
	});
	assert_eq!(busy(&w), (1, 0));
	assert_eq!(w.ctx.status.hud().unwrap().running, 0);
	// The pane herdr restored as a shell is relaunched: its new omp comes up idle, and the close question counts 0.
	t.with(|t| t.shown = ThreadState::Idle);
	assert_eq!(busy(&w), (0, 0));
}

#[tokio::test]
async fn a_thread_at_work_is_never_counted_where_it_awaits_him() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| t.state = AgentState::Working);
	let awaited = HashSet::from(["1-1".to_owned()]);
	assert!(!w.ctx.status.at_work(&t, &awaited));
	assert!(w.ctx.status.at_work(&t, &HashSet::new()));
	t.with(|t| {
		t.state = AgentState::Done;
		t.shown = ThreadState::Working;
	});
	assert!(w.ctx.status.at_work(&t, &HashSet::new()));
	t.with(|t| t.shown = ThreadState::Done);
	assert!(!w.ctx.status.at_work(&t, &HashSet::new()));
}

#[tokio::test]
async fn awaited_panes_are_the_panes_of_the_rows_of_awaiting_you() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	thread(&w, "2-1", "Second");
	assert!(w.ctx.status.awaited_panes().unwrap().is_empty());
	t.with(|t| t.shown = ThreadState::Blocked);
	assert_eq!(w.ctx.status.awaited_panes().unwrap(), HashSet::from(["1-1".to_owned()]));
}

#[tokio::test]
async fn a_plan_waiting_in_omp_s_plan_review_awaits_him_and_thread_status_says_plan_ready_with_its_spend() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.set_warmed();
	with_session(&t, &session(&w, "plan", &[("Planning.", 0.25)]));
	// herdr says idle there; the plan waits on him.
	t.with(|t| {
		t.state = AgentState::Idle;
		t.shown = ThreadState::Blocked;
		t.plan = Some(waiting_plan());
	});
	assert_eq!(busy(&w), (0, 1));
	assert_eq!(w.ctx.status.hud().unwrap().awaiting, 1);
	assert_eq!(
		w.ctx.status.thread_status(None, None).await.unwrap(),
		"demo/Readme line [feature/readme-line] plan ready\n  spend $0.25"
	);
}

#[tokio::test]
async fn reviewed_work_awaits_him_until_finished_and_thread_status_says_done() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| {
		t.shown = ThreadState::Done;
		t.review = Some(reviewed(now()));
	});
	assert_eq!(busy(&w), (0, 1));
	assert_eq!(w.ctx.status.hud().unwrap().awaiting, 1);
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(status.starts_with("demo/Readme line [feature/readme-line] done"), "{status}");
	assert_eq!(status, "demo/Readme line [feature/readme-line] done — done and reviewed: 2 notes, tests pass");
}

#[tokio::test]
async fn an_idle_thread_awaits_nothing_and_thread_status_says_idle() {
	let w = world(|_| {});
	thread(&w, "1-1", "Task B");
	assert_eq!(w.ctx.status.thread_status(None, None).await.unwrap(), "demo/Task B [feature/task-b] idle");
	assert_eq!(busy(&w), (0, 0));
}

#[tokio::test]
async fn the_hud_counts_a_working_and_a_blocked_thread_apart_and_says_when_quiet_hours_are_on() {
	let w = world(|_| {});
	let a = thread(&w, "1-1", "One");
	let b = thread(&w, "2-1", "Two");
	a.with(|t| t.state = AgentState::Working);
	b.with(|t| {
		t.state = AgentState::Idle;
		t.shown = ThreadState::Blocked;
	});
	let hud = w.ctx.status.hud().unwrap();
	assert_eq!((hud.running, hud.blocked, hud.awaiting), (1, 1, 1));
	assert_eq!(hud.quiet, Some(w.ctx.day.quiet()));
	assert!(hud.quota.is_empty());
}

#[tokio::test]
async fn the_spend_reads_null_until_today_s_sessions_are_read_then_the_day_s_total() {
	let w = world(|_| {});
	assert_eq!(w.ctx.status.hud().unwrap().spend_today, None);
	w.ctx.set_warmed();
	assert_eq!(w.ctx.status.hud().unwrap().spend_today, Some(0.0));
}

// ---- the HUD and the sidebar's tokens ----

#[tokio::test]
async fn sidebar_tokens_per_thread_pane_and_project_workspace_the_hud_and_the_tokens_cleared_when_the_service_stops() {
	let w = world(|_| {});
	project_workspace(&w, "w_demo");
	w.ctx.set_warmed();
	// Every omp session of the day counts (the HUD matches omp's own stats), not only the threads'.
	session(&w, "elsewhere", &[("elsewhere", 9.0)]);
	let t = thread(&w, "1-1", "Readme line");
	with_session(&t, &session(&w, "mine", &[("Planning.", 1.25), ("Done planning.", 0.5)]));
	t.with(|t| t.state = AgentState::Working);
	w.ctx.status.report().await.unwrap();
	assert!(pane_reports(&w).contains(&vec![
		json!("1-1"),
		json!({"tokens": {"model": "opus-5-5", "cost": "$1.75"},
			// "done" is labelled "review ready" only once a review's notes are counted (0394).
			"stateLabels": {"blocked": "awaiting you"}, "ttlMs": 900_000}),
	]));
	assert!(workspace_reports(&w).contains(&vec![json!("w_demo"), json!({"running": "1 at work"})]));
	// The sessions the listing found are read off the call; the HUD says the spend and NEEDS YOU's rows.
	w.ctx.calls.settled().await;
	w.ctx.status.report().await.unwrap();
	assert_eq!(last_hud(&w), Some((0, Some(10.75))));
	// A thread awaiting him counts once.
	t.with(|t| t.shown = ThreadState::Blocked);
	w.ctx.status.report().await.unwrap();
	assert_eq!(last_hud(&w), Some((1, Some(10.75))));
	assert!(workspace_reports(&w).contains(&vec![json!("w_demo"), json!({"running": "1 at work · 1 awaiting you"})]));
	// Stopped: the HUD says so, and the tokens are cleared (UX-13).
	// Stopped: the tokens are cleared (UX-13).
	w.ctx.status.clear().await;
	assert_eq!(pane_reports(&w).last().unwrap(), &vec![json!("1-1"), json!({"tokens": {"model": null, "cost": null}})]);
	assert_eq!(workspace_reports(&w).last().unwrap()[1], json!({"running": null}));
}

#[tokio::test]
async fn the_boss_pane_carries_the_pin_token_renewed_with_the_rest_kept_when_it_is_the_only_pane_and_cleared_at_stop_0667()
 {
	let w = world(|_| {});
	w.with_boss_idle();
	w.ctx.status.report().await.unwrap();
	assert_eq!(pane_reports(&w), [vec![json!("boss-1"), json!({"tokens": {"pin": "1"}, "ttlMs": 900_000})]]);
	// Unchanged: not reported again.
	w.ctx.status.report().await.unwrap();
	assert_eq!(pane_reports(&w).len(), 1);
	// Old enough to renew: reported again, not forgotten as a pane no thread holds.
	{
		let mut st = w.ctx.status.st();
		for (_, reported) in &mut st.reported {
			reported.at -= METADATA_RENEW_MS + 1;
		}
	}
	w.ctx.status.report().await.unwrap();
	assert_eq!(pane_reports(&w).len(), 2);
	// Stopped: the pin is cleared with the pane's other tokens.
	w.ctx.status.clear().await;
	assert_eq!(
		pane_reports(&w).last().unwrap(),
		&vec![json!("boss-1"), json!({"tokens": {"model": null, "cost": null, "pin": null}})]
	);
}

#[tokio::test]
async fn a_reviewed_thread_s_state_labels_say_review_ready_and_a_waiting_plan_s_say_plan_ready() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| {
		t.shown = ThreadState::Done;
		t.review = Some(reviewed(now()));
	});
	w.ctx.status.report().await.unwrap();
	assert_eq!(
		pane_reports(&w).last().unwrap()[1]["stateLabels"],
		json!({"blocked": "awaiting you", "done": "review ready"})
	);
	t.with(|t| {
		t.plan = Some(waiting_plan());
		t.shown = ThreadState::Blocked;
	});
	w.ctx.status.report().await.unwrap();
	assert_eq!(
		pane_reports(&w).last().unwrap()[1]["stateLabels"],
		json!({"blocked": "awaiting you", "idle": "plan ready", "done": "plan ready"})
	);
}

#[tokio::test]
async fn a_blocked_thread_s_label_is_its_question() {
	let w = world(|_| {});
	w.ctx.set_warmed();
	let t = thread(&w, "1-1", "Readme line");
	with_session(&t, &session_with(&w, "ask", &[("Which format?", 0.1, Some("CSV or JSON?"))]));
	w.ctx.status.report().await.unwrap();
	assert_eq!(pane_reports(&w).last().unwrap()[1]["stateLabels"], json!({"blocked": "CSV or JSON?"}));
}

#[tokio::test]
async fn svc_14_a_project_s_running_count_goes_to_its_own_workspace_not_a_later_one_with_the_same_label() {
	let w = world(|_| {});
	project_workspace(&w, "w_demo");
	project_workspace(&w, "w_late");
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| t.state = AgentState::Working);
	w.ctx.status.report().await.unwrap();
	let reported: Vec<Value> = workspace_reports(&w).iter().map(|call| call[0].clone()).collect();
	assert!(reported.contains(&json!("w_demo")));
	assert!(!reported.contains(&json!("w_late")));
}

#[tokio::test]
async fn unchanged_tokens_are_not_reported_again_until_they_renew_and_a_failed_report_is_tried_again() {
	let w = world(|_| {});
	project_workspace(&w, "w_demo");
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| t.state = AgentState::Working);
	w.ctx.status.report().await.unwrap();
	w.ctx.status.report().await.unwrap();
	assert_eq!(pane_reports(&w).len(), 1);
	assert_eq!(workspace_reports(&w).len(), 1);
	// A change is reported at once.
	t.with(|t| t.shown = ThreadState::Blocked);
	w.ctx.status.report().await.unwrap();
	assert_eq!(workspace_reports(&w).len(), 2);
	assert_eq!(pane_reports(&w).len(), 1);
	// Old enough to renew before herdr expires it.
	{
		let mut st = w.ctx.status.st();
		for (_, reported) in &mut st.reported {
			reported.at -= METADATA_RENEW_MS + 1;
		}
	}
	w.ctx.status.report().await.unwrap();
	assert_eq!((pane_reports(&w).len(), workspace_reports(&w).len()), (2, 3));
}

#[tokio::test]
async fn a_closed_workspace_s_tokens_are_never_reported_again_and_a_workspace_with_no_thread_left_is_cleared_once_co_13()
 {
	let w = world(|_| {});
	project_workspace(&w, "w_demo");
	let t = thread(&w, "1-1", "Readme line");
	t.with(|t| t.state = AgentState::Working);
	w.ctx.status.report().await.unwrap();
	assert_eq!(workspace_reports(&w).last().unwrap()[1], json!({"running": "1 at work"}));
	// Its last thread closed: the tokens are cleared once, then forgotten.
	w.ctx.threads.drop_pane("1-1");
	w.ctx.status.report().await.unwrap();
	assert_eq!(workspace_reports(&w).last().unwrap()[1], json!({"running": null}));
	let cleared = workspace_reports(&w).len();
	w.ctx.status.report().await.unwrap();
	assert_eq!(workspace_reports(&w).len(), cleared);
	// A workspace herdr no longer lists: nothing is reported for it.
	let u = thread(&w, "2-1", "Second");
	u.with(|t| t.state = AgentState::Working);
	w.ctx.status.report().await.unwrap();
	let reported = workspace_reports(&w).len();
	w.ctx.status.set_workspaces(Vec::new());
	w.ctx.status.report().await.unwrap();
	assert_eq!(workspace_reports(&w).len(), reported);
	assert!(w.ctx.status.st().reported.iter().all(|(key, _)| !key.starts_with("ws:")));
}

#[tokio::test]
async fn a_pane_that_is_no_thread_gets_no_tokens_and_a_forgotten_thread_s_tokens_are_not_renewed() {
	let w = world(|_| {});
	w.herdr
		.state()
		.pane_list
		.push(crate::frontier::server::herdr::PaneInfo { pane_id: "shell-1".to_owned(), ..Default::default() });
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.status.report().await.unwrap();
	assert!(pane_reports(&w).iter().all(|call| call[0] != json!("shell-1")));
	assert_eq!(pane_reports(&w).len(), 1);
	w.ctx.threads.drop_pane("1-1");
	w.ctx.status.forget(&t);
	w.ctx.status.report().await.unwrap();
	assert!(w.ctx.status.st().reported.is_empty());
}

#[tokio::test]
async fn the_setup_notice_and_the_boss_notice_are_each_said_once_as_a_toast() {
	let w = world(|_| {});
	w.ctx.status.set_setup_notice("jocasta setup has changes pending.".to_owned());
	w.ctx.status.report().await.unwrap();
	w.ctx.status.report().await.unwrap();
	w.ctx.events.settled().await;
	let toasts: Vec<(ToastLevel, String)> = w
		.sent
		.lock()
		.unwrap()
		.iter()
		.filter_map(|message| match message {
			ServerMessage::Toast { level, text, .. } => Some((*level, text.clone())),
			_ => None,
		})
		.collect();
	assert_eq!(toasts, [(ToastLevel::Info, "jocasta setup has changes pending.".to_owned())]);
}

#[tokio::test]
async fn the_page_gets_the_hud_when_only_the_quota_changed() {
	let usage =
		json!({"reports": [{"limits": [{"window": {"id": "5h"}, "amount": {"usedFraction": 0.5}}]}]}).to_string();
	let w = world(|options| {
		options.day = Some(DayOptions {
			pollers: Some(false),
			usage: Some(Arc::new(move || {
				let text = usage.clone();
				Box::pin(async move { Ok(text) })
			})),
			..DayOptions::default()
		});
	});
	w.ctx.status.report().await.unwrap();
	assert_eq!(last_hud(&w).map(|(awaiting, _)| awaiting), Some(0));
	w.ctx.day.read_quota().await;
	w.ctx.status.report().await.unwrap();
	let sent = w.sent.lock().unwrap();
	let huds: Vec<&Hud> = sent
		.iter()
		.filter_map(|message| match message {
			ServerMessage::Hud { hud } => Some(hud),
			_ => None,
		})
		.collect();
	assert_eq!(huds.len(), 2);
	assert!(huds[0].quota.is_empty());
	assert_eq!((huds[1].quota[0].window, huds[1].quota[0].used), (QuotaSpan::FiveHours, 0.5));
}

#[tokio::test]
async fn the_hud_is_sent_only_when_its_figures_change() {
	let w = world(|_| {});
	w.ctx.status.report().await.unwrap();
	w.ctx.status.report().await.unwrap();
	assert_eq!(last_hud(&w), Some((0, None)));
	let huds = |w: &World| w.sent.lock().unwrap().iter().filter(|m| matches!(m, ServerMessage::Hud { .. })).count();
	assert_eq!(huds(&w), 1);
	w.ctx.set_warmed();
	w.ctx.status.report().await.unwrap();
	assert_eq!(last_hud(&w), Some((0, Some(0.0))));
	assert_eq!(huds(&w), 2);
}

// ---- spend passed $50 ----

fn boss_lines(w: &World) -> Vec<String> {
	w.herdr.of("prompt").iter().map(|args| args[1].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn prm_11_a_thread_found_past_50_dollars_on_the_first_look_was_warned_before_this_run() {
	let w = world(|_| {});
	w.with_boss_idle();
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.set_warmed();
	with_session(&t, &session(&w, "big", &[("Spent a lot.", 51.2)]));
	let line = "[event] demo/Readme line — spend passed $50.00 ($51.20); omp warned in its pane".to_owned();
	w.ctx.status.spend_passed().await;
	assert_eq!(t.with(|t| t.spend_said), Some(true));
	assert!(!boss_lines(&w).contains(&line));
}

#[tokio::test]
async fn prm_11_a_thread_below_50_dollars_crossing_it_says_so_to_the_boss_once() {
	let w = world(|_| {});
	let boss = w.with_boss_idle();
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.set_warmed();
	let file = session(&w, "grows", &[("Spent some.", 12.0)]);
	with_session(&t, &file);
	w.ctx.status.spend_passed().await;
	assert_eq!(t.with(|t| t.spend_said), Some(false));
	assert!(boss_lines(&w).is_empty());
	// The session passes it.
	let more = json!({"type": "message", "id": "grows-m9", "message": {"role": "assistant", "provider": "anthropic",
		"model": "claude-opus-5-5", "timestamp": now(), "stopReason": "stop", "usage": {"cost": {"total": 39.2}},
		"content": [{"type": "text", "text": "More."}]}});
	let mut text = fs::read_to_string(&file).unwrap();
	text.push_str(&format!("{more}\n"));
	fs::write(&file, text).unwrap();
	// Looked at most every few seconds: the tick's reader is a few seconds behind (SVC-8).
	w.ctx.status.st().ledgers.clear();
	w.ctx.status.spend_passed().await;
	let line = "[event] demo/Readme line — spend passed $50.00 ($51.20); omp warned in its pane".to_owned();
	assert_eq!(boss_lines(&w), std::slice::from_ref(&line));
	// Said once: later passes, the boss idle again, say nothing.
	w.ctx.events.boss_changed(AgentState::Idle).await;
	boss.with(|b| b.state = AgentState::Idle);
	w.ctx.status.spend_passed().await;
	assert_eq!(boss_lines(&w).iter().filter(|l| **l == line).count(), 1);
}

#[tokio::test]
async fn spend_passed_says_nothing_before_the_sessions_were_read() {
	let w = world(|_| {});
	w.with_boss_idle();
	let t = thread(&w, "1-1", "Readme line");
	with_session(&t, &session(&w, "big", &[("Spent a lot.", 70.0)]));
	w.ctx.status.spend_passed().await;
	assert_eq!(t.with(|t| t.spend_said), None);
	assert!(boss_lines(&w).is_empty());
}

impl World {
	fn with_boss_idle(&self) -> crate::frontier::server::context::BossRef {
		use crate::frontier::server::events::BossPane;
		let boss = crate::frontier::server::context::Shared::new(BossPane {
			pane_id: "boss-1".to_owned(),
			workspace_id: "w-boss".to_owned(),
			state: AgentState::Idle,
			session_path: None,
			event_at: None,
			prompted_at: None,
		});
		self.ctx.set_boss(Some(boss.clone()));
		boss
	}
}

// ---- thread_status ----

#[tokio::test]
async fn thread_status_with_no_threads_says_so_and_names_the_project_when_one_was_asked() {
	let w = world(|_| {});
	assert_eq!(w.ctx.status.thread_status(None, None).await.unwrap(), "No threads.");
	let project = demo(&w);
	assert_eq!(w.ctx.status.thread_status(None, Some(&project)).await.unwrap(), "No threads in demo.");
	thread(&w, "1-1", "Readme line");
	let other = Project { id: "p-other".to_owned(), name: "other".to_owned(), ..project };
	assert_eq!(w.ctx.status.thread_status(None, Some(&other)).await.unwrap(), "No threads in other.");
	assert!(w.ctx.status.thread_status(Some("nothing"), None).await.is_err());
}

fn task_of(w: &World, t: &ThreadRef) -> crate::frontier::server::task_file::TaskRecord {
	let record = new_task(
		NewTask {
			project_id: "p-demo".to_owned(),
			project: "demo".to_owned(),
			title: "Readme line".to_owned(),
			brief: "Add a README line.".to_owned(),
			path: Some(w.j.root().join("wt").join("readme-line").to_string_lossy().into_owned()),
			branch: Some("feature/readme-line".to_owned()),
			base: Some("abc1234".to_owned()),
			..NewTask::default()
		},
		Some(now() - 60_000),
	);
	t.with(|t| t.task = Some(record.clone()));
	record
}

fn report(outcome: Outcome) -> ThreadReport {
	ThreadReport {
		outcome,
		summary: "Readme line added".to_owned(),
		files: vec!["README.md".to_owned()],
		tests: vec![crate::frontier::server::report::ReportedTest { command: "bun test".to_owned(), passed: true }],
		decisions: vec![],
		questions: vec![],
		follow_ups: vec![],
	}
}

#[tokio::test]
async fn a_thread_s_report_shows_in_the_list_as_its_outcome_and_in_full_for_a_named_thread_with_its_task_file() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	thread(&w, "2-1", "Second");
	let record = task_of(&w, &t);
	let file = task_file(&record.project_id, &record.id);
	assert_eq!(
		w.ctx.status.thread_status(Some("Readme line"), None).await.unwrap(),
		format!("demo/Readme line [feature/readme-line] idle\ntask file: {}\nreport: none yet", file.display())
	);
	// The report came in after the last review: this task's.
	t.with(|t| {
		t.task.as_mut().unwrap().report = Some(TaskReport { at: now(), value: report(Outcome::Done) });
	});
	let full = r#"outcome done · "Readme line added" · tests 1/1 · files: README.md"#;
	let status =
		w.ctx.status.thread_status(None, None).await.unwrap().split('\n').map(str::to_owned).collect::<Vec<_>>();
	assert!(status.contains(&"demo/Readme line [feature/readme-line] idle · reported done".to_owned()), "{status:?}");
	assert!(status.contains(&"demo/Second [feature/second] idle".to_owned()));
	let named = w.ctx.status.thread_status(Some("Readme line"), None).await.unwrap();
	assert!(named.ends_with(&format!("\nreport: {full}")), "{named}");
	// A report from before the task's last review is an earlier task's: not shown as reported in the list.
	t.with(|t| {
		let task = t.task.as_mut().unwrap();
		task.reviews.push(TaskReview {
			at: now() + 1_000,
			passed: true,
			checks: vec![],
			said: "no report".to_owned(),
			notes: None,
			blocking: None,
			tests: None,
			proof: None,
		});
	});
	let listed = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(!listed.contains("reported done"), "{listed}");
	let named = w.ctx.status.thread_status(Some("Readme line"), None).await.unwrap();
	assert!(named.ends_with(&format!("\nreport: {full} (an earlier task's)")), "{named}");
}

#[tokio::test]
async fn thread_status_says_each_thread_s_ledger_line_once_its_session_has_model_calls() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	w.ctx.set_warmed();
	assert_eq!(w.ctx.status.thread_status(None, None).await.unwrap(), "demo/Readme line [feature/readme-line] idle");
	with_session(&t, &session(&w, "edited", &[("Edited the README.", 0.25)]));
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(status.contains("\n  spend $0.25"), "{status}");
	let named = w.ctx.status.thread_status(Some("Readme line"), None).await.unwrap();
	assert!(named.contains("\nspend $0.25"), "{named}");
}

#[tokio::test]
async fn a_thread_line_says_what_its_session_last_said_unless_that_is_its_state() {
	let w = world(|_| {});
	w.ctx.set_warmed();
	let t = thread(&w, "1-1", "Readme line");
	with_session(&t, &session(&w, "said", &[("Working on the readme.\nSecond line.", 0.1)]));
	t.with(|t| t.state = AgentState::Working);
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert_eq!(status, "demo/Readme line [feature/readme-line] working — Working on the readme.\n  spend $0.10");
	t.with(|t| {
		t.shown = ThreadState::Done;
		t.phase = Phase::Review;
		t.state = AgentState::Idle;
	});
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(status.starts_with("demo/Readme line [feature/readme-line] done — in review"), "{status}");
}

// ---- today's thread rows ----

#[tokio::test]
async fn a_thread_row_in_today_says_its_question_its_plan_its_review_or_its_last_line_with_cost_and_context() {
	let w = world(|_| {});
	w.ctx.set_warmed();
	let t = thread(&w, "1-1", "Readme line");
	let file = session_with(&w, "row", &[("Last thing said.", 0.5, None)]);
	with_session(&t, &file);
	let row = w.ctx.status.day_thread(&t);
	assert_eq!(
		(row.pane_id.as_str(), row.title.as_str(), row.project.as_deref(), row.line.as_str(), row.state),
		("1-1", "Readme line", Some("demo"), "Last thing said.", ThreadState::Idle)
	);
	assert_eq!(row.cost, Some(0.5));
	assert_eq!((row.overlaps, row.reviewed_at), (None, None));
	// A thread blocked: its question, else "awaiting you"; a plan waiting: "plan ready".
	t.with(|t| t.shown = ThreadState::Blocked);
	assert_eq!(w.ctx.status.day_thread(&t).line, "Last thing said.".replace("Last thing said.", "awaiting you"));
	with_session(&t, &session_with(&w, "ask", &[("Which?", 0.1, Some("CSV or JSON?"))]));
	assert_eq!(w.ctx.status.day_thread(&t).line, "CSV or JSON?");
	t.with(|t| t.plan = Some(waiting_plan()));
	assert_eq!(w.ctx.status.day_thread(&t).line, "plan ready");
	// Done: "done" before a review, "in review" while it runs, then what the review found, with when.
	t.with(|t| {
		t.plan = None;
		t.shown = ThreadState::Done;
	});
	assert_eq!(w.ctx.status.day_thread(&t).line, "done");
	t.with(|t| t.phase = Phase::Review);
	assert_eq!(w.ctx.status.day_thread(&t).line, "in review");
	let at = now();
	t.with(|t| {
		t.phase = Phase::Task;
		t.review = Some(reviewed(at));
	});
	let row = w.ctx.status.day_thread(&t);
	assert_eq!(row.line, "done and reviewed: 2 notes, tests pass");
	assert_eq!(row.reviewed_at, Some(at));
}

#[tokio::test]
async fn a_thread_row_has_no_session_figures_before_the_sessions_were_read() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	with_session(&t, &session(&w, "cold", &[("Said.", 0.5)]));
	let row = w.ctx.status.day_thread(&t);
	assert_eq!((row.line.as_str(), row.cost, row.context), ("", None, None));
}

// ---- the thread's ledger and task file ----

#[tokio::test]
async fn a_thread_s_ledger_covers_every_session_since_its_task_began_and_is_read_again_only_when_a_file_changed() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	task_of(&w, &t);
	assert!(w.ctx.status.ledger_of(&t, false).is_none());
	session(&w, "planning", &[("Planning.", 0.25)]);
	let executing = session(&w, "executing", &[("Added the line.", 0.5)]);
	with_session(&t, &executing);
	let ledger = w.ctx.status.ledger_of(&t, false).unwrap();
	// Both sessions' spend: the plan's $0.25 and the work's $0.50.
	assert_eq!((ledger.spend, ledger.requests), (0.75, 2));
	// Looked at most every few seconds, and the files' sizes decide whether to parse again (SVC-8).
	let mut text = fs::read_to_string(&executing).unwrap();
	text.push_str(&format!(
		"{}\n",
		json!({"type": "message", "id": "x9", "message": {"role": "assistant", "provider": "anthropic",
			"model": "claude-opus-5-5", "timestamp": now(), "usage": {"cost": {"total": 1.0}}, "content": []}})
	));
	fs::write(&executing, text).unwrap();
	assert_eq!(w.ctx.status.ledger_of(&t, false).unwrap().spend, 0.75);
	assert_eq!(w.ctx.status.ledger_of(&t, true).unwrap().spend, 1.75);
	// A thread's own session alone, without a task: the files of the folder are not all its.
	t.with(|t| t.task = None);
	w.ctx.status.forget(&t);
	assert_eq!(w.ctx.status.ledger_of(&t, true).unwrap().spend, 1.5);
}

fn task_md(t: &ThreadRef) -> String {
	let (project_id, id) = t.with(|t| {
		let task = t.task.as_ref().unwrap();
		(task.project_id.clone(), task.id.clone())
	});
	fs::read_to_string(task_dir(&project_id, &id).join("task.md")).unwrap()
}

#[tokio::test]
async fn write_task_brings_the_task_file_up_to_date_with_its_session_state_and_spend_over_all_its_sessions() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	let record = task_of(&w, &t);
	w.ctx.status.write_task(&t);
	let text = task_md(&t);
	let head = format!("- Thread: {} · session — · parent: — · base abc1234\n- State: idle · spend —\n", record.id);
	assert!(text.contains(&head), "{text}");
	// Spend follows once warmed: the plan's $0.25 and the work's $0.50.
	w.ctx.set_warmed();
	session(&w, "planning", &[("Planning.", 0.25)]);
	let executing = session(&w, "executing", &[("Added the line.", 0.5)]);
	with_session(&t, &executing);
	t.with(|t| t.state = AgentState::Working);
	w.ctx.status.write_task(&t);
	let text = task_md(&t);
	assert!(text.contains("- State: working · spend $0.75\n"), "{text}");
	assert!(text.contains(&format!("session {}", executing.file_stem().unwrap().to_string_lossy())), "{text}");
	// Reviewed work reads "done and reviewed".
	t.with(|t| {
		t.shown = ThreadState::Done;
		t.review = Some(reviewed(now()));
	});
	w.ctx.status.write_task(&t);
	assert!(task_md(&t).contains("- State: done and reviewed · spend $0.75\n"));
	let on_disk = fs::read_to_string(task_dir(&record.project_id, &record.id).join("thread.json")).unwrap();
	assert!(on_disk.contains("\"state\": \"done and reviewed\""), "{on_disk}");
}

#[tokio::test]
async fn write_task_leaves_a_thread_that_is_no_longer_open_and_one_without_a_task_alone() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	// No task file: nothing to write.
	w.ctx.status.write_task(&t);
	let record = task_of(&w, &t);
	w.ctx.threads.drop_pane("1-1");
	w.ctx.status.write_task(&t);
	assert!(!task_dir(&record.project_id, &record.id).join("task.md").exists());
}

// ---- overlaps, leftovers and projects (real git) ----

fn git(dir: &Path, args: &[&str]) -> String {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	crate::shared::env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
	String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The watched root and the demo repo in it; the catalog synced; the project as the catalog lists it.
async fn demo_repo(w: &World) -> Project {
	let root = real_path(w.j.root());
	let main = root.join("demo");
	fs::create_dir_all(&main).unwrap();
	git(&main, &["init", "-q", "-b", "main"]);
	fs::write(main.join("README.md"), "demo\n").unwrap();
	git(&main, &["add", "."]);
	git(&main, &["commit", "-q", "-m", "init demo"]);
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let text = format!(
		"version: 1\nroots:\n  - path: {}\n    depth: 2\n    ignore: [\"*.wt\"]\n",
		root.display().to_string().replace('\\', "/")
	);
	fs::write(file, text).unwrap();
	w.ctx.places.sync().await;
	let listed = list_projects(&w.ctx.catalog.lock().unwrap(), None).unwrap();
	listed.into_iter().find(|p| p.name == "demo").expect("the demo project")
}

/// A thread in a new worktree of `project`.
fn thread_in(w: &World, project: &Project, pane: &str, title: &str) -> (ThreadRef, PathBuf) {
	let slug = title.to_lowercase().replace(' ', "-");
	let path = real_path(&project.main_path).parent().unwrap().join("demo.wt").join(&slug);
	git(&project.main_path, &["worktree", "add", "-q", "-b", &format!("feature/{slug}"), &path.to_string_lossy()]);
	let mut t = Thread::new(pane, "w-1", project.clone(), title, format!("feature/{slug}"));
	t.path = path.clone();
	t.state = AgentState::Idle;
	let t = crate::frontier::server::context::Shared::new(t);
	w.ctx.threads.add(t.clone());
	(t, path)
}

#[tokio::test]
async fn thread_status_names_open_threads_touching_the_same_files_and_projects_lists_leftover_worktrees_0456() {
	let w = world(|_| {});
	let project = demo_repo(&w).await;
	let (_, first) = thread_in(&w, &project, "1-1", "Readme line");
	let (_, second) = thread_in(&w, &project, "2-1", "Second");
	fs::write(first.join("README.md"), "demo\nOne.\n").unwrap();
	git(&first, &["commit", "-qam", "One"]);
	fs::write(second.join("README.md"), "demo\nTwo.\n").unwrap();
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(status.contains("demo/Readme line [feature/readme-line] idle · overlaps Second: README.md"), "{status}");
	assert!(status.contains("demo/Second [feature/second] idle · overlaps Readme line: README.md"), "{status}");
	// The overlaps `today` shows are the ones read last, and a read is started in the background.
	let row = w.ctx.status.day_thread(&w.ctx.threads.get("1-1").unwrap());
	assert_eq!(row.overlaps, Some(vec!["Second".to_owned()]));
	assert!(w.ctx.status.overlaps(&w.ctx.threads.get("1-1").unwrap(), &FileReads::default()).await.unwrap().len() == 1);

	// The second thread is closed in herdr: its worktree is left on disk, with its change.
	w.ctx.threads.drop_pane("2-1");
	let left = w.ctx.status.leftovers().await.unwrap();
	assert_eq!(left.len(), 1);
	assert_eq!(
		(left[0].project.as_str(), left[0].branch.as_deref(), left[0].ahead, left[0].dirty),
		("demo", Some("feature/second"), 0, 1)
	);
	assert_eq!(path_key(&left[0].folder), path_key(&second));
	let text = w.ctx.status.projects_text().await.unwrap();
	let header = format!("demo ({}): {}", project.id, project.main_path.display());
	assert!(text.starts_with(&header), "{text}");
	assert!(text.contains("\n  demo/Readme line [feature/readme-line] idle\n"), "{text}");
	assert!(
		text.contains(&format!("  left on disk: {} [feature/second] 0 ahead, 1 changed", left[0].folder.display())),
		"{text}"
	);
	// Clean, it is said so; a closed thread's reads are forgotten.
	git(&second, &["checkout", "--", "README.md"]);
	let text = w.ctx.status.projects_text().await.unwrap();
	assert!(text.contains("[feature/second] 0 ahead, clean"), "{text}");
}

#[tokio::test]
async fn thread_status_reads_the_files_again_rather_than_joining_a_read_the_tick_began_before_a_change_0456() {
	use futures_util::FutureExt;
	let w = world(|_| {});
	let project = demo_repo(&w).await;
	let (_, first) = thread_in(&w, &project, "1-1", "Readme line");
	let (_, second) = thread_in(&w, &project, "2-1", "Second");
	// The refresh tick's reads, begun while neither thread had changed a file, are still running.
	for pane in ["1-1", "2-1"] {
		let before: BoxFuture<'static, Result<Vec<String>, String>> = async { Ok(Vec::new()) }.boxed();
		w.ctx.status.st().files_reading.insert(pane.to_owned(), (0, before.shared()));
	}
	fs::write(first.join("README.md"), "demo\nOne.\n").unwrap();
	fs::write(second.join("README.md"), "demo\nTwo.\n").unwrap();
	let status = w.ctx.status.thread_status(None, None).await.unwrap();
	assert!(status.contains("demo/Readme line [feature/readme-line] idle · overlaps Second: README.md"), "{status}");
}

#[tokio::test]
async fn projects_say_when_there_are_none_and_a_thread_s_files_are_read_once_for_the_whole_answer() {
	let w = world(|_| {});
	assert_eq!(
		w.ctx.status.projects_text().await.unwrap(),
		"No projects: add folders to the watched roots in projects.yaml."
	);
	let project = demo_repo(&w).await;
	let (t, _) = thread_in(&w, &project, "1-1", "Readme line");
	let reads = FileReads::default();
	let target = Target::of(&t);
	let one = w.ctx.status.read_files(&target);
	let two = w.ctx.status.read_files(&target);
	assert_eq!(one.await, two.await);
	assert!(w.ctx.status.st().files_reading.is_empty());
	// A call doesn't join a read that began before it: it reads again, and that read is the one later callers join.
	let before = w.ctx.status.read_files(&target);
	let fresh = w.ctx.status.read_files_fresh(&target);
	let joined = w.ctx.status.read_files(&target);
	assert!(!before.ptr_eq(&fresh) && fresh.ptr_eq(&joined));
	assert_eq!(before.await, joined.await);
	assert!(w.ctx.status.st().files_reading.is_empty());
	assert!(w.ctx.status.overlaps(&t, &reads).await.unwrap().is_empty());
	assert_eq!(reads.map().len(), 1);
}

#[tokio::test]
async fn forget_drops_what_was_read_of_a_thread() {
	let w = world(|_| {});
	let t = thread(&w, "1-1", "Readme line");
	task_of(&w, &t);
	with_session(&t, &session(&w, "kept", &[("Said.", 0.5)]));
	assert!(w.ctx.status.ledger_of(&t, false).is_some());
	w.ctx.status.st().changed_files.insert("1-1".to_owned(), (now(), 1, vec!["a".to_owned()]));
	w.ctx.status.forget(&t);
	let st = w.ctx.status.st();
	assert!(st.ledgers.is_empty() && st.changed_files.is_empty());
}
