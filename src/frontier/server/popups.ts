// His questions (0434, 0484, 0519, 0552, 0558, 0568): the close question, his Allow box, Finish's merge-or-PR choice
// and the New thread form. Each has a one-time code, random and never listed or broadcast: whatever shows the question
// (the herdr popup, 0553) gets the code from the service, reads its texts and posts its answer with the service's token
// and that code, and the code dies with the answer, so an answer can't be forged by something that only has the token
// (the Allow box's, 0484). Allow, Merge, PR and Stop everything also take three letters only their popup shows (0558,
// 0568): herdr's socket can press keys in an open popup, but blind keys can't type letters they can't see.
import { randomBytes, randomInt } from "node:crypto";
import type { PopupAnswer, PopupView } from "../protocol.ts";

/** The close question's buttons (0434, 0549); Cancel is the default, Escape's and the answer of no answer. */
export const STOP = "Stop everything";
export const CANCEL = "Cancel";
/** His Allow box's buttons (0484); Deny is the default, Escape's and the answer of no answer. */
export const ALLOW = "Allow";
export const DENY = "Deny";
/** Finish's choices (0426, 0544): merge into trunk or open a PR; Cancel is the default. */
export const MERGE = "Merge";
export const PR = "PR";
/** New thread's button (0554). */
export const START = "Start";

/** Letters a typed code is drawn from: no I or O, which read as 1 and 0. */
const CODE_LETTERS = "ABCDEFGHJKLMNPQRSTUVWXYZ";
/** Wrong codes before a coded question counts as its fallback: no guessing through herdr's socket (0558). */
export const WRONG_CODES = 3;

/** Three random letters he types to take a coded question's choices other than its fallback (0558, 0568). */
export function typedCode(): string {
	return Array.from({ length: 3 }, () => CODE_LETTERS[randomInt(CODE_LETTERS.length)]).join("");
}

/** The close question's text (0438, 0549): working and awaiting threads counted apart; "thread" for 1. */
export function closeQuestion(counts: { working: number; awaiting: number }): string {
	const { working, awaiting } = counts;
	const noun = (n: number) => (n === 1 ? "thread" : "threads");
	const question =
		awaiting === 0
			? `Stop ${working} working ${noun(working)}`
			: working === 0
				? `Stop ${awaiting} ${noun(awaiting)} awaiting you`
				: `Stop ${working} working ${noun(working)} and ${awaiting} awaiting you`;
	return `${question}? Their sessions are saved.`;
}

/** The close question (0434, 0438, 0549, 0568): his threads counted; Stop everything takes its code, Cancel one key. */
export function stopQuestion(counts: { working: number; awaiting: number }, code = typedCode()): PopupView {
	return {
		title: "Jocasta: Stop everything?",
		message: closeQuestion(counts),
		choices: [STOP, CANCEL],
		fallback: CANCEL,
		code,
	};
}

/** His Allow box (0484, 0558): the service's question and its detail; typing its code allows, anything else denies. */
export function allowQuestion(ask: { message: string; detail: string }, code = typedCode()): PopupView {
	return {
		title: "Jocasta: Allow or Deny",
		message: ask.message,
		detail: ask.detail,
		choices: [ALLOW, DENY],
		fallback: DENY,
		code,
	};
}

/** Finish (0426, 0544, 0568): merge or PR for the thread his key named, each taking its code; Cancel one key. */
export function finishQuestion(name: string, detail: string, code = typedCode()): PopupView {
	return {
		title: `Jocasta: Finish ${name}`,
		message: `Merge ${name} into its trunk, or open a PR?`,
		detail,
		choices: [MERGE, PR, CANCEL],
		fallback: CANCEL,
		code,
	};
}

/** New thread (0554): the project and the brief; it opens in plan mode and waits for his approval. */
export function newThreadQuestion(projects: string[], project: string | undefined): PopupView {
	return {
		title: "Jocasta: New thread",
		message: "Pick the project and write the brief; the thread opens in plan mode.",
		choices: [START, CANCEL],
		fallback: CANCEL,
		form: { projects, ...(project ? { project } : {}) },
	};
}

export interface Popup {
	/** 256 random bits as hex: only what shows the question gets it. */
	readonly code: string;
	readonly view: PopupView;
	/** His choice; the fallback (Cancel, Deny) when it goes unanswered. Resolves once. */
	readonly answer: Promise<string>;
	/** The form's fields with his answer (New thread): the project and the brief. */
	readonly fields: { project?: string; brief?: string };
	/** Whether it still waits for its answer. */
	readonly open: boolean;
	/** Ends it unanswered: the answer is the fallback and the code dies. Nothing once answered. */
	close(): void;
}

interface Entry {
	popup: Popup;
	settle: (choice: string) => void;
	fields: { project?: string; brief?: string };
	wrong: number;
}

/** What `Popups.answer` made of an answer. */
export type AnswerTaken = "taken" | "unknown" | "invalid" | "wrong";

/** The open questions by code. A code is only ever looked up here; a used or unknown one finds nothing. */
export class Popups {
	private readonly entries = new Map<string, Entry>();

	/** A new question's code and its answer. */
	open(view: PopupView): Popup {
		const code = randomBytes(32).toString("hex");
		const answer = Promise.withResolvers<string>();
		const fields: Entry["fields"] = {};
		let open = true;
		const settle = (choice: string) => {
			if (!open) return;
			open = false;
			this.entries.delete(code);
			answer.resolve(choice);
		};
		const popup: Popup = {
			code,
			view,
			answer: answer.promise,
			fields,
			get open() {
				return open;
			},
			close: () => settle(view.fallback),
		};
		this.entries.set(code, { popup, settle, fields, wrong: 0 });
		return popup;
	}

	/** The texts for `code`'s question; undefined for a code that is unknown or already answered. */
	view(code: string): PopupView | undefined {
		return this.entries.get(code)?.popup.view;
	}

	/** `code`'s open question, or undefined. */
	get(code: string): Popup | undefined {
		return this.entries.get(code)?.popup;
	}

	/**
	 * Takes `code`'s answer once: `taken`, `unknown` for a code that is wrong or already answered, `invalid` for a choice
	 * its question doesn't offer or a form missing a field (the code stays alive for the real answer), `wrong` for a
	 * coded question's choice other than its fallback with letters that aren't its code. The third wrong code ends it as
	 * its fallback (0558, 0568) and is `taken`. A bare string is the choice alone.
	 */
	answer(code: string, given: unknown): AnswerTaken {
		const entry = this.entries.get(code);
		if (!entry) return "unknown";
		const body = (typeof given === "string" ? { choice: given } : given) as Partial<PopupAnswer> | null;
		const choice = body?.choice;
		const { view } = entry.popup;
		if (typeof choice !== "string" || !view.choices.includes(choice)) return "invalid";
		if (view.code !== undefined && choice !== view.fallback) {
			const typed = typeof body?.typed === "string" ? body.typed.trim().toUpperCase() : "";
			if (typed !== view.code) {
				entry.wrong++;
				if (entry.wrong < WRONG_CODES) return "wrong";
				entry.settle(view.fallback);
				return "taken";
			}
		}
		if (view.form && choice === view.choices[0]) {
			const project = typeof body?.project === "string" ? body.project : "";
			const brief = typeof body?.brief === "string" ? body.brief.trim() : "";
			if (!view.form.projects.includes(project) || !brief) return "invalid";
			entry.fields.project = project;
			entry.fields.brief = brief;
		}
		entry.settle(choice);
		return "taken";
	}

	/** The questions still waiting for an answer, oldest first. */
	list(): Popup[] {
		return [...this.entries.values()].map((e) => e.popup);
	}

	/** Ends every open question unanswered (Jocasta is closing). */
	closeAll(): void {
		for (const popup of this.list()) popup.close();
	}
}
