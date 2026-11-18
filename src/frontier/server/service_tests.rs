//! Direct tests of `JocastaService`: the start and the stop, the timers and the stop's safety, the calls it counts and
//! refuses once closing, and the state feed. The TypeScript's other service tests (`service-*.test.ts`,
//! `server.test.ts`, `shutdown.test.ts`, `production.test.ts`, …) go through `startTestServer` and HTTP; they wait for
//! `server.rs` (wave 11). `service-phases.test.ts` tests `setPhase` alone (`thread_tests.rs`) and `day-stop.test.ts`
//! the day alone (`day_stop_tests.rs`): neither has a service-level part.

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::frontier::protocol::ToastPriority;
use crate::frontier::server::herdr::WorkspaceInfo;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::shared::layer::HOSTS_ENV;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

const BOSS_KEY: &str = "boss-key-of-this-run";

/// A started-or-not service over a fake herdr and a temp Jocasta, with the clocks and the spawns of a test: no weekly
/// reviews, no mise, no prompt file, loopback relays, a refresh tick and a watcher scan that never come by themselves.
struct Stand {
	service: Arc<JocastaService>,
	herdr: Arc<FakeHerdr>,
	sent: Arc<Mutex<Vec<ServerMessage>>>,
	_jocasta: TempJocasta,
}

fn stand(change: impl FnOnce(&mut TempJocasta, &mut ServiceOptions)) -> Stand {
	let mut jocasta = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	let sent = Arc::new(Mutex::new(Vec::new()));
	let broadcast = Arc::clone(&sent);
	let mut options = ServiceOptions::new(
		herdr.clone(),
		Arc::new(move |message| broadcast.lock().unwrap_or_else(PoisonError::into_inner).push(message)),
	);
	options.sessions_dir = Some(jocasta.root().join("sessions"));
	options.day = Some(DayOptions { pollers: Some(false), ..DayOptions::default() });
	options.omp = Some(vec!["omp".to_owned()]);
	options.pane_prompt = Some(None);
	options.mise = Some(None);
	options.hunk = Some(None);
	options.weekly_reviews = Some(false);
	options.boss_key = Some(BOSS_KEY.to_owned());
	options.relay_url_of = Some(Arc::new(|_, _, _| async { Ok("ws://127.0.0.1:47499".to_owned()) }.boxed()));
	options.refresh_ms = Some(3_600_000);
	options.host_poll_ms = Some(3_600_000);
	options.watcher_interval_ms = Some(3_600_000);
	change(&mut jocasta, &mut options);
	let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog().expect("the catalog")));
	let service = JocastaService::new(catalog, options).expect("the service");
	Stand { service, herdr, sent, _jocasta: jocasta }
}

fn caller() -> Caller {
	Caller { pane_key: None, boss_key: Some(BOSS_KEY.to_owned()) }
}

async fn within_a_second<T>(work: impl Future<Output = T>) -> T {
	tokio::time::timeout(Duration::from_secs(10), work).await.expect("done in time")
}

#[tokio::test]
async fn start_ensures_the_boss_pane_finds_the_threads_and_marks_them_discovered() {
	let s = stand(|_, _| {});
	assert!(s.service.context().boss().is_none());
	within_a_second(s.service.start()).await.unwrap();
	within_a_second(s.service.discovered()).await;
	let boss = s.service.context().boss().expect("the boss pane");
	assert_eq!(boss.with(|boss| boss.pane_id.clone()), s.herdr.state().pane_list[0].pane_id);
	assert!(s.service.context().warmed());
	assert_eq!(s.service.busy_threads().unwrap().working, 0);
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn a_second_start_gets_the_first_ones_outcome_and_starts_nothing_again() {
	let s = stand(|_, _| {});
	let first = s.service.start();
	let second = s.service.start();
	within_a_second(first).await.unwrap();
	within_a_second(second).await.unwrap();
	within_a_second(s.service.start()).await.unwrap();
	// One boss workspace was made, and one pair of timers runs.
	assert_eq!(s.herdr.of("ensureWorkspace").len(), 1);
	assert_eq!(s.service.timer_count(), 2);
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn a_start_that_fails_still_marks_the_threads_discovered_and_the_same_error_comes_to_every_caller() {
	// The boss pane's omp would host its session on a public relay: it doesn't start (0536, 0561).
	let s = stand(|_, options| {
		options.relay_url_of = Some(Arc::new(|_, _, _| async { Ok("wss://relay.example.org".to_owned()) }.boxed()));
	});
	let first = within_a_second(s.service.start()).await.unwrap_err();
	let second = within_a_second(s.service.start()).await.unwrap_err();
	assert_eq!(first.to_string(), second.to_string());
	within_a_second(s.service.discovered()).await;
	assert!(s.service.context().boss().is_none());
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn a_stop_during_the_start_ends_it_and_discovered_resolves() {
	let s = stand(|_, _| {});
	let starting = s.service.start();
	within_a_second(s.service.stop()).await;
	let _ = within_a_second(starting).await;
	within_a_second(s.service.discovered()).await;
	assert!(s.service.context().stopping());
}

#[tokio::test]
async fn stop_is_safe_to_call_twice_and_at_once() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let (a, b) = tokio::join!(s.service.stop(), s.service.stop());
	let _ = (a, b);
	within_a_second(s.service.stop()).await;
	assert!(s.service.context().stopping());
}

#[tokio::test]
async fn stop_without_a_start_closes_the_service() {
	let s = stand(|_, _| {});
	within_a_second(s.service.stop()).await;
	assert!(s.service.context().stopping());
	within_a_second(s.service.discovered()).await;
}

#[tokio::test]
async fn stop_waits_for_the_calls_in_flight_and_the_refreshers_running_tick() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let in_flight = s.service.context().calls.enter();
	let service = Arc::clone(&s.service);
	let stopping = tokio::spawn(async move { service.stop().await });
	tokio::time::sleep(Duration::from_millis(150)).await;
	assert!(!stopping.is_finished(), "the stop waits for the call in flight");
	// Refused meanwhile: the service is closing.
	let reply = s.service.call_tool("thread_status", &Map::new(), &caller()).await;
	assert_eq!(reply, ToolReply { text: "Jocasta is closing; nothing was changed.".to_owned(), is_error: Some(true) });
	drop(in_flight);
	within_a_second(stopping).await.unwrap();
}

#[tokio::test]
async fn once_closing_every_call_is_refused_and_nothing_is_changed() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	within_a_second(s.service.stop()).await;
	let refusal = "Jocasta is closing; nothing was changed.";
	let args = Map::new();
	assert_eq!(
		s.service.call_tool("thread_status", &args, &caller()).await,
		ToolReply { text: refusal.to_owned(), is_error: Some(true) }
	);
	assert_eq!(
		s.service.cli_call("thread list", &args).await,
		ToolReply { text: refusal.to_owned(), is_error: Some(true) }
	);
	let request = PushRequest { dir: "/x".into(), remote: "origin".to_owned(), refs: Vec::new(), pane_key: None };
	assert_eq!(s.service.push_verdict(&request).await, PushAnswer { allow: false, reason: Some(refusal.to_owned()) });
	assert_eq!(s.service.pane_check("key", None).await, "");
	assert_eq!(s.service.key_stop(None).await.unwrap_err().to_string(), refusal);
	assert_eq!(s.service.key_review(None).await.unwrap_err().to_string(), refusal);
	// As the TypeScript's `Keys`: start refuses; finish finds its thread first; the read of finishable doesn't refuse.
	assert_eq!(s.service.key_start("acme-api", "brief").await.unwrap_err().to_string(), refusal);
	assert_eq!(
		s.service.key_finish("nowhere", FinishHow::Merge).await.unwrap_err().to_string(),
		"Finish: the thread closed meanwhile."
	);
	assert_ne!(s.service.key_finishable(None).await.unwrap_err().to_string(), refusal);
}

#[tokio::test]
async fn a_tool_call_answers_its_text_cleaned_and_a_refusal_as_an_error() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let ok = s.service.call_tool("thread_status", &Map::new(), &caller()).await;
	assert_eq!(ok.is_error, None);
	let unknown = s.service.call_tool("nothing", &Map::new(), &caller()).await;
	assert_eq!(unknown, ToolReply { text: "no tool nothing.".to_owned(), is_error: Some(true) });
	let not_boss = s.service.call_tool("projects", &Map::new(), &Caller::default()).await;
	assert_eq!(not_boss.is_error, Some(true));
	assert!(not_boss.text.starts_with("projects is the boss's tool"));
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_refresh_tick_runs_every_refresh_ms_until_the_stop() {
	let s = stand(|_, options| options.refresh_ms = Some(40));
	within_a_second(s.service.start()).await.unwrap();
	let before = s.service.ticks();
	let more = async {
		while s.service.ticks() < before + 2 {
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	};
	within_a_second(more).await;
	within_a_second(s.service.stop()).await;
	let after = s.service.ticks();
	tokio::time::sleep(Duration::from_millis(200)).await;
	assert_eq!(s.service.ticks(), after, "no tick after the stop");
}

#[tokio::test]
async fn a_change_herdr_reports_refreshes_its_facts_soon_while_the_full_tick_waits() {
	let changes = Arc::new(tokio::sync::Notify::new());
	let changed = Arc::clone(&changes);
	let s = stand(move |_, options| {
		options.herdr_changed = Some(Arc::new(move || {
			let changed = Arc::clone(&changed);
			async move { changed.notified().await }.boxed()
		}));
	});
	within_a_second(s.service.start()).await.unwrap();
	let ticks = s.service.ticks();
	s.herdr.state().workspace_list.push(WorkspaceInfo { id: "w-new".to_owned(), label: "new".to_owned(), cwd: None });
	changes.notify_one();
	let seen = async {
		while !s.service.context().status.workspaces().iter().any(|w| w.id == "w-new") {
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	};
	within_a_second(seen).await;
	assert_eq!(s.service.ticks(), ticks, "herdr's facts, not the full tick");
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_host_poll_runs_only_with_the_host_registry_on() {
	let on = stand(|_, _| {});
	within_a_second(on.service.start()).await.unwrap();
	assert_eq!(on.service.timer_count(), 2);
	within_a_second(on.service.stop()).await;
	assert_eq!(on.service.timer_count(), 0);
	drop(on);
	let off = stand(|jocasta, _| jocasta.set_var(HOSTS_ENV, Some("off")));
	within_a_second(off.service.start()).await.unwrap();
	assert_eq!(off.service.timer_count(), 1);
	within_a_second(off.service.stop()).await;
}

#[tokio::test]
async fn refresh_sends_the_notifications_no_client_showed_again_and_joins_the_running_tick() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let (a, b) = tokio::join!(s.service.refresh(), s.service.refresh());
	let _ = (a, b);
	assert!(s.service.context().warmed());
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_dictation_toggle_goes_to_the_boss_pane_once_it_is_open() {
	let s = stand(|_, _| {});
	let early = s.service.toggle_dictation().await.unwrap_err();
	assert_eq!(early.to_string(), "the boss pane isn't open yet");
	within_a_second(s.service.start()).await.unwrap();
	s.service.toggle_dictation().await.unwrap();
	let sent = s.herdr.of("sendKeys");
	assert_eq!(sent.len(), 1);
	assert_eq!(sent[0][1], json!(["f9"]));
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_hud_the_boss_state_and_a_notification_are_the_parts() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let hud = s.service.hud(None).unwrap();
	assert_eq!((hud.running, hud.blocked), (0, 0));
	let today = s.service.context().day.today().unwrap();
	assert_eq!(s.service.hud(Some(&today)).unwrap().running, 0);
	assert_eq!(s.service.boss_state(), s.service.context().events.boss_state());
	let mut toast = Toast::new(ToastReason::Event, ToastLevel::Info, "Readme — done");
	toast.priority = Some(ToastPriority::Normal);
	let shown = s.herdr.of("notify").len();
	s.service.notify(&toast, None).await;
	assert_eq!(s.herdr.of("notify").len(), shown + 1);
	s.service.notify(&toast, Some(false)).await;
	assert_eq!(s.herdr.of("notify").len(), shown + 1, "a repeat is logged, not shown again");
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_leftovers_the_weekly_review_and_the_new_thread_form_are_the_parts() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	assert!(s.service.leftovers().await.unwrap().is_empty());
	let form = s.service.new_thread_form(None).await.unwrap_err();
	assert_eq!(form.to_string(), "New thread: no git project to start one in; add one with `jocasta project add`.");
	let project = Project {
		id: "p-demo".to_owned(),
		identity: String::new(),
		name: "demo".to_owned(),
		main_path: s._jocasta.root().join("nowhere"),
		git: true,
		paths: Vec::new(),
	};
	// Nothing merged in a week's worth of a folder with no repository: no week to review.
	assert_eq!(s.service.weekly_review(&project).await.unwrap(), None);
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_keys_answer_with_the_parts_refusals() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let focused = "the focused pane isn't a thread's; focus a thread or pick one in today.";
	let stop = s.service.key_stop(Some("nowhere")).await.unwrap_err();
	assert!(stop.to_string().ends_with(focused), "{stop}");
	assert!(s.service.key_finishable(None).await.unwrap_err().to_string().ends_with(focused));
	assert!(s.service.key_finish("nowhere", FinishHow::Merge).await.is_err());
	assert!(s.service.key_review(None).await.unwrap_err().to_string().ends_with(focused));
	let started = s.service.key_start("nowhere", "A brief").await.unwrap_err();
	assert_eq!(started.to_string(), "no project nowhere.");
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_steps_of_stop_everything_run_on_a_started_service() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	s.service.pause_working().await.unwrap();
	s.service.cut_paused_turns().await;
	s.service.remove_ended_hosts().await;
	s.service.end_thread_leftovers().await.unwrap();
	s.service.end_hunk_daemon().await;
	assert!(!s.service.context().stopping());
	assert!(std::ptr::eq(s.service.day(), &s.service.context().day));
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_snapshot_holds_the_hud_today_and_the_dashboard() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let ServerMessage::Snapshot { hud, today, dashboard } = s.service.snapshot().unwrap() else { panic!("a snapshot") };
	assert_eq!(hud.running, 0);
	assert_eq!(today, s.service.context().day.today().unwrap());
	assert_eq!(dashboard, s.service.context().day.dashboard());
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn a_client_can_focus_a_pane_and_open_only_a_link_today_shows() {
	let s = stand(|_, _| {});
	within_a_second(s.service.start()).await.unwrap();
	let replies: Mutex<Vec<ServerMessage>> = Mutex::new(Vec::new());
	let reply = |message| replies.lock().unwrap().push(message);
	s.service.handle(ClientMessage::PaneFocus { pane_id: "p-9".to_owned() }, reply).await;
	assert_eq!(s.herdr.of("focus"), vec![vec![json!("p-9")]]);
	assert!(replies.lock().unwrap().is_empty());
	for url in ["https://example.org/not-shown", "file:///etc/passwd", "javascript:alert(1)"] {
		s.service.handle(ClientMessage::LinkOpen { url: url.to_owned() }, reply).await;
	}
	let replies = replies.into_inner().unwrap();
	assert_eq!(replies.len(), 3);
	for message in replies.iter() {
		assert_eq!(*message, ServerMessage::Error { message: "That link isn't one today shows.".to_owned() });
	}
	within_a_second(s.service.stop()).await;
}

#[tokio::test]
async fn the_options_default_to_the_services_own_and_report_their_switches() {
	let s = stand(|_, _| {});
	assert!(!s.service.context().options.weekly_reviews_on());
	assert!(s.service.context().options.ask_report_on());
	let _ = &s.sent;
}
