import { afterEach, expect, test, vi } from "bun:test";
import type { KeyEvent } from "@opentui/core";
import { type AskIo, askProgram } from "../../../src/frontier/ask/main.ts";
import type { AnswerReply, Question } from "../../../src/frontier/client.ts";
import { allowQuestion } from "../../../src/frontier/server/popups.ts";
import type { Line } from "../../../src/frontier/tui.ts";

afterEach(() => {
	vi.useRealTimers();
});

const text = (lines: Line[]) => lines.map((l) => l.map((s) => s.text).join("")).join("\n");

/** The popup's program on a fake service: the test ends the hold and answers each post when it wants. */
function popup() {
	const ended: number[] = [];
	const replies: PromiseWithResolvers<AnswerReply>[] = [];
	let endHold = () => {};
	const io: AskIo = {
		hold: (_question, onEnd) => {
			endHold = onEnd;
			return () => {};
		},
		answer: () => {
			const reply = Promise.withResolvers<AnswerReply>();
			replies.push(reply);
			return reply.promise;
		},
		// Past the key grace from the first key on.
		now: (() => {
			let calls = 0;
			return () => (calls++ === 0 ? 0 : 10_000);
		})(),
	};
	const question: Question = {
		code: "c0de",
		view: allowQuestion({ message: "Merge acme-api/retry into main?", detail: "2 commits." }, "KQX"),
		reader: "r",
	};
	const program = askProgram({ draw: () => {}, end: (code) => void ended.push(code) }, question, io);
	const type = (letters: string) => {
		for (const letter of letters) {
			// A key as OpenTUI hands it; the popup reads only its name and sequence.
			const key = { name: letter, sequence: letter } as KeyEvent;
			program.onKey?.(key);
		}
	};
	return { program, ended, replies, type, endHold: () => endHold() };
}

test("the third wrong code's note shows for 2 s though the hold ends before the 410 reply (0578)", async () => {
	vi.useFakeTimers();
	const { program, ended, replies, type, endHold } = popup();
	type("kqz");
	expect(replies).toHaveLength(1);
	// The service settled the question on the post: the hold ends first, its reply comes after.
	endHold();
	expect(ended).toEqual([]);
	replies[0]?.resolve({ kind: "exhausted" });
	await replies[0]?.promise;
	await Promise.resolve();
	expect(text(program.lines(70, 18))).toContain("Wrong code three times; Deny was taken.");
	vi.advanceTimersByTime(1_900);
	expect(ended).toEqual([]);
	vi.advanceTimersByTime(200);
	expect(ended).toEqual([0]);
});

test("a hold that ends while a wrong code's reply is awaited closes the popup once the reply came", async () => {
	const { ended, replies, type, endHold } = popup();
	type("kqz");
	endHold();
	replies[0]?.resolve({ kind: "wrong", left: 1 });
	await replies[0]?.promise;
	await Promise.resolve();
	expect(ended).toEqual([0]);
});

test("a hold that ends with no answer posting closes the popup at once", () => {
	const { ended, endHold } = popup();
	endHold();
	expect(ended).toEqual([0]);
});
