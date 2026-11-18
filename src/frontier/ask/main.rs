//! `jocasta plugin ask` (the Rust port of `main.ts`): the `ask` popup (0553, 0558, 0563, 0568, 0578), a terminal program
//! herdr opens in front with the question's one-time code in `JOCASTA_POPUP`. It reads the question (its only reader,
//! keeping the reader secret), holds the service's held request while it shows it (closing the popup any other way ends
//! that request, which takes the fallback: Deny, Cancel), posts his answer and exits; it also exits when the service
//! ends the question (answered, withdrawn, Jocasta closing).

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ratatui::crossterm::event::KeyEvent;
use tokio::task::JoinHandle;

use crate::frontier::ask::view::{AskState, Key, ask_lines, ask_state, exhausted_note, paste, press, wrong_note};
use crate::frontier::client::{AnswerReply, Question, answer_question, hold_question, question_view};
use crate::frontier::protocol::{POPUP_ENV, PopupAnswer};
use crate::frontier::server::routes::BoxFuture;
use crate::frontier::tui::{Line, Pane, PaneProgram, Segment, TOKYO_NIGHT, fit, run_pane};
use crate::shared::env;
use crate::shared::errors::Result;

/// Keys this soon after the popup first draws are dropped: typing meant for another pane spends no tries (0578).
const KEY_GRACE_MS: i64 = 750;
/// How long the third wrong code's note shows before the popup closes (0578).
const EXHAUSTED_MS: u64 = 2_000;

/// What ends a hold, and what releases it.
pub type OnEnd = Box<dyn FnOnce() + Send>;

/// `client::hold_question`'s shape: the callback for the hold's end in, the release out.
pub type Hold = Box<dyn Fn(&Question, OnEnd) -> OnEnd + Send + Sync>;

/// How the popup reaches the service: its hold and its answer (`client.rs`'s; a test passes its own).
pub struct AskIo {
	pub hold: Hold,
	pub answer: Box<dyn Fn(Question, PopupAnswer) -> BoxFuture<AnswerReply> + Send + Sync>,
	pub now: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl AskIo {
	/// The popup's own client: `client.rs`'s hold and answer, the wall clock.
	pub fn service() -> Self {
		Self {
			hold: Box::new(|question, on_end| hold_question(question, on_end)),
			answer: Box::new(|question, answer| Box::pin(async move { answer_question(&question, &answer).await })),
			now: Box::new(|| chrono::Utc::now().timestamp_millis()),
		}
	}
}

/// Runs the popup until the question is answered or over; resolves its exit code.
pub async fn run_ask() -> Result<i32> {
	let code = env::var(POPUP_ENV).unwrap_or_default();
	let question = if code.is_empty() {
		Err("No question was handed to this popup.".to_owned())
	} else {
		question_view(&code).await
	};
	match question {
		Err(text) => run_pane(|pane| Notice::new(pane, text)).await,
		Ok(question) => run_pane(|pane| AskProgram::new(pane, question, AskIo::service())).await,
	}
}

/// The popup when there is no question to show: its one line, then exit code 1 after 2 s.
struct Notice {
	text: String,
	timer: JoinHandle<()>,
}

impl Notice {
	fn new(pane: Pane, text: String) -> Self {
		let timer = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_secs(2)).await;
			pane.end(1);
		});
		Self { text, timer }
	}
}

impl PaneProgram for Notice {
	fn lines(&mut self, width: u16, _height: u16) -> Vec<Line> {
		vec![fit(&[Segment::new(self.text.clone(), TOKYO_NIGHT.yellow)], usize::from(width))]
	}

	fn cleanup(&mut self) {
		self.timer.abort();
	}
}

/// What the popup's key handler and its post share.
struct Shared {
	state: AskState,
	/// The third wrong code's note is showing; the popup closes when its timer ends.
	closing: Option<JoinHandle<()>>,
	sending: bool,
	/// The hold ended: the question is over, and the popup closes once no reply is awaited.
	over: bool,
	/// An Esc pressed while an answer was posting: sent once that post wasn't taken (FE-11).
	queued: Option<PopupAnswer>,
}

struct Inner {
	pane: Pane,
	question: Question,
	io: AskIo,
	shared: Mutex<Shared>,
}

impl Inner {
	fn shared(&self) -> MutexGuard<'_, Shared> {
		self.shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	/// What a reply does once it came: `Some(next)` is a queued answer to post now.
	fn settle(&self, reply: AnswerReply) -> Option<PopupAnswer> {
		let fallback = &self.question.view.fallback;
		let mut shared = self.shared();
		shared.sending = false;
		match reply {
			AnswerReply::Exhausted => {
				shared.state.typed.clear();
				shared.state.note = Some(exhausted_note(fallback));
				let pane = self.pane.clone();
				shared.closing = Some(tokio::spawn(async move {
					tokio::time::sleep(Duration::from_millis(EXHAUSTED_MS)).await;
					pane.end(0);
				}));
				drop(shared);
				self.pane.draw();
				None
			}
			AnswerReply::Taken => {
				drop(shared);
				self.pane.end(0);
				None
			}
			_ if shared.over => {
				drop(shared);
				self.pane.end(0);
				None
			}
			reply => {
				shared.state.typed.clear();
				shared.state.note = Some(match reply {
					AnswerReply::Wrong { left } => wrong_note(left.and_then(|left| u32::try_from(left).ok()), fallback),
					AnswerReply::Failed { text } => text,
					AnswerReply::Taken | AnswerReply::Exhausted => String::new(),
				});
				let next = shared.queued.take();
				drop(shared);
				self.pane.draw();
				next
			}
		}
	}

	/// Posts `answer` (its request starts at once) and settles its reply; a queued Esc goes next. The service ends the
	/// hold as soon as an answer settles the question, which can reach the popup before the answer's own reply: while
	/// an answer is posting, the hold's end waits for that reply, so the third wrong code's note shows for its 2 s
	/// whichever arrives first (0578).
	fn send(self: &Arc<Self>, answer: PopupAnswer) {
		self.shared().sending = true;
		let reply = (self.io.answer)(self.question.clone(), answer);
		let inner = Arc::clone(self);
		tokio::spawn(async move {
			let mut reply = reply;
			loop {
				let settled = reply.await;
				let Some(next) = inner.settle(settled) else { return };
				inner.shared().sending = true;
				reply = (inner.io.answer)(inner.question.clone(), next);
			}
		});
	}
}

/// The popup's program for a question.
pub struct AskProgram {
	inner: Arc<Inner>,
	shown_at: i64,
	release: Option<OnEnd>,
}

impl AskProgram {
	/// The popup's program for `question`.
	pub fn new(pane: Pane, question: Question, io: AskIo) -> Self {
		let state = ask_state(question.view.clone());
		let shown_at = (io.now)();
		let inner = Arc::new(Inner {
			pane,
			question,
			io,
			shared: Mutex::new(Shared { state, closing: None, sending: false, over: false, queued: None }),
		});
		let ending = Arc::clone(&inner);
		let release = (inner.io.hold)(
			&inner.question,
			Box::new(move || {
				let mut shared = ending.shared();
				shared.over = true;
				let ends_now = shared.closing.is_none() && !shared.sending;
				drop(shared);
				if ends_now {
					ending.pane.end(0);
				}
			}),
		);
		Self { inner, shown_at, release: Some(release) }
	}

	fn release(&mut self) {
		if let Some(release) = self.release.take() {
			release();
		}
		if let Some(closing) = self.inner.shared().closing.take() {
			closing.abort();
		}
	}
}

impl Drop for AskProgram {
	fn drop(&mut self) {
		self.release();
	}
}

impl PaneProgram for AskProgram {
	fn lines(&mut self, width: u16, height: u16) -> Vec<Line> {
		let width = usize::from(width).saturating_sub(2).max(20);
		ask_lines(&mut self.inner.shared().state, width, usize::from(height))
	}

	fn on_key(&mut self, key: KeyEvent) {
		let inner = &self.inner;
		if inner.shared().closing.is_some() || (inner.io.now)() - self.shown_at < KEY_GRACE_MS {
			return;
		}
		let answer = press(&mut inner.shared().state, &Key::from(key));
		inner.pane.draw();
		let Some(answer) = answer else { return };
		let sending = inner.shared().sending;
		if !sending {
			inner.send(answer);
		} else if answer.choice == inner.question.view.fallback && answer.typed.is_none() {
			inner.shared().queued = Some(answer);
		}
	}

	fn on_paste(&mut self, text: &str) {
		paste(&mut self.inner.shared().state, text);
		self.inner.pane.draw();
	}

	fn cleanup(&mut self) {
		self.release();
	}
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
