//! `asker.rs`'s tests.

use std::sync::Mutex;

use tokio::sync::watch;

use super::*;
use crate::frontier::server::popups::{PopupEnd, Popups};
use crate::shared::errors::Error;
use crate::shared::testing::temp_jocasta;

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap()
}

fn allow_box() -> PopupView {
	PopupView {
		title: "Jocasta: Allow or Deny".to_owned(),
		message: "Remove the leftover worktree acme-api/retry?".to_owned(),
		detail: Some("Its branch stays.".to_owned()),
		choices: vec!["Allow".to_owned(), "Deny".to_owned()],
		fallback: "Deny".to_owned(),
		code: None,
		form: None,
	}
}

fn aborting() -> (watch::Sender<Option<String>>, AbortSignal) {
	watch::channel(None)
}

#[tokio::test]
async fn the_asker_opens_each_question_shows_it_and_ends_it_unanswered_on_its_signal() {
	let _j = temp_jocasta();
	let popups = Arc::new(Popups::new());
	let shown = Arc::new(Mutex::new(Vec::<Popup>::new()));
	let record = shown.clone();
	let show: Show = Arc::new(move |popup| {
		locked(&record).push(popup.clone());
		Ok(())
	});
	let ask = asker(popups.clone(), Some(show));
	// What shows it gets the popup; nothing else does.
	let popup = ask(&allow_box(), None, false);
	assert_eq!(*locked(&shown), std::slice::from_ref(&popup));
	assert!(popup.open());
	assert!(!popup.from_key());
	assert_eq!(popups.list(), std::slice::from_ref(&popup));
	assert!(ask(&allow_box(), None, true).from_key());

	// Aborted later: the fallback.
	let (abort, signal) = aborting();
	let gone = ask(&allow_box(), Some(signal), false);
	assert!(gone.open());
	abort.send_replace(Some("no longer wanted".to_owned()));
	assert_eq!(gone.answer().await, "Deny");
	assert_eq!(gone.how().await, PopupEnd::Withdrawn);

	// Aborted already: closed before it is shown.
	let before = locked(&shown).len();
	let (abort, signal) = aborting();
	abort.send_replace(Some("already".to_owned()));
	let never = ask(&allow_box(), Some(signal), false);
	assert!(!never.open());
	assert_eq!(never.answer().await, "Deny");
	assert_eq!(locked(&shown).len(), before);
}

#[tokio::test]
async fn a_question_that_cant_be_shown_is_logged_and_stays_open() {
	let _j = temp_jocasta();
	let popups = Arc::new(Popups::new());
	let show: Show = Arc::new(|_popup| Err(Error::other("herdr is gone")));
	let popup = asker(popups, Some(show))(&allow_box(), None, false);
	assert!(popup.open());
	let without = asker(Arc::new(Popups::new()), None)(&allow_box(), None, false);
	assert!(without.open());
}
