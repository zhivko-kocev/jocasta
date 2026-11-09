// His questions (0434, 0484, 0519, 0552, 0558, 0568, 0578): the close question, his Allow box, Finish's merge-or-PR
// choice and the New thread form. Each has a one-time code, random and never listed or broadcast: whatever shows the
// question (the herdr popup, 0553) gets the code from the service, reads its texts and posts its answer with the
// service's token and that code, and the code dies with the answer, so an answer can't be forged by something that only
// has the token (the Allow box's, 0484). Allow, Merge, PR and Stop everything also take three letters only their popup
// shows (0558, 0568): herdr's socket can press keys in an open popup, but blind keys can't type letters they can't see.
// One reader per question (0578): the first read of its texts gets a reader secret its hold and answer must carry, and a
// second read ends it as its fallback, so a program that took the code from the popup's environment gets nothing.
import { randomBytes, randomInt } from "node:crypto";
import { sameToken } from "../../shared/fsx.ts";
import type { PopupAnswer, PopupView } from "../protocol.ts";
import { cleanText } from "./herdr.ts";

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
	return `${question}? They pick up where they left off next time.`;
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

/**
 * How a question ended (0578): his answer (`answered`, his Deny or Cancel included), three wrong codes (`wrong-codes`),
 * its popup closed without an answer (`closed`), a second program read it (`second-reader`), or the asker stopped
 * waiting (`withdrawn`: the wait ran out, Jocasta is closing).
 */
export type PopupEnd = "answered" | "wrong-codes" | "closed" | "second-reader" | "withdrawn";

export interface Popup {
	/** 256 random bits as hex: only what shows the question gets it. */
	readonly code: string;
	/** Its texts, control characters removed (N-3, SEC-NET-3). */
	readonly view: PopupView;
	/** His choice; the fallback (Cancel, Deny) when it goes unanswered. Resolves once. */
	readonly answer: Promise<string>;
	/** How it ended; resolves with `answer`. */
	readonly how: Promise<PopupEnd>;
	/** The form's fields with his answer (New thread): the project and the brief. */
	readonly fields: { project?: string; brief?: string };
	/** Whether it still waits for its answer. */
	readonly open: boolean;
	/** Whether its texts were read (its reader secret handed out). */
	readonly read: boolean;
	/** Opened by one of his keys (Finish, New thread, Quit): no notification, and Allow questions go first (0578, 0581). */
	readonly fromKey: boolean;
	/** Ends it unanswered (`withdrawn` unless said): the answer is the fallback and the code dies. Nothing once ended. */
	close(how?: PopupEnd): void;
}

interface Entry {
	popup: Popup;
	settle: (choice: string, how: PopupEnd) => void;
	fields: { project?: string; brief?: string };
	wrong: number;
	reader?: string;
}

/** What `Popups.answer` made of an answer. */
export type AnswerTaken = "taken" | "unknown" | "invalid" | "wrong" | "exhausted";

/** The open questions by code. A code is only ever looked up here; a used or unknown one finds nothing. */
export class Popups {
	private readonly entries = new Map<string, Entry>();

	/** A new question's code and its answer; `fromKey` when one of his keys opened it. */
	open(given: PopupView, options: { fromKey?: boolean } = {}): Popup {
		const view = cleanText(given);
		const code = randomBytes(32).toString("hex");
		const answer = Promise.withResolvers<string>();
		const how = Promise.withResolvers<PopupEnd>();
		const fields: Entry["fields"] = {};
		let open = true;
		const settle = (choice: string, end: PopupEnd) => {
			if (!open) return;
			open = false;
			this.entries.delete(code);
			how.resolve(end);
			answer.resolve(choice);
		};
		const entry: Entry = { popup: undefined as unknown as Popup, settle, fields, wrong: 0 };
		entry.popup = {
			code,
			view,
			answer: answer.promise,
			how: how.promise,
			fields,
			get open() {
				return open;
			},
			get read() {
				return entry.reader !== undefined;
			},
			fromKey: options.fromKey === true,
			close: (end = "withdrawn") => settle(view.fallback, end),
		};
		this.entries.set(code, entry);
		return entry.popup;
	}

	/**
	 * The first read of `code`'s texts (0578): the view and the reader secret its hold and answer must carry. A second
	 * read of an open question ends it as its fallback (`second-reader`) and resolves `{second}`; undefined for a code
	 * that is unknown or already answered.
	 */
	read(code: string): { view: PopupView; reader: string } | { second: Popup } | undefined {
		const entry = this.entries.get(code);
		if (!entry) return undefined;
		if (entry.reader !== undefined) {
			entry.settle(entry.popup.view.fallback, "second-reader");
			return { second: entry.popup };
		}
		entry.reader = randomBytes(32).toString("hex");
		return { view: entry.popup.view, reader: entry.reader };
	}

	/** Whether `secret` is `code`'s reader secret: its texts were read, and by the one asking. */
	isReader(code: string, secret: string | null | undefined): boolean {
		const reader = this.entries.get(code)?.reader;
		return reader !== undefined && sameToken(secret ?? "", reader);
	}

	/** The texts for `code`'s question; undefined for a code that is unknown or already answered. */
	view(code: string): PopupView | undefined {
		return this.entries.get(code)?.popup.view;
	}

	/** `code`'s open question, or undefined. */
	get(code: string): Popup | undefined {
		return this.entries.get(code)?.popup;
	}

	/** The wrong codes `code`'s question still takes before it counts as its fallback. */
	left(code: string): number {
		const entry = this.entries.get(code);
		return entry ? WRONG_CODES - entry.wrong : 0;
	}

	/**
	 * Takes `code`'s answer once: `taken`, `unknown` for a code that is wrong or already answered, `invalid` for a choice
	 * its question doesn't offer or a form missing a field (the code stays alive for the real answer), `wrong` for a
	 * coded question's choice other than its fallback with letters that aren't its code. The third wrong code ends it as
	 * its fallback (0558, 0568) and is `exhausted`. A bare string is the choice alone.
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
				entry.settle(view.fallback, "wrong-codes");
				return "exhausted";
			}
		}
		if (view.form && choice === view.choices[0]) {
			const project = typeof body?.project === "string" ? body.project : "";
			const brief = typeof body?.brief === "string" ? body.brief.trim() : "";
			if (!view.form.projects.includes(project) || !brief) return "invalid";
			entry.fields.project = project;
			entry.fields.brief = brief;
		}
		entry.settle(choice, "answered");
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
