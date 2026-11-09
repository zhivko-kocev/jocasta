import { expect, test } from "bun:test";
import { askLines, askState, type Key, press, wrap } from "../../../src/frontier/ask/view.ts";
import { allowQuestion, finishQuestion, newThreadQuestion, stopQuestion } from "../../../src/frontier/server/popups.ts";
import type { Line } from "../../../src/frontier/today/view.ts";

const key = (name: string, sequence = name.length === 1 ? name : ""): Key => ({ name, sequence });
const text = (lines: Line[]) => lines.map((l) => l.map((s) => s.text).join(""));

test("Allow: only its three letters typed allow; Enter and other keys never do; Esc denies (0558)", () => {
	const state = askState(allowQuestion({ message: "Merge acme-api/retry into main?", detail: "2 commits." }, "KQX"));
	expect(text(askLines(state, 60))).toContain("Type KQX for Allow");
	expect(text(askLines(state, 60))).toContain("←→ choose · Esc Deny");
	for (const k of [key("return"), key("enter"), key("space", " "), key("1"), key("y")]) {
		expect(press(state, k)).toBeUndefined();
	}
	// "y" was a letter: it counts as typed, so it is taken back.
	press(state, key("backspace"));
	expect(press(state, key("k"))).toBeUndefined();
	expect(press(state, key("q"))).toBeUndefined();
	expect(press(state, key("x"))).toEqual({ choice: "Allow", typed: "KQX" });
	expect(press(askState(state.view), key("escape"))).toEqual({ choice: "Deny" });
	// Deny picked: one key, and letters type nothing.
	const deny = askState(state.view);
	press(deny, key("right"));
	for (const c of "kqx") expect(press(deny, key(c))).toBeUndefined();
	expect(press(deny, key("return"))).toEqual({ choice: "Deny" });
});

test("the close question: Stop everything takes its code, never Enter; Cancel is one key (0563, 0568)", () => {
	const state = askState(stopQuestion({ working: 2, awaiting: 1 }, "ABC"));
	expect(text(askLines(state, 100))).toContain("Stop 2 working threads and 1 awaiting you? Their sessions are saved.");
	expect(text(askLines(state, 100))).toContain("Type ABC for Stop everything");
	expect(press(state, key("return"))).toBeUndefined();
	expect(press(askState(state.view), key("escape"))).toEqual({ choice: "Cancel" });
	const cancel = askState(state.view);
	press(cancel, key("left"));
	expect(text(askLines(cancel, 100))).toContain("←→ choose · Enter Cancel · Esc Cancel");
	expect(press(cancel, key("return"))).toEqual({ choice: "Cancel" });
	for (const c of "ab") press(state, key(c));
	expect(press(state, key("c"))).toEqual({ choice: "Stop everything", typed: "ABC" });
});

test("Finish: Merge and PR each take the code, moving the pick clears the letters; New thread takes the project and the brief (0544, 0554, 0568)", () => {
	const finish = askState(finishQuestion("acme-api/retry", "2 commits.", "KQX"));
	expect(text(askLines(finish, 60))).toContain("Type KQX for Merge");
	expect(press(finish, key("return"))).toBeUndefined();
	press(finish, key("k"));
	press(finish, key("right"));
	expect(finish.typed).toBe("");
	expect(text(askLines(finish, 60))).toContain("Type KQX for PR");
	for (const c of "kq") press(finish, key(c));
	expect(press(finish, key("x"))).toEqual({ choice: "PR", typed: "KQX" });
	const cancel = askState(finish.view);
	press(cancel, key("left"));
	expect(press(cancel, key("enter"))).toEqual({ choice: "Cancel" });

	const form = askState(newThreadQuestion(["acme-api", "client-a"], "client-a"));
	expect(form.field).toBe("brief");
	expect(press(form, key("return"))).toBeUndefined();
	expect(form.note).toBe("Write the brief first.");
	for (const c of "Fix it") press(form, key(c === " " ? "space" : c.toLowerCase(), c));
	press(form, key("tab"));
	press(form, key("up"));
	press(form, key("tab"));
	expect(press(form, key("enter"))).toEqual({ choice: "Start", project: "acme-api", brief: "Fix it" });
});

test("wrap keeps words whole within the width", () => {
	expect(wrap("one two three four", 9)).toEqual(["one two", "three", "four"]);
});
