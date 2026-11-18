//! His Allow before a risky boss or CLI action (0484, 0544, 0578): one question, answered or counted as Deny.

use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::future::BoxFuture;
use tokio::sync::watch;
use tokio::time::{Duration, sleep};

use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::wording::{Door, person};
use crate::shared::errors::{Error, Result};
use crate::shared::process::{AbortSignal, aborted};

/// How long a confirmation waits for his Allow before it counts as Deny (0484): 2 minutes.
pub const CONFIRM_WAIT_MS: i64 = 120_000;

/// One Allow/Deny question for him before a risky boss action (0484).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
	/// What the boss asks for, one line ending in `?`.
	pub message: String,
	/// What it does, plain lines; the boss's own text (brief, lesson) is shown as it is, for him to read.
	pub detail: String,
}

/// His answer to a question asked through the service's question API (0578), and how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
	pub allow: bool,
	pub how: PopupEnd,
}

/// How a question asked through the service's question API ended (0578), with his answer.
pub type Confirm = Arc<dyn Fn(Confirmation, AbortSignal) -> BoxFuture<'static, Result<Answer>> + Send + Sync>;

/// A refusal by the Allow box itself (0613). `chose` is true when he pressed Deny, false for every other end (no answer
/// in time, wrong codes, a closed popup, a stopping service), which says nothing about what he wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowDenied {
	pub message: String,
	pub chose: bool,
}

/// Why `AllowGate::ask` refused: the Allow box's own refusal, or any other.
#[derive(Debug)]
pub enum AskError {
	Denied(AllowDenied),
	Refused(Error),
}

impl From<AskError> for Error {
	fn from(error: AskError) -> Self {
		match error {
			AskError::Denied(denied) => Error::user(denied.message),
			AskError::Refused(error) => error,
		}
	}
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct AllowGate {
	confirm: Option<Confirm>,
	wait_ms: i64,
	/// The confirmations waiting for his answer, so a stopping service withdraws them.
	asking: Mutex<Vec<(u64, watch::Sender<Option<String>>)>>,
	next: Mutex<u64>,
}

enum Ended {
	Answer(Answer),
	Failed(String),
	Stopped,
	Timeout,
}

impl AllowGate {
	/// `wait_ms` defaults to `CONFIRM_WAIT_MS`.
	pub fn new(confirm: Option<Confirm>, wait_ms: Option<i64>) -> Self {
		Self {
			confirm,
			wait_ms: wait_ms.unwrap_or(CONFIRM_WAIT_MS),
			asking: Mutex::new(Vec::new()),
			next: Mutex::new(0),
		}
	}

	/// His Allow (0484), asked through the service's question API (0552). Deny, three wrong codes, a popup closed
	/// unanswered, another program reading the question (0578), no answer within the wait (2 minutes), a stopping
	/// service or nothing to ask with refuse with `nothing` (what stays as it was), worded for the door (0581), so the
	/// reply says which it was.
	pub async fn ask(&self, ask: &Confirmation, nothing: &str, door: Door) -> std::result::Result<(), AskError> {
		let person = person(door);
		let Some(confirm) = &self.confirm else {
			return Err(AskError::Refused(Error::user(format!(
				"Nothing can ask {} in this run, which counts as Deny; {nothing}.",
				person.him
			))));
		};
		let (sender, receiver) = watch::channel(None);
		let id = {
			let mut next = locked(&self.next);
			*next += 1;
			*next
		};
		locked(&self.asking).push((id, sender.clone()));
		let ended = self.race(confirm, ask, receiver).await;
		locked(&self.asking).retain(|(other, _)| *other != id);
		// The dialog goes once the answer isn't wanted (the wait ran out); after his answer this changes nothing.
		sender.send_replace(Some("the answer isn't wanted".to_owned()));
		self.refusal(ended, person.him, person.he_cap, nothing)
	}

	/// His answer, or the end of the wait, or the service stopping (`withdraw_all`), whichever comes first.
	async fn race(&self, confirm: &Confirm, ask: &Confirmation, signal: AbortSignal) -> Ended {
		let mut withdrawn = Some(signal.clone());
		// Spawned, so a dialog that fails after the wait ran out has nobody left to tell, and one still open sees the abort.
		let mut asked = tokio::spawn(confirm(ask.clone(), signal));
		let wait = Duration::from_millis(u64::try_from(self.wait_ms).unwrap_or(0));
		tokio::select! {
			biased;
			_ = aborted(&mut withdrawn) => Ended::Stopped,
			result = &mut asked => match result {
				Ok(Ok(answer)) => Ended::Answer(answer),
				Ok(Err(error)) => Ended::Failed(error.to_string()),
				Err(error) => Ended::Failed(error.to_string()),
			},
			() = sleep(wait) => Ended::Timeout,
		}
	}

	fn refusal(&self, ended: Ended, him: &str, he_cap: &str, nothing: &str) -> std::result::Result<(), AskError> {
		let answer = match ended {
			Ended::Failed(message) => {
				return Err(AskError::Refused(Error::user(format!(
					"Jocasta couldn't ask {him} ({message}), which counts as Deny; {nothing}."
				))));
			}
			Ended::Stopped => {
				return Err(AskError::Refused(Error::user(format!(
					"Jocasta is closing, which counts as Deny; {nothing}."
				))));
			}
			Ended::Timeout => {
				let wait_ms = self.wait_ms;
				let wait = if wait_ms % 60_000 == 0 {
					format!("{} minutes", wait_ms / 60_000)
				} else {
					format!("{} s", wait_ms as f64 / 1000.0)
				};
				return Err(AskError::Refused(Error::user(format!(
					"No answer within {wait}, which counts as Deny; {nothing}."
				))));
			}
			Ended::Answer(answer) => answer,
		};
		if answer.allow {
			return Ok(());
		}
		// How the question ended (0578), said truly.
		let why = match answer.how {
			PopupEnd::Answered => format!("{he_cap} chose Deny"),
			PopupEnd::WrongCodes => "Three wrong codes, which count as Deny".to_owned(),
			PopupEnd::Closed => "The popup closed without an answer, which counts as Deny".to_owned(),
			PopupEnd::SecondReader => "Another program read the question, which counts as Deny".to_owned(),
			PopupEnd::Withdrawn => "The question was withdrawn, which counts as Deny".to_owned(),
		};
		Err(AskError::Denied(AllowDenied {
			message: format!("{why}; {nothing}."),
			chose: answer.how == PopupEnd::Answered,
		}))
	}

	/// Withdraws every question still open: the service is stopping, which counts as Deny.
	pub fn withdraw_all(&self) {
		for (_, sender) in locked(&self.asking).iter() {
			sender.send_replace(Some("Jocasta is closing".to_owned()));
		}
	}
}

#[cfg(test)]
#[path = "allow_tests.rs"]
mod tests;
