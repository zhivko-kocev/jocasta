//! The test context every part's tests build their world from (`test_context`), and its own cases.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;

use super::*;
use crate::frontier::protocol::{Leftover, ServerMessage, ServiceHealth, ToastLevel, ToastPriority};
use crate::frontier::server::day::{AlertDetails, DayHooks, DayOptions, DayThread};
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::thread::{Thread, ThreadRef};
use crate::frontier::server::worklog::WorklogSession;
use crate::shared::errors::Result;
use crate::shared::projects::Project;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// The day's hooks in a test context: it says nothing, reads nothing and has no projects, threads or sessions.
struct NoHooks;

impl DayHooks for NoHooks {
	fn send(&self, _: ServerMessage) {}

	fn alert(&self, _: &str, _: AlertDetails) {}

	fn notify(&self, _: &str, _: ToastLevel) {}

	fn projects(&self) -> Vec<Project> {
		Vec::new()
	}

	fn threads(&self) -> Vec<DayThread> {
		Vec::new()
	}

	fn sessions_by_project(&self, _: &str) -> HashMap<String, Vec<WorklogSession>> {
		HashMap::new()
	}

	fn prepare_sessions(&self, _: &str) -> BoxFuture<'_, Result<()>> {
		Box::pin(async { Ok(()) })
	}

	fn services(&self) -> BoxFuture<'_, Result<Vec<ServiceHealth>>> {
		Box::pin(async { Ok(Vec::new()) })
	}

	fn project_name(&self, _: &Path) -> Option<String> {
		None
	}

	fn leftovers(&self) -> BoxFuture<'_, Result<Vec<Leftover>>> {
		Box::pin(async { Ok(Vec::new()) })
	}

	fn weekly_review(&self, _: &Project) -> BoxFuture<'_, Result<Option<String>>> {
		Box::pin(async { Ok(None) })
	}
}

/// A context over a temp Jocasta (its data dir, catalog and sessions folder), a [`FakeHerdr`], a day with no pollers and
/// every part. Hold it for the test's whole length: it keeps the temp roots and the environment lock.
pub(crate) struct TestContext {
	pub ctx: Arc<ServiceContext>,
	pub herdr: Arc<FakeHerdr>,
	/// Every message the service broadcast to its clients, in order.
	pub sent: Arc<Mutex<Vec<ServerMessage>>>,
	_jocasta: TempJocasta,
}

impl TestContext {
	/// Makes a boss pane `boss-1` in `state`, as `ensureBoss` leaves it.
	pub fn with_boss(&self, state: AgentState) -> BossRef {
		let boss = Shared::new(BossPane {
			pane_id: "boss-1".to_owned(),
			workspace_id: "w-boss".to_owned(),
			state,
			session_path: None,
			event_at: None,
			prompted_at: None,
		});
		self.ctx.set_boss(Some(boss.clone()));
		boss
	}

	/// Registers a thread `title` of project `demo` in pane `pane_id`, in the task phase, and returns it.
	pub fn with_thread(&self, pane_id: &str, title: &str) -> ThreadRef {
		let project = Project {
			id: "p-demo".to_owned(),
			identity: String::new(),
			name: "demo".to_owned(),
			main_path: "/code/demo".into(),
			git: true,
			paths: Vec::new(),
		};
		let mut thread = Thread::new(pane_id, "w-1", project, title, format!("feature/{pane_id}"));
		thread.path = "/wt/x".into();
		let thread = Shared::new(thread);
		self.ctx.threads.add(thread.clone());
		thread
	}
}

/// Builds a [`TestContext`] from earlier waves' fakes, so no part's test wires one itself. `options` may change the
/// options before the context is built (a seam, a confirm); the herdr, the broadcast and the sessions folder are
/// the test context's own. Call it inside a tokio test (`#[tokio::test]`): the check runner needs a runtime.
pub(crate) fn test_context(options: impl FnOnce(&mut ServiceOptions)) -> TestContext {
	let jocasta = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	let sent = Arc::new(Mutex::new(Vec::new()));
	let broadcast = Arc::clone(&sent);
	let mut service = ServiceOptions::new(
		herdr.clone(),
		Arc::new(move |message| broadcast.lock().unwrap_or_else(PoisonError::into_inner).push(message)),
	);
	service.sessions_dir = Some(jocasta.root().join("sessions"));
	// Never his disk: a nearly full one raises an urgent line for the boss (as `with_test_defaults`).
	service.day = Some(DayOptions { pollers: Some(false), disk_free: Some(Arc::new(|| 0.5)), ..DayOptions::default() });
	// Never his omp's path: panes run `omp` by name, as where none is on PATH (`pane_omp_program` names the one found).
	service.omp = Some(vec!["omp".to_owned()]);
	// Prompts and control go to a recording bridge that reports them to the fake herdr (0675).
	service.bridge = Some(herdr.recording_bridge());
	options(&mut service);
	let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog().expect("the catalog")));
	let link = Link::new();
	let day = Day::new(catalog.clone(), Arc::new(NoHooks), service.day.take().unwrap_or_default()).expect("the day");
	let ctx = ServiceContext::new(&link, catalog, service, day);
	TestContext { ctx, herdr, sent, _jocasta: jocasta }
}

fn assert_shared_across_tasks<T: Send + Sync>() {}

#[test]
fn the_context_can_be_shared_across_tokio_tasks() {
	assert_shared_across_tasks::<ServiceContext>();
}

#[tokio::test]
async fn a_test_context_builds_and_its_parts_reach_each_other_through_it() {
	let t = test_context(|_| {});
	t.with_boss(AgentState::Idle);
	let thread = t.with_thread("1-1", "Readme line");
	assert!(t.ctx.threads.current(&thread));
	assert!(!t.ctx.stopping() && !t.ctx.warmed());
	// A part reaches a sibling through the context: events asks the context for the boss and herdr.
	t.ctx.events.tell("[event] hello", ToastPriority::Urgent, None).await;
	assert_eq!(t.herdr.of("prompt"), [vec![serde_json::json!("boss-1"), serde_json::json!("[event] hello")]]);
	t.ctx.set_stopping();
	assert!(t.ctx.stopping());
}

#[tokio::test]
async fn calls_in_flight_are_waited_for_until_they_end_or_are_dropped() {
	let calls = Calls::default();
	assert!(calls.settled_within(10).await);
	let (release, released) = tokio::sync::oneshot::channel::<()>();
	let guard = calls.enter();
	calls.spawn(async move {
		let _ = released.await;
	});
	assert_eq!(calls.len(), 2);
	assert!(!calls.settled_within(20).await);
	drop(guard);
	release.send(()).unwrap();
	assert!(calls.settled_within(1_000).await);
	assert!(calls.is_empty());
	assert_eq!(calls.run(async { 7 }).await, 7);
}

#[tokio::test]
async fn finishing_boss_and_warned_state_live_on_the_context() {
	let t = test_context(|_| {});
	assert_eq!(t.ctx.finishing("w1"), None);
	t.ctx.set_finishing("w1", Finishing::Removing);
	assert_eq!(t.ctx.finishing("w1"), Some(Finishing::Removing));
	t.ctx.set_finishing("w1", Finishing::Removed);
	assert_eq!(t.ctx.finishing("w1"), Some(Finishing::Removed));
	t.ctx.clear_finishing("w1");
	assert_eq!(t.ctx.finishing("w1"), None);
	assert!(t.ctx.boss().is_none());
	let boss = t.with_boss(AgentState::Working);
	assert!(t.ctx.boss().is_some_and(|found| found.same(&boss)));
	t.ctx.set_warmed();
	assert!(t.ctx.warmed());
}
