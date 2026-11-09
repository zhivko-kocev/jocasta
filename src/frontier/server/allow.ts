/** His Allow before a risky boss or CLI action (0484, 0544, 0578): one question, answered or counted as Deny. */
import { UserError } from "../../shared/errors.ts";
import type { PopupEnd } from "./popups.ts";
import { type Door, person } from "./wording.ts";

/** How long a confirmation waits for his Allow before it counts as Deny (0484): 2 minutes. */
export const CONFIRM_WAIT_MS = 120_000;

/** One Allow/Deny question for him before a risky boss action (0484). */
export interface Confirmation {
	/** What the boss asks for, one line ending in `?`. */
	message: string;
	/** What it does, plain lines; the boss's own text (brief, lesson) is shown as it is, for him to read. */
	detail: string;
}

/** How a question asked through the service's question API ended (0578), with his answer. */
export type Confirm = (ask: Confirmation, signal: AbortSignal) => Promise<{ allow: boolean; how: PopupEnd }>;

export class AllowGate {
	/** The confirmations waiting for his answer, so a stopping service withdraws them. */
	private readonly asking = new Set<AbortController>();

	constructor(
		private readonly confirm: Confirm | undefined,
		private readonly waitMs = CONFIRM_WAIT_MS,
	) {}

	/**
	 * His Allow (0484), asked through the service's question API (0552). Deny, three wrong codes, a popup closed
	 * unanswered, another program reading the question (0578), no answer within the wait (2 minutes), a stopping service
	 * or nothing to ask with refuse with `nothing` (what stays as it was), worded for the door (0581), so the reply says
	 * which it was.
	 */
	async ask(ask: Confirmation, nothing: string, door: Door = "boss"): Promise<void> {
		const { He, him } = person(door);
		const confirm = this.confirm;
		if (!confirm) throw new UserError(`Nothing can ask ${him} in this run, which counts as Deny; ${nothing}.`);
		const waitMs = this.waitMs;
		const controller = new AbortController();
		this.asking.add(controller);
		// His answer, or the end of the wait, or the service stopping (`withdrawAll`), whichever comes first.
		const ended = Promise.withResolvers<"timeout" | "stopped">();
		const timer = setTimeout(() => ended.resolve("timeout"), waitMs);
		controller.signal.addEventListener("abort", () => ended.resolve("stopped"));
		let answer: { allow: boolean; how: PopupEnd } | "timeout" | "stopped";
		try {
			const asked = confirm(ask, controller.signal);
			// A dialog that fails after the wait ran out has nobody left to tell.
			asked.catch(() => {});
			answer = await Promise.race([asked, ended.promise]);
		} catch (error) {
			throw new UserError(
				`Jocasta couldn't ask ${him} (${(error as Error).message}), which counts as Deny; ${nothing}.`,
			);
		} finally {
			clearTimeout(timer);
			this.asking.delete(controller);
			// The dialog goes once the answer isn't wanted (the wait ran out); after his answer this changes nothing.
			controller.abort();
		}
		if (answer === "stopped") throw new UserError(`Jocasta is closing, which counts as Deny; ${nothing}.`);
		if (answer === "timeout") {
			const wait = waitMs % 60_000 === 0 ? `${waitMs / 60_000} minutes` : `${waitMs / 1000} s`;
			throw new UserError(`No answer within ${wait}, which counts as Deny; ${nothing}.`);
		}
		if (answer.allow) return;
		// How the question ended (0578), said truly.
		const why: Record<PopupEnd, string> = {
			answered: `${He} chose Deny`,
			"wrong-codes": "Three wrong codes, which count as Deny",
			closed: "The popup closed without an answer, which counts as Deny",
			"second-reader": "Another program read the question, which counts as Deny",
			withdrawn: "The question was withdrawn, which counts as Deny",
		};
		throw new UserError(`${why[answer.how]}; ${nothing}.`);
	}

	/** Withdraws every question still open: the service is stopping, which counts as Deny. */
	withdrawAll(): void {
		for (const ask of this.asking) ask.abort();
	}
}
