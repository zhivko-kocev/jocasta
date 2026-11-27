use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::frontier::server::context::test_context;
use crate::frontier::server::surface::NotificationResult;

const URGENT: ToastPriority = ToastPriority::Urgent;
const NORMAL: ToastPriority = ToastPriority::Normal;

fn prompts(t: &crate::frontier::server::context::TestContext) -> Vec<String> {
	t.herdr.of("prompt").iter().map(|args| args[1].as_str().unwrap().to_owned()).collect()
}

fn notifications() -> Vec<Value> {
	let text = std::fs::read_to_string(data_dir().join("notifications.jsonl")).unwrap_or_default();
	text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

#[test]
fn clean_removes_control_characters_from_every_string_but_tab_and_newline() {
	let message = ServerMessage::Toast {
		reason: ToastReason::Event,
		level: ToastLevel::Info,
		text: "a\u{1b}[31mred\u{7}\tb\nc\u{7f}d\u{85}e".to_owned(),
		pane_id: Some("p\u{1b}1".to_owned()),
		priority: None,
		url: None,
	};
	let ServerMessage::Toast { text, pane_id, .. } = clean(&message).unwrap() else { panic!("a toast stays a toast") };
	assert_eq!(text, "a[31mred\tb\ncde");
	assert_eq!(pane_id.as_deref(), Some("p1"));
}

#[tokio::test]
async fn an_event_line_goes_into_an_idle_boss_at_once_and_makes_it_busy_so_the_next_waits() {
	let t = test_context(|_| {});
	let boss = t.with_boss(AgentState::Idle);
	t.ctx.events.tell("[event] one", NORMAL, None).await;
	assert_eq!(prompts(&t), ["[event] one"]);
	assert_eq!(boss.with(|b| b.state), AgentState::Working);
	assert!(boss.with(|b| b.prompted_at).is_some());
	t.ctx.events.tell("[event] two", NORMAL, None).await;
	t.ctx.events.tell("[event] three", NORMAL, None).await;
	assert_eq!(prompts(&t), ["[event] one"]);
	// herdr's next idle sends what waits as one prompt.
	t.ctx.events.boss_changed(AgentState::Idle).await;
	assert_eq!(prompts(&t), ["[event] one", "[event] two\n[event] three"]);
}

#[tokio::test]
async fn an_urgent_line_goes_in_while_the_boss_works_and_a_normal_one_waits_and_drops_what_no_longer_holds() {
	let t = test_context(|_| {});
	let boss = t.with_boss(AgentState::Working);
	let still = Arc::new(AtomicBool::new(true));
	let holds: Holds = {
		let still = Arc::clone(&still);
		Arc::new(move || still.load(Ordering::SeqCst))
	};
	t.ctx.events.tell("[event] plan ready", NORMAL, Some(holds)).await;
	t.ctx.events.tell("[event] kept", NORMAL, None).await;
	assert!(prompts(&t).is_empty());
	t.ctx.events.tell("[event] urgent", URGENT, None).await;
	assert_eq!(prompts(&t), ["[event] urgent"]);
	// The plan was approved meanwhile: its line isn't said as waiting (P8).
	still.store(false, Ordering::SeqCst);
	boss.with(|b| b.state = AgentState::Working);
	t.ctx.events.boss_changed(AgentState::Done).await;
	assert_eq!(prompts(&t), ["[event] urgent", "[event] kept"]);
}

#[tokio::test]
async fn without_a_boss_lines_wait_and_the_boss_state_reads_unknown_and_done_as_idle() {
	let t = test_context(|_| {});
	assert_eq!(t.ctx.events.boss_state(), AgentState::Unknown);
	t.ctx.events.tell("[event] early", URGENT, None).await;
	assert!(prompts(&t).is_empty());
	let boss = t.with_boss(AgentState::Done);
	assert_eq!(t.ctx.events.boss_state(), AgentState::Idle);
	boss.with(|b| b.state = AgentState::Working);
	t.ctx.events.boss_changed(AgentState::Idle).await;
	assert_eq!(prompts(&t), ["[event] early"]);
}

#[tokio::test]
async fn a_thread_line_names_its_project_and_title() {
	let t = test_context(|_| {});
	t.with_boss(AgentState::Idle);
	let thread = t.with_thread("1-1", "Readme line");
	t.ctx.events.thread(&thread, "finished its turn", NORMAL, None).await;
	assert_eq!(prompts(&t), ["[event] demo/Readme line — finished its turn"]);
}

#[tokio::test]
async fn an_alert_flattens_whitespace_quotes_the_outside_parts_and_adds_only_a_plain_web_link() {
	let t = test_context(|_| {});
	t.with_boss(AgentState::Idle);
	let details = |priority, url: Option<&str>, outside: &[&str]| AlertDetails {
		priority,
		url: url.map(str::to_owned),
		outside: outside.iter().map(|part| (*part).to_owned()).collect(),
	};
	t.ctx
		.events
		.alert("Pipeline red —\n web on\tmain", &details(URGENT, Some("https://example.test/run/1"), &["web on main"]));
	t.ctx.events.settled().await;
	t.ctx.events.alert("Meeting in 5 min — Standup", &details(URGENT, Some("javascript:alert(1)"), &["Standup"]));
	t.ctx.events.settled().await;
	assert_eq!(
		prompts(&t),
		["[event] Pipeline red — «web on main» · https://example.test/run/1", "[event] Meeting in 5 min — «Standup»"]
	);
}

#[tokio::test]
async fn a_toast_is_broadcast_cleaned_and_raised_through_herdr_with_a_title_a_body_and_a_sound() {
	let t = test_context(|_| {});
	let mut toast = Toast::new(ToastReason::Blocked, ToastLevel::Warning, "demo/Readme line — Run \u{1b}rm -rf now?");
	toast.pane_id = Some("1-1".to_owned());
	t.ctx.events.send(toast.into());
	t.ctx.events.send(Toast::new(ToastReason::Done, ToastLevel::Info, "no separator here").into());
	t.ctx.events.settled().await;
	let sent = t.sent.lock().unwrap().clone();
	let ServerMessage::Toast { text, .. } = &sent[0] else { panic!("a toast") };
	assert_eq!(text, "demo/Readme line — Run rm -rf now?");
	let calls = t.herdr.of("notify");
	assert_eq!(calls[0][0], json!({"title": "demo/Readme line", "sound": "request", "body": "Run rm -rf now?"}));
	assert_eq!(calls[1][0], json!({"title": "Jocasta", "sound": "none", "body": "no separator here"}));
	let lines = notifications();
	assert_eq!(lines[0]["paneId"], "1-1");
	assert_eq!(lines[0]["reason"], "blocked");
	assert_eq!(lines[0]["result"], "shown");
	assert_eq!(lines[1]["paneId"], Value::Null);
	assert_eq!(lines[1]["title"], "Jocasta");
	assert_eq!(
		lines[0].as_object().unwrap().keys().collect::<Vec<_>>(),
		["at", "paneId", "reason", "title", "body", "result"]
	);
}

#[tokio::test]
async fn a_notification_is_cut_to_herdrs_limits_and_a_repeat_is_logged_not_shown() {
	let t = test_context(|_| {});
	let long = format!("{} — {}", "t".repeat(100), "b".repeat(300));
	t.ctx.events.notify(&Toast::new(ToastReason::Event, ToastLevel::Info, long), true).await;
	t.ctx.events.notify(&Toast::new(ToastReason::Event, ToastLevel::Info, "again"), false).await;
	let calls = t.herdr.of("notify");
	assert_eq!(calls.len(), 1);
	assert_eq!(calls[0][0]["title"].as_str().unwrap().chars().count(), 80);
	assert_eq!(calls[0][0]["body"].as_str().unwrap().chars().count(), 240);
	assert_eq!(notifications()[1]["result"], "repeat");
}

#[tokio::test]
async fn the_starts_unshown_notifications_wait_for_a_client_and_are_sent_once_when_one_attaches() {
	let t = test_context(|_| {});
	t.herdr.state().notify_result = NotificationResult::NoForegroundClient;
	t.ctx.events.warning("settings: a problem");
	t.ctx.events.settled().await;
	assert_eq!(t.herdr.of("notify").len(), 1);
	// No client yet: the tick probes with the first one and they all wait.
	t.ctx.events.resend_unshown();
	t.ctx.events.settled().await;
	assert_eq!(t.herdr.of("notify").len(), 2);
	// A client attached: each is sent once, whatever herdr answers, and no more after.
	t.herdr.state().notify_result = NotificationResult::Shown;
	t.ctx.events.resend_unshown();
	t.ctx.events.settled().await;
	assert_eq!(t.herdr.of("notify").len(), 3);
	t.ctx.events.resend_unshown();
	t.ctx.events.settled().await;
	assert_eq!(t.herdr.of("notify").len(), 3);
	// The start is over: a later one without a client is dropped, not kept.
	t.herdr.state().notify_result = NotificationResult::NoForegroundClient;
	t.ctx.events.warning("later");
	t.ctx.events.settled().await;
	t.ctx.events.resend_unshown();
	t.ctx.events.settled().await;
	assert_eq!(t.herdr.of("notify").len(), 4);
}

#[tokio::test]
async fn a_warning_is_a_log_line_and_a_warning_toast() {
	let t = test_context(|_| {});
	t.ctx.events.warning_from("herdr is old", "herdr");
	t.ctx.events.settled().await;
	let sent = t.sent.lock().unwrap().clone();
	assert_eq!(
		sent,
		[ServerMessage::Toast {
			reason: ToastReason::Event,
			level: ToastLevel::Warning,
			text: "herdr is old".to_owned(),
			pane_id: None,
			priority: None,
			url: None,
		}]
	);
}

/// The page server's stand-in: links by id, and the ids opened.
struct OpenedBriefs(std::sync::Mutex<Vec<String>>);

impl crate::frontier::brief::BriefLinks for OpenedBriefs {
	fn url(&self, id: &str) -> String {
		format!("http://127.0.0.1:1/brief/{id}")
	}
	fn open(&self, id: &str) {
		self.0.lock().unwrap().push(id.to_owned());
	}
	fn open_board(&self) {
		self.0.lock().unwrap().push("board".to_owned());
	}
}

#[tokio::test]
async fn a_question_is_raised_with_its_sound_and_logged_as_blocked_0699() {
	let t = test_context(|_| {});
	let question = Toast::new(ToastReason::Blocked, ToastLevel::Warning, "Jocasta asks — Allow?");
	t.ctx.events.notify(&question, true).await;
	assert_eq!(t.herdr.of("notify")[0][0], json!({"title": "Jocasta asks", "sound": "request", "body": "Allow?"}));
	let line = notifications().pop().unwrap();
	assert_eq!(line["reason"], "blocked");
	assert_eq!(line["title"], "Jocasta asks");
	assert_eq!(line["body"], "Allow?");
	assert_eq!(line["result"], "shown");
}

#[tokio::test]
async fn a_done_notification_links_its_brief_and_a_click_opens_it_0699() {
	let links = Arc::new(OpenedBriefs(std::sync::Mutex::new(Vec::new())));
	let page: Arc<dyn crate::frontier::brief::BriefLinks> = links.clone();
	let t = test_context(move |options| options.briefs = Some(Arc::clone(&page)));
	let brief = crate::frontier::brief::model::Brief {
		id: "thread-t1".to_owned(),
		kind: "thread".to_owned(),
		title: "t".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 0,
		sections: Vec::new(),
	};
	crate::frontier::brief::store::save(&brief).unwrap();
	let url = "http://127.0.0.1:1/brief/thread-t1".to_owned();
	let mut done = Toast::new(ToastReason::Done, ToastLevel::Info, "demo/t — done and reviewed");
	done.url = Some(url.clone());
	let mut event = Toast::new(ToastReason::Event, ToastLevel::Info, "demo/t — a link that isn't a Done");
	event.url = Some(url.clone());
	let note = |toast: &Toast| t.ctx.events.brief_link(toast);
	let link = note(&done).expect("a Done toast with its brief links it");
	assert_eq!(link.url, url);
	assert!(note(&event).is_none());
	(link.open)();
	assert_eq!(*links.0.lock().unwrap(), ["thread-t1"]);
	t.ctx.events.notify(&done, true).await;
	assert_eq!(t.herdr.of("notify")[0][0]["link"], json!(url));
}
