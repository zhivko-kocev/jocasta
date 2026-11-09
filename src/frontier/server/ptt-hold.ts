// Global push-to-talk (0368, 0556): Right Ctrl held alone, anywhere in Windows. The hook worker feeds every key event
// here; the service sends omp's dictation toggle to the boss pane on its press and again on its release.

/** Right Ctrl's virtual-key code as a low-level keyboard hook reports it (VK_RCONTROL). */
export const VK_RCONTROL = 0xa3;

/** omp's key binding that starts or stops its speech-to-text (0556), unbound by omp. */
export const STT_TOGGLE_SETTING = "app.stt.toggle";
/** The key setup binds it to and the service sends through herdr's `pane.send_keys`: unused by omp and herdr (0556). */
export const STT_TOGGLE_KEY = "f9";

export type PushToTalkPhase = "start" | "stop" | "cancel";

/**
 * Turns raw key events into push-to-talk phases: `start` when Right Ctrl goes down (its auto-repeats are ignored);
 * `cancel` once, when any other key goes down during the hold, since the hold is then a shortcut, not a dictation;
 * `stop` when Right Ctrl comes up after a hold that stayed alone.
 */
export class RightCtrlHold {
	private held = false;
	private spoiled = false;

	/** One key event; returns the phase it causes, if any. */
	key(vk: number, down: boolean): PushToTalkPhase | undefined {
		if (vk === VK_RCONTROL) {
			if (down) {
				if (this.held) return undefined;
				this.held = true;
				this.spoiled = false;
				return "start";
			}
			if (!this.held) return undefined;
			this.held = false;
			return this.spoiled ? undefined : "stop";
		}
		if (!down || !this.held || this.spoiled) return undefined;
		this.spoiled = true;
		return "cancel";
	}
}

/**
 * What Right Ctrl's phases send to the boss pane (0556): the toggle on press (recording starts) and on release
 * (recording stops and omp sends the text, `stt.submitTrigger: release`); a cancelled hold (another key went down, a
 * shortcut) toggles too, so the recording it started stops. Each phase sends at most one toggle, and a stop or
 * cancel only after a start that sent one, so omp's recording and the key stay in step.
 */
export class DictationToggle {
	private recording = false;

	constructor(private readonly toggle: () => Promise<void>) {}

	async phase(phase: PushToTalkPhase): Promise<void> {
		if (phase === "start") {
			if (this.recording) return;
			this.recording = true;
		} else {
			if (!this.recording) return;
			this.recording = false;
		}
		await this.toggle();
	}
}
