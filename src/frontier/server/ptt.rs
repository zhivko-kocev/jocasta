//! Global push-to-talk in the service (0368, 0552, 0556): the Right Ctrl hook runs on a thread of its own
//! ([`super::ptt_worker`]; a Bun Worker in the TypeScript); each phase toggles omp's own dictation in the boss pane
//! through herdr. Windows only: elsewhere there is no global key.

#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use tokio::sync::mpsc;
#[cfg(windows)]
use tokio::task::JoinHandle;

#[cfg(windows)]
use super::ptt_hold::{DictationToggle, PushToTalkPhase};
#[cfg(windows)]
use super::ptt_worker::{self, PushToTalkHook};
use crate::shared::errors::Result;
#[cfg(windows)]
use crate::shared::log::{LogLevel, log};

/// How long a stopping push-to-talk waits for its queue of phases to drain.
#[cfg(windows)]
const DRAIN: Duration = Duration::from_secs(1);

/// A running push-to-talk.
pub struct PushToTalk {
	/// The hook, unless it wasn't installed (logged then; the next start tries again).
	#[cfg(windows)]
	hook: Option<PushToTalkHook>,
	#[cfg(windows)]
	phases: JoinHandle<()>,
	#[cfg(not(windows))]
	never: std::convert::Infallible,
}

impl PushToTalk {
	/// Unhooks the key and ends the hook's thread.
	#[cfg(windows)]
	pub async fn stop(self) {
		if let Some(hook) = self.hook {
			// The hook's thread ends its message loop on WM_QUIT; waiting for it blocks.
			let _ = tokio::task::spawn_blocking(move || hook.stop()).await;
		}
		let mut phases = self.phases;
		if tokio::time::timeout(DRAIN, &mut phases).await.is_err() {
			phases.abort();
		}
	}

	/// Unhooks the key and ends the hook's thread.
	#[cfg(not(windows))]
	pub async fn stop(self) {
		match self.never {}
	}
}

/// Starts the hook; `toggle` sends omp's toggle key to the boss pane (`app.stt.toggle`). A failed send is logged: the
/// next press toggles again. `None` off Windows. Runs inside a tokio runtime.
#[cfg(windows)]
pub fn start_push_to_talk<F, Fut>(toggle: F) -> Option<PushToTalk>
where
	F: FnMut() -> Fut + Send + 'static,
	Fut: Future<Output = Result<()>> + Send + 'static,
{
	let (sender, mut queue) = mpsc::unbounded_channel::<PushToTalkPhase>();
	// In order: a release never overtakes its press.
	let phases = tokio::spawn(async move {
		let mut dictation = DictationToggle::new(toggle);
		while let Some(phase) = queue.recv().await {
			if let Err(error) = dictation.phase(phase).await {
				log(LogLevel::Warn, "ptt", &format!("dictation's key didn't reach the boss pane: {error}"), None);
			}
		}
	});
	let hook = match ptt_worker::start(move |phase| {
		let _ = sender.send(phase);
	}) {
		Ok(hook) => {
			log(LogLevel::Info, "ptt", "the push-to-talk hook is installed (Right Ctrl)", None);
			Some(hook)
		}
		Err(error) => {
			log(LogLevel::Warn, "ptt", &format!("the push-to-talk hook failed: {error}"), None);
			None
		}
	};
	Some(PushToTalk { hook, phases })
}

/// Starts the hook; `None` off Windows, where there is no global key.
#[cfg(not(windows))]
pub fn start_push_to_talk<F, Fut>(_toggle: F) -> Option<PushToTalk>
where
	F: FnMut() -> Fut + Send + 'static,
	Fut: Future<Output = Result<()>> + Send + 'static,
{
	None
}
