//! `native.rs`'s tests: the questions' one reader, the shells' boss and feed, the Agents view, his keybinds.

use std::path::Path;
use std::time::Duration;

use super::*;
use crate::frontier::protocol::{PopupAnswer, PopupView};
use crate::frontier::server::popups::{Popups, allow_question};
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::frontier::server::surface::{AgentState, Surface};
use crate::shared::testing::temp_jocasta;

/// One message the shell host was asked to send.
struct Outbound {
	kind: String,
	target: Target,
	data: String,
}

/// The server a shell reaches: real questions, and a record of the rest.
#[derive(Default)]
struct Fake {
	popups: Popups,
	keys: Mutex<Vec<(PluginAction, Option<String>)>>,
	notes: Mutex<Vec<String>>,
	handled: Mutex<Vec<ClientMessage>>,
}

impl NativeServer for Arc<Fake> {
	fn answer(&self, code: &str, answer: &Value) -> (AnswerTaken, u32) {
		(self.popups.answer(code, answer), self.popups.left(code))
	}

	fn key(&self, action: PluginAction, pane_id: Option<String>) -> BoxFuture<'static, ()> {
		locked(&self.keys).push((action, pane_id));
		Box::pin(async {})
	}

	fn snapshot(&self) -> Option<ServerMessage> {
		Some(ServerMessage::Action { text: "snapshot".to_owned(), is_error: false })
	}

	fn handle(&self, message: ClientMessage, reply: Reply) -> BoxFuture<'static, ()> {
		locked(&self.handled).push(message);
		Box::pin(async move { reply(ServerMessage::Action { text: "handled".to_owned(), is_error: false }) })
	}

	fn notify(&self, toast: Toast) -> BoxFuture<'static, ()> {
		assert_eq!(toast.reason, crate::frontier::protocol::ToastReason::Blocked, "a question is a blocked toast");
		locked(&self.notes).push(toast.text.strip_prefix("Jocasta asks — ").unwrap_or_default().to_owned());
		Box::pin(async {})
	}
}

struct Rig {
	native: Arc<Native>,
	fake: Arc<Fake>,
	sent: Arc<Mutex<Vec<Outbound>>>,
}

fn rig() -> Rig {
	let sent = Arc::new(Mutex::new(Vec::new()));
	let record = Arc::clone(&sent);
	let host = ShellHost::new(
		Arc::new(move |target, kind, data| locked(&record).push(Outbound { kind: kind.to_owned(), target, data })),
		Arc::new(tokio::sync::Notify::new()),
	);
	let native = Native::new(host, Handle::current());
	let fake = Arc::new(Fake::default());
	native.attach(Arc::new(Arc::clone(&fake)));
	Rig { native, fake, sent }
}

impl Rig {
	/// What went out of `kind`, as (target, data).
	fn sent(&self, kind: &str) -> Vec<(Target, Value)> {
		locked(&self.sent)
			.iter()
			.filter(|out| out.kind == kind)
			.map(|out| (out.target, serde_json::from_str(&out.data).unwrap_or(Value::Null)))
			.collect()
	}

	/// The codes of the questions presented so far, in order.
	fn presented(&self) -> Vec<String> {
		self.sent(SHELL_QUESTION).iter().map(|(_, data)| data["code"].as_str().unwrap_or("").to_owned()).collect()
	}

	fn answer(&self, client: u64, popup: &Popup, choice: &str, typed: Option<&str>) -> AnswerReply {
		let answer =
			PopupAnswer { choice: choice.to_owned(), typed: typed.map(str::to_owned), project: None, brief: None };
		let before = self.sent(SHELL_ANSWERED).len();
		self.native.message(client, SHELL_ANSWER, &json_of(&ShellAnswer { code: popup.code().to_owned(), answer }));
		let replies = self.sent(SHELL_ANSWERED);
		assert_eq!(replies.len(), before + 1, "one reply per answer");
		let (target, data) = replies.last().cloned().unwrap();
		assert_eq!(target, Target::Client(client), "the reply goes to the one that answered");
		serde_json::from_value::<ShellAnswered>(data).unwrap().reply
	}
}

fn coded(code: &str) -> PopupView {
	allow_question("Remove the leftover worktree acme-api/retry?", "Its branch stays.", Some(code))
}

/// Waits (up to 5 s) until `check` holds.
async fn until(mut check: impl FnMut() -> bool) {
	for _ in 0..1_000 {
		if check() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(5)).await;
	}
	panic!("the condition never held");
}

#[tokio::test]
async fn questions_go_one_at_a_time_to_the_first_shell_allow_first_and_notified_unless_his_key_opened_them() {
	let _j = temp_jocasta();
	let r = rig();
	r.native.joined(1, None);
	r.native.joined(2, None);
	let first = r.fake.popups.open(&coded("ABC"), false);
	let keyed = r.fake.popups.open(&coded("DEF"), true);
	let second = r.fake.popups.open(&coded("GHJ"), false);
	for popup in [&first, &keyed, &second] {
		r.native.show(popup);
	}
	assert_eq!(r.presented(), [first.code()]);
	assert!(r.sent(SHELL_QUESTION).iter().all(|(target, _)| *target == Target::Client(1)), "the first shell reads it");
	assert_eq!(r.answer(1, &first, "Deny", None), AnswerReply::Taken);
	// The answered one goes from every shell; the next Allow question comes ahead of the one his key opened.
	until(|| r.presented().len() == 2).await;
	assert_eq!(r.sent(SHELL_GONE), [(Target::Shells, json!(first.code()))]);
	assert_eq!(r.presented(), [first.code(), second.code()]);
	second.close(None);
	until(|| r.presented().len() == 3).await;
	assert_eq!(r.presented()[2], keyed.code());
	let message = "Remove the leftover worktree acme-api/retry?";
	assert_eq!(*locked(&r.fake.notes), [message, message], "no notification for his key's question (0581)");
}

#[tokio::test]
async fn only_its_reader_answers_a_question_and_the_reader_leaving_ends_it_as_closed() {
	let _j = temp_jocasta();
	let r = rig();
	r.native.joined(1, None);
	r.native.joined(2, None);
	let popup = r.fake.popups.open(&coded("ABC"), false);
	r.native.show(&popup);
	let refused = r.answer(2, &popup, ALLOW_CHOICE, Some("ABC"));
	assert_eq!(refused, AnswerReply::Failed { text: "This question is shown in another terminal.".to_owned() });
	assert!(popup.open(), "a foreign answer settles nothing (0578)");
	// A wrong code says the tries left; the third takes the fallback.
	assert_eq!(r.answer(1, &popup, ALLOW_CHOICE, Some("XYZ")), AnswerReply::Wrong { left: Some(2) });
	r.native.left(2);
	assert!(popup.open(), "another client leaving changes nothing");
	r.native.left(1);
	assert_eq!(popup.how().await, PopupEnd::Closed);
	assert_eq!(popup.answer().await, "Deny");
	assert_eq!(
		r.answer(1, &popup, ALLOW_CHOICE, Some("ABC")),
		AnswerReply::Failed { text: "That question is over.".to_owned() }
	);
}

const ALLOW_CHOICE: &str = "Allow";

/// A process under this one, as a `jocasta` a thread runs is under the server: it waits on its stdin until killed.
fn child() -> std::process::Child {
	let mut command = std::process::Command::new(if cfg!(windows) { "findstr" } else { "cat" });
	if cfg!(windows) {
		command.arg("x");
	}
	command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().expect("a child process")
}

#[tokio::test]
async fn a_question_waits_for_a_shell_and_a_terminal_run_inside_a_pane_never_reads_one() {
	let _j = temp_jocasta();
	let r = rig();
	let popup = r.fake.popups.open(&coded("ABC"), false);
	r.native.show(&popup);
	assert!(r.presented().is_empty(), "no shell to read it yet");
	// A thread's `jocasta` attaches first: it gets the boss and the feed, never the question.
	let mut inside = child();
	r.native.joined(4, Some(inside.id()));
	assert_eq!(
		r.sent(SHELL_FEED),
		[(Target::Client(4), json!({"type": "action", "text": "snapshot", "isError": false}))]
	);
	assert!(r.presented().is_empty(), "a terminal under the server reads no question (0578)");
	assert_eq!(
		r.answer(4, &popup, ALLOW_CHOICE, Some("ABC")),
		AnswerReply::Failed { text: "This question is shown in another terminal.".to_owned() }
	);
	assert!(popup.open());
	r.native.joined(5, None);
	assert_eq!(r.sent(SHELL_QUESTION).iter().map(|(target, _)| *target).collect::<Vec<_>>(), [Target::Client(5)]);
	assert_eq!(
		r.answer(4, &popup, ALLOW_CHOICE, Some("ABC")),
		AnswerReply::Failed { text: "This question is shown in another terminal.".to_owned() }
	);
	assert_eq!(r.answer(5, &popup, ALLOW_CHOICE, Some("abc")), AnswerReply::Taken);
	assert_eq!(popup.answer().await, ALLOW_CHOICE);
	// A shell that attaches with nothing unread gets no question.
	r.native.joined(6, None);
	assert_eq!(r.presented().len(), 1);
	let _ = inside.kill();
	let _ = inside.wait();
}
#[tokio::test]
async fn a_shells_feed_messages_reach_the_service_and_the_reply_goes_back_to_that_shell_its_keys_run() {
	let _j = temp_jocasta();
	let r = rig();
	r.native.message(3, SHELL_FEED, &json_of(&ClientMessage::PaneFocus { pane_id: "w1:p2".to_owned() }));
	until(|| !r.sent(SHELL_FEED).is_empty()).await;
	assert_eq!(*locked(&r.fake.handled), [ClientMessage::PaneFocus { pane_id: "w1:p2".to_owned() }]);
	assert_eq!(r.sent(SHELL_FEED)[0].0, Target::Client(3));
	r.native.message(
		3,
		SHELL_ACTION,
		&json_of(&ShellAction { action: PluginAction::Stop, pane_id: Some("p1".into()) }),
	);
	assert!(r.native.action("review", Some("p2")));
	assert!(r.native.action("new-thread", Some("")));
	assert!(!r.native.action("today", None), "not a thread key: herdr looks for a plugin's");
	until(|| locked(&r.fake.keys).len() == 3).await;
	assert_eq!(
		*locked(&r.fake.keys),
		[
			(PluginAction::Stop, Some("p1".to_owned())),
			(PluginAction::Review, Some("p2".to_owned())),
			(PluginAction::NewThread, None)
		]
	);
	r.native.feed(&ServerMessage::Bye);
	assert_eq!(r.sent(SHELL_FEED).last().cloned(), Some((Target::Shells, json!({"type": "bye"}))));
}

#[tokio::test]
async fn heal_tells_every_shell_the_boss_workspace_once() {
	let _j = temp_jocasta();
	let r = rig();
	let herdr = FakeSurface::new();
	let workspace =
		herdr.ensure_workspace("Jocasta", Path::new("/home"), Some(&["omp".to_owned()]), None).await.unwrap();
	let boss = herdr.state().pane_list[0].pane_id.clone();
	herdr.set_state(&boss, AgentState::Idle, None, true);
	let snapshot = herdr.snapshot().await.unwrap();
	r.native.heal(&snapshot, None).await;
	r.native.heal(&snapshot, Some("nope")).await;
	assert!(r.sent(SHELL_BOSS).is_empty());
	r.native.heal(&snapshot, Some(&boss)).await;
	r.native.heal(&snapshot, Some(&boss)).await;
	assert_eq!(r.sent(SHELL_BOSS), [(Target::Shells, json!(workspace))]);
	// A relaunched boss pane (a new pane id) is a new boss; the shells are told again only when its workspace differs.
	let mut relaunched = snapshot.clone();
	relaunched.panes.iter_mut().find(|pane| pane.pane_id == boss).unwrap().pane_id = "w9:p9".to_owned();
	r.native.heal(&relaunched, Some("w9:p9")).await;
	assert_eq!(r.sent(SHELL_BOSS).len(), 1);
	// A shell that attaches later is told too.
	r.native.joined(9, None);
	assert_eq!(r.sent(SHELL_BOSS).last().cloned(), Some((Target::Client(9), json!(workspace))));
}

#[test]
fn a_notification_body_is_cut_to_240_characters_with_an_ellipsis() {
	assert_eq!(note_body("short"), "short");
	let exact = "x".repeat(240);
	assert_eq!(note_body(&exact), exact);
	let cut = note_body(&"x".repeat(241));
	assert_eq!(cut.encode_utf16().count(), 240);
	assert!(cut.ends_with('…'));
}
