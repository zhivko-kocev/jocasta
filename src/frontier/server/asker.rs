//! His questions' asker (0519, 0552): each question opens in `Popups` with its one-time code and goes to what shows it,
//! herdr's client shell in front (fork plan phase 5) or a test's `on_question`.

use std::sync::Arc;

use futures_util::future::BoxFuture as LocalBoxFuture;

use crate::frontier::protocol::PopupView;
use crate::frontier::server::events::Toast;
use crate::frontier::server::popups::{Popup, Popups};
use crate::shared::log::{LogLevel, log};
use crate::shared::process::AbortSignal;

/// Asks him a question: `popups` opens it, `show` shows it (0519, 0552). The signal ends it unanswered once it aborts;
/// the flag says his key asked it (`fromKey`).
pub type Ask = Arc<dyn Fn(&PopupView, Option<AbortSignal>, bool) -> Popup + Send + Sync>;

/// Shows a question; an `Err` is logged as a question it couldn't show.
pub type Show = Arc<dyn Fn(&Popup) -> crate::shared::errors::Result<()> + Send + Sync>;

/// What the service raises a notification with: the TypeScript's `Pick<JocastaService, "notify">`. `show` false is a
/// repeat that is logged and not shown again.
pub trait Notifier: Send + Sync {
	fn notify(&self, toast: Toast, show: bool) -> LocalBoxFuture<'_, ()>;
}

/// Opens each question in `popups` and hands it to `show`; it ends unanswered on a signal.
pub fn asker(popups: Arc<Popups>, show: Option<Show>) -> Ask {
	Arc::new(move |view, signal, from_key| {
		let popup = popups.open(view, from_key);
		if let Some(signal) = signal {
			if signal.borrow().is_some() {
				popup.close(None);
			} else {
				let watched = popup.clone();
				tokio::spawn(async move {
					let mut signal = Some(signal);
					tokio::select! {
						_ = crate::shared::process::aborted(&mut signal) => watched.close(None),
						_ = watched.answer() => {}
					}
				});
			}
		}
		if popup.open() {
			log(LogLevel::Info, "question", &format!("asking him: {}", popup.view().title), None);
			if let Some(Err(error)) = show.as_ref().map(|show| show(&popup)) {
				log(LogLevel::Warn, "question", &format!("couldn't show it: {error}"), None);
			}
		}
		popup
	})
}

#[cfg(test)]
#[path = "asker_tests.rs"]
mod tests;
