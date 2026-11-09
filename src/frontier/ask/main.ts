// `jocasta plugin ask`: the `ask` popup (0553, 0558, 0563, 0568, 0578), an OpenTUI program herdr opens in front with
// the question's one-time code in `JOCASTA_POPUP`. It reads the question (its only reader, keeping the reader secret),
// holds the service's held request while it shows it (closing the popup any other way ends that request, which takes
// the fallback: Deny, Cancel), posts his answer and exits; it also exits when the service ends the question (answered,
// withdrawn, Jocasta closing).
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import { answerQuestion, holdQuestion, type Question, questionView } from "../client.ts";
import { POPUP_ENV, type PopupAnswer } from "../protocol.ts";
import { fit, type Pane, type PaneProgram, runPane } from "../tui.ts";
import { askLines, askState, exhaustedNote, paste, press, wrongNote } from "./view.ts";

/** Keys this soon after the popup first draws are dropped: typing meant for another pane spends no tries (0578). */
const KEY_GRACE_MS = 750;
/** How long the third wrong code's note shows before the popup closes (0578). */
const EXHAUSTED_MS = 2_000;

/** How the popup reaches the service: its hold and its answer (`client.ts`'s; a test passes its own). */
export interface AskIo {
	hold: typeof holdQuestion;
	answer: typeof answerQuestion;
	now: () => number;
}

/** Runs the popup until the question is answered or over; resolves its exit code. */
export async function runAsk(): Promise<number> {
	const code = process.env[POPUP_ENV] ?? "";
	const question = code ? await questionView(code) : "No question was handed to this popup.";
	if (typeof question === "string") {
		return runPane((pane) => {
			const timer = setTimeout(() => pane.end(1), 2_000);
			return {
				lines: (width) => [fit([{ text: question, fg: TOKYO_NIGHT.yellow }], width)],
				cleanup: () => clearTimeout(timer),
			};
		});
	}
	return runPane((pane) => askProgram(pane, question));
}

/**
 * The popup's program for `question`. The service ends the hold as soon as an answer settles the question, which can
 * reach the popup before the answer's own reply: while an answer is posting, the hold's end waits for that reply, so
 * the third wrong code's note shows for its 2 s whichever arrives first (0578).
 */
export function askProgram(
	pane: Pane,
	question: Question,
	io: AskIo = { hold: holdQuestion, answer: answerQuestion, now: Date.now },
): PaneProgram {
	const state = askState(question.view);
	const shownAt = io.now();
	let closing: Timer | undefined;
	let sending = false;
	/** The hold ended: the question is over, and the popup closes once no reply is awaited. */
	let over = false;
	const release = io.hold(question, () => {
		over = true;
		if (!closing && !sending) pane.end(0);
	});
	/** An Esc pressed while an answer was posting: sent once that post wasn't taken (FE-11). */
	let queued: PopupAnswer | undefined;
	const send = async (answer: PopupAnswer) => {
		sending = true;
		const reply = await io.answer(question, answer);
		sending = false;
		if (reply.kind === "exhausted") {
			state.typed = "";
			state.note = exhaustedNote(question.view.fallback);
			closing = setTimeout(() => pane.end(0), EXHAUSTED_MS);
			pane.draw();
			return;
		}
		if (reply.kind === "taken" || over) {
			pane.end(0);
			return;
		}
		state.typed = "";
		state.note = reply.kind === "wrong" ? wrongNote(reply.left, question.view.fallback) : reply.text;
		pane.draw();
		const next = queued;
		queued = undefined;
		if (next) void send(next);
	};
	return {
		lines: (width, height) => askLines(state, Math.max(20, width - 2), height),
		onKey(key) {
			if (closing || io.now() - shownAt < KEY_GRACE_MS) return;
			const answer = press(state, key);
			pane.draw();
			if (!answer) return;
			if (!sending) void send(answer);
			else if (answer.choice === question.view.fallback && answer.typed === undefined) queued = answer;
		},
		onPaste(text) {
			paste(state, text);
			pane.draw();
		},
		cleanup() {
			clearTimeout(closing);
			release();
		},
	};
}
