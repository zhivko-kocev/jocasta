//! Tests of `main.rs`, ported from `test/frontier/ask/main.test.ts`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::oneshot;

use super::*;
use crate::frontier::server::popups::allow_question;

fn text(lines: &[Line]) -> String {
	lines.iter().map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

type Replies = Arc<Mutex<Vec<Option<oneshot::Sender<AnswerReply>>>>>;

/// The popup's program on a fake service: the test ends the hold and answers each post when it wants.
struct Popup {
	program: AskProgram,
	ended: Arc<Mutex<Vec<i32>>>,
	replies: Replies,
	end_hold: Arc<Mutex<Option<OnEnd>>>,
}

impl Popup {
	fn new() -> Self {
		let ended = Arc::new(Mutex::new(Vec::new()));
		let replies: Replies = Arc::new(Mutex::new(Vec::new()));
		let end_hold: Arc<Mutex<Option<OnEnd>>> = Arc::new(Mutex::new(None));
		let (held, posted) = (Arc::clone(&end_hold), Arc::clone(&replies));
		let calls = AtomicUsize::new(0);
		let io = AskIo {
			hold: Box::new(move |_question, on_end| {
				*held.lock().unwrap() = Some(on_end);
				Box::new(|| {})
			}),
			answer: Box::new(move |_question, _answer| {
				let (send, receive) = oneshot::channel();
				posted.lock().unwrap().push(Some(send));
				Box::pin(async move { receive.await.expect("the test answers every post") })
			}),
			// Past the key grace from the first key on.
			now: Box::new(move || if calls.fetch_add(1, Ordering::SeqCst) == 0 { 0 } else { 10_000 }),
		};
		let question = Question {
			code: "c0de".to_owned(),
			view: allow_question("Merge acme-api/retry into main?", "2 commits.", Some("KQX")),
			reader: "r".to_owned(),
		};
		let sink = Arc::clone(&ended);
		let pane = Pane::new(|| {}, move |code| sink.lock().unwrap().push(code));
		Self { program: AskProgram::new(pane, question, io), ended, replies, end_hold }
	}

	fn type_letters(&mut self, letters: &str) {
		for letter in letters.chars() {
			self.program.on_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
		}
	}

	fn end_hold(&self) {
		let end = self.end_hold.lock().unwrap().take().expect("a hold is open");
		end();
	}

	fn reply(&self, at: usize, reply: AnswerReply) {
		let send = self.replies.lock().unwrap()[at].take().expect("one reply per post");
		assert!(send.send(reply).is_ok());
	}

	fn ended(&self) -> Vec<i32> {
		self.ended.lock().unwrap().clone()
	}
}

/// Lets the tasks the popup spawned run.
async fn settle() {
	for _ in 0..5 {
		tokio::task::yield_now().await;
	}
}

#[tokio::test(start_paused = true)]
async fn the_third_wrong_code_s_note_shows_for_2_s_though_the_hold_ends_before_the_410_reply_0578() {
	let mut popup = Popup::new();
	popup.type_letters("kqz");
	assert_eq!(popup.replies.lock().unwrap().len(), 1);
	// The service settled the question on the post: the hold ends first, its reply comes after.
	popup.end_hold();
	assert_eq!(popup.ended(), Vec::<i32>::new());
	popup.reply(0, AnswerReply::Exhausted);
	settle().await;
	assert!(text(&popup.program.lines(70, 18)).contains("Wrong code three times; Deny was taken."));
	tokio::time::advance(Duration::from_millis(1_900)).await;
	settle().await;
	assert_eq!(popup.ended(), Vec::<i32>::new());
	tokio::time::advance(Duration::from_millis(200)).await;
	settle().await;
	assert_eq!(popup.ended(), [0]);
}

#[tokio::test]
async fn a_hold_that_ends_while_a_wrong_code_s_reply_is_awaited_closes_the_popup_once_the_reply_came() {
	let mut popup = Popup::new();
	popup.type_letters("kqz");
	popup.end_hold();
	popup.reply(0, AnswerReply::Wrong { left: Some(1) });
	settle().await;
	assert_eq!(popup.ended(), [0]);
}

#[tokio::test]
async fn a_hold_that_ends_with_no_answer_posting_closes_the_popup_at_once() {
	let popup = Popup::new();
	popup.end_hold();
	assert_eq!(popup.ended(), [0]);
}

#[tokio::test]
async fn the_hold_is_released_on_cleanup_and_when_the_program_is_dropped() {
	let released = Arc::new(AtomicUsize::new(0));
	let make = |released: &Arc<AtomicUsize>| {
		let count = Arc::clone(released);
		let io = AskIo {
			hold: Box::new(move |_question, _on_end| {
				let count = Arc::clone(&count);
				Box::new(move || {
					count.fetch_add(1, Ordering::SeqCst);
				})
			}),
			answer: Box::new(|_question, _answer| Box::pin(async { AnswerReply::Taken })),
			now: Box::new(|| 0),
		};
		let question =
			Question { code: "c0de".to_owned(), view: allow_question("Merge?", "", None), reader: "r".to_owned() };
		AskProgram::new(Pane::new(|| {}, |_| {}), question, io)
	};
	let mut program = make(&released);
	program.cleanup();
	program.cleanup();
	assert_eq!(released.load(Ordering::SeqCst), 1);
	drop(program);
	assert_eq!(released.load(Ordering::SeqCst), 1);
	drop(make(&released));
	assert_eq!(released.load(Ordering::SeqCst), 2);
}
