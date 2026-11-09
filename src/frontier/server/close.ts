import { spawn } from "node:child_process";
import { selfCommand } from "../../shared/install.ts";
import { log } from "../../shared/log.ts";
import { onPath } from "../../shared/process.ts";
import type { PopupView } from "../protocol.ts";
import { type Popup, STOP, stopQuestion } from "./popups.ts";
import type { JocastaService } from "./service.ts";
import { within } from "./stop-sequence.ts";

/** How the close went: everything stopped, or he chose Cancel and everything runs on. */
export type CloseOutcome = "stopped" | "kept";

/**
 * The argv that opens a new terminal running `command` on Windows (0549): a Windows Terminal window when `wt` is
 * installed; otherwise `command` itself, which `openJocastaTerminal` starts detached so Windows gives it a console of
 * its own (cmd's `start` would read an unquoted title as the program, SEC-NET-9).
 */
export function terminalArgv(command: string[], wt: string | null): string[] {
	return wt ? [wt, "-w", "new", "--title", "Jocasta", ...command] : command;
}

/**
 * A new Windows Terminal window running `jocasta`, which attaches to herdr's session as it always does (0549); a new
 * console window where Windows Terminal isn't installed. Off Windows there is none: the question waits for a client.
 */
export function openJocastaTerminal(): void {
	if (process.platform !== "win32") {
		log("warn", "shutdown", "no terminal opens here; the close question waits for `jocasta`");
		return;
	}
	const [program, ...args] = terminalArgv(selfCommand(), onPath("wt"));
	if (!program) return;
	// Not hidden: a detached console program gets a console window of its own.
	spawn(program, args, { detached: true, stdio: "ignore" }).unref();
}

/** What the close needs. */
export interface CloseParts {
	service: Pick<JocastaService, "discovered" | "busyThreads">;
	question: (view: PopupView, signal?: AbortSignal, options?: { fromKey?: boolean }) => Popup;
	/** The stop under way, if any. */
	stopping: () => Promise<void> | undefined;
	/** Stops everything (`all`). */
	stopAll: () => Promise<void>;
	/** Opens a new terminal attached to herdr's session; `openJocastaTerminal` by default. */
	reattach?: (() => void) | undefined;
}

/**
 * The close (0434, 0438, 0549): threads working or awaiting him ask first (`reattach` first opens a terminal on
 * herdr's session for it: his client is gone), and Cancel keeps everything; otherwise everything stops. One close at a
 * time: a second asks nothing more. The question his quit key opened raises no notification (0581); the one after his
 * terminal closed does.
 */
export function closeFlow(parts: CloseParts): (how: { ask: boolean; reattach: boolean }) => Promise<CloseOutcome> {
	const { service } = parts;
	let asking: Promise<CloseOutcome> | undefined;
	return ({ ask, reattach }) => {
		const stopping = parts.stopping();
		if (stopping) return stopping.then(() => "stopped");
		asking ??= (async (): Promise<CloseOutcome> => {
			// Threads count once herdr's panes were found (LC-9); a start that hangs is waited for 10 s at most.
			if (ask) await within(service.discovered, 10_000).catch(() => {});
			const { working, awaiting } = ask ? service.busyThreads() : { working: 0, awaiting: 0 };
			const asked = working + awaiting > 0;
			if (asked) {
				if (reattach) {
					try {
						(parts.reattach ?? openJocastaTerminal)();
					} catch (error) {
						log("warn", "shutdown", `no terminal opened for the close question: ${(error as Error).message}`);
					}
				}
				const popup = parts.question(stopQuestion({ working, awaiting }), undefined, { fromKey: !reattach });
				if ((await popup.answer) !== STOP) {
					const now = service.busyThreads();
					log(
						"info",
						"shutdown",
						`kept running: Cancel chosen with ${now.working} working and ${now.awaiting} awaiting`,
					);
					return "kept";
				}
			}
			const now = asked ? service.busyThreads() : undefined;
			log(
				"info",
				"shutdown",
				now
					? `stopping everything: Stop chosen with ${now.working} working and ${now.awaiting} awaiting`
					: "stopping everything",
			);
			await parts.stopAll();
			return "stopped";
		})().finally(() => {
			asking = undefined;
		});
		return asking;
	};
}
