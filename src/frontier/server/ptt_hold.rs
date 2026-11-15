//! Global push-to-talk (0368, 0556): Right Ctrl held alone, anywhere in Windows. The hook thread
//! ([`super::ptt_worker`]) feeds every key event here; the service sends omp's dictation toggle to the boss pane on its
//! press and again on its release.

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::shared::errors::Result;

/// Right Ctrl's virtual-key code as a low-level keyboard hook reports it (VK_RCONTROL).
pub const VK_RCONTROL: u32 = 0xa3;

/// omp's key binding that starts or stops its speech-to-text (0556), unbound by omp.
pub const STT_TOGGLE_SETTING: &str = "app.stt.toggle";
/// The key setup binds it to and the service sends through herdr's `pane.send_keys`: unused by omp and herdr (0556).
pub const STT_TOGGLE_KEY: &str = "f9";

/// What one key event makes of push-to-talk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PushToTalkPhase {
	Start,
	Stop,
	Cancel,
}

impl PushToTalkPhase {
	/// The phase as the TypeScript names it (`"start"`, `"stop"`, `"cancel"`).
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Start => "start",
			Self::Stop => "stop",
			Self::Cancel => "cancel",
		}
	}
}

/// Turns raw key events into push-to-talk phases: `Start` when Right Ctrl goes down (its auto-repeats are ignored);
/// `Cancel` once, when any other key goes down during the hold, since the hold is then a shortcut, not a dictation;
/// `Stop` when Right Ctrl comes up after a hold that stayed alone.
#[derive(Debug, Default)]
pub struct RightCtrlHold {
	held: bool,
	spoiled: bool,
}

impl RightCtrlHold {
	pub fn new() -> Self {
		Self::default()
	}

	/// One key event; returns the phase it causes, if any.
	pub fn key(&mut self, vk: u32, down: bool) -> Option<PushToTalkPhase> {
		if vk == VK_RCONTROL {
			if down {
				if self.held {
					return None;
				}
				self.held = true;
				self.spoiled = false;
				return Some(PushToTalkPhase::Start);
			}
			if !self.held {
				return None;
			}
			self.held = false;
			return if self.spoiled { None } else { Some(PushToTalkPhase::Stop) };
		}
		if !down || !self.held || self.spoiled {
			return None;
		}
		self.spoiled = true;
		Some(PushToTalkPhase::Cancel)
	}
}

/// What Right Ctrl's phases send to the boss pane (0556): the toggle on press (recording starts) and on release
/// (recording stops and omp sends the text, `stt.submitTrigger: release`); a cancelled hold (another key went down, a
/// shortcut) toggles too, so the recording it started stops. Each phase sends at most one toggle, and a stop or
/// cancel only after a start that sent one, so omp's recording and the key stay in step.
pub struct DictationToggle<F> {
	toggle: F,
	recording: bool,
}

impl<F, Fut> DictationToggle<F>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = Result<()>>,
{
	pub fn new(toggle: F) -> Self {
		Self { toggle, recording: false }
	}

	pub async fn phase(&mut self, phase: PushToTalkPhase) -> Result<()> {
		if phase == PushToTalkPhase::Start {
			if self.recording {
				return Ok(());
			}
			self.recording = true;
		} else {
			if !self.recording {
				return Ok(());
			}
			self.recording = false;
		}
		(self.toggle)().await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const A: u32 = 0x41;

	#[test]
	fn press_starts_auto_repeats_are_ignored_release_stops() {
		let mut hold = RightCtrlHold::new();
		assert_eq!(hold.key(VK_RCONTROL, true), Some(PushToTalkPhase::Start));
		assert_eq!(hold.key(VK_RCONTROL, true), None);
		assert_eq!(hold.key(VK_RCONTROL, true), None);
		assert_eq!(hold.key(VK_RCONTROL, false), Some(PushToTalkPhase::Stop));
	}

	#[test]
	fn another_key_during_the_hold_cancels_once_and_the_release_sends_nothing() {
		let mut hold = RightCtrlHold::new();
		assert_eq!(hold.key(VK_RCONTROL, true), Some(PushToTalkPhase::Start));
		assert_eq!(hold.key(A, true), Some(PushToTalkPhase::Cancel));
		assert_eq!(hold.key(A, false), None);
		assert_eq!(hold.key(A, true), None);
		assert_eq!(hold.key(VK_RCONTROL, false), None);
		// The next hold is a fresh one.
		assert_eq!(hold.key(VK_RCONTROL, true), Some(PushToTalkPhase::Start));
		assert_eq!(hold.key(VK_RCONTROL, false), Some(PushToTalkPhase::Stop));
	}

	#[test]
	fn keys_outside_a_hold_and_a_release_without_a_press_do_nothing() {
		let mut hold = RightCtrlHold::new();
		assert_eq!(hold.key(A, true), None);
		assert_eq!(hold.key(VK_RCONTROL, false), None);
		// A key held down before Right Ctrl only spoils the hold if it repeats or another goes down during it.
		assert_eq!(hold.key(VK_RCONTROL, true), Some(PushToTalkPhase::Start));
		assert_eq!(hold.key(A, false), None);
		assert_eq!(hold.key(VK_RCONTROL, false), Some(PushToTalkPhase::Stop));
	}

	#[tokio::test]
	async fn press_and_release_each_send_the_toggle_a_cancelled_hold_toggles_once_to_stop_repeats_send_nothing() {
		let toggles = std::cell::Cell::new(0);
		let mut dictation = DictationToggle::new(|| {
			toggles.set(toggles.get() + 1);
			async { Ok(()) }
		});
		dictation.phase(PushToTalkPhase::Start).await.unwrap();
		dictation.phase(PushToTalkPhase::Start).await.unwrap();
		assert_eq!(toggles.get(), 1);
		dictation.phase(PushToTalkPhase::Stop).await.unwrap();
		assert_eq!(toggles.get(), 2);
		// A stop or cancel without a recording sends nothing: omp and the key stay in step.
		dictation.phase(PushToTalkPhase::Stop).await.unwrap();
		dictation.phase(PushToTalkPhase::Cancel).await.unwrap();
		assert_eq!(toggles.get(), 2);
		dictation.phase(PushToTalkPhase::Start).await.unwrap();
		dictation.phase(PushToTalkPhase::Cancel).await.unwrap();
		assert_eq!(toggles.get(), 4);
	}
}
