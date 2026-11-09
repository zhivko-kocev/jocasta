// Global push-to-talk in the service (0368, 0552, 0556): the Right Ctrl hook runs in a worker; each phase toggles omp's
// own dictation in the boss pane through herdr. Windows only: elsewhere there is no global key.
import { dlopen, FFIType } from "bun:ffi";
import { log } from "../../shared/log.ts";
import { DictationToggle, type PushToTalkPhase } from "./ptt-hold.ts";

const WM_QUIT = 0x12;

export interface PushToTalk {
	/** Unhooks the key and ends the worker. */
	stop(): Promise<void>;
}

/**
 * Starts the hook; `toggle` sends omp's toggle key to the boss pane (`app.stt.toggle`). A failed send is logged: the
 * next press toggles again. Undefined off Windows.
 */
export function startPushToTalk(toggle: () => Promise<void>): PushToTalk | undefined {
	if (process.platform !== "win32") return undefined;
	const dictation = new DictationToggle(async () => {
		try {
			await toggle();
		} catch (error) {
			log("warn", "ptt", `dictation's key didn't reach the boss pane: ${(error as Error).message}`);
		}
	});
	// The worker first sends its thread id, so the stop can end its message loop with WM_QUIT (`terminate` can't stop a
	// thread blocked in GetMessageW).
	const worker = new Worker(new URL("./ptt-worker.ts", import.meta.url).href);
	let thread: number | undefined;
	let queue = Promise.resolve();
	worker.onmessage = (event: MessageEvent<PushToTalkPhase | { thread: number }>) => {
		// The worker posts its thread id once its hook is in.
		if (typeof event.data === "object") {
			thread = event.data.thread;
			log("info", "ptt", "the push-to-talk hook is installed (Right Ctrl)");
		} else {
			const phase = event.data;
			// In order: a release never overtakes its press.
			queue = queue.then(() => dictation.phase(phase));
		}
	};
	worker.onerror = (event) => log("warn", "ptt", `the push-to-talk hook failed: ${event.message}`);
	return {
		stop: async () => {
			const { promise: closed, resolve } = Promise.withResolvers<void>();
			worker.addEventListener("close", () => resolve(), { once: true });
			if (thread !== undefined) {
				const user32 = dlopen("user32.dll", {
					PostThreadMessageW: { args: [FFIType.u32, FFIType.u32, FFIType.u64, FFIType.i64], returns: FFIType.i32 },
				});
				try {
					if (user32.symbols.PostThreadMessageW(thread, WM_QUIT, 0n, 0n))
						await Promise.race([closed, Bun.sleep(1_000)]);
				} finally {
					user32.close();
				}
			}
			worker.terminate();
		},
	};
}
