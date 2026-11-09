import { expect, test } from "bun:test";
import {
	askLines,
	askState,
	exhaustedNote,
	type Key,
	latinNote,
	paste,
	press,
	wrongNote,
} from "../../../src/frontier/ask/view.ts";
import { allowQuestion, finishQuestion, newThreadQuestion, stopQuestion } from "../../../src/frontier/server/popups.ts";
import { columns, type Line, wrap } from "../../../src/frontier/tui.ts";

const key = (name: string, sequence = name.length === 1 ? name : ""): Key => ({ name, sequence });
const text = (lines: Line[]) => lines.map((l) => l.map((s) => s.text).join(""));

test("Allow: only its three letters typed allow; Enter and other keys never do; Esc denies (0558)", () => {
	const state = askState(allowQuestion({ message: "Merge acme-api/retry into main?", detail: "2 commits." }, "KQX"));
	expect(text(askLines(state, 60, 18))).toContain("Type KQX for Allow");
	expect(text(askLines(state, 60, 18))).toContain("←→ choose · Esc Deny");
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
	expect(text(askLines(state, 100, 18))).toContain(state.view.message);
	expect(text(askLines(state, 100, 18))).toContain("Type ABC for Stop everything");
	expect(press(state, key("return"))).toBeUndefined();
	expect(press(askState(state.view), key("escape"))).toEqual({ choice: "Cancel" });
	const cancel = askState(state.view);
	press(cancel, key("left"));
	expect(text(askLines(cancel, 100, 18))).toContain("←→ choose · Enter Cancel · Esc Cancel");
	expect(press(cancel, key("return"))).toEqual({ choice: "Cancel" });
	for (const c of "ab") press(state, key(c));
	expect(press(state, key("c"))).toEqual({ choice: "Stop everything", typed: "ABC" });
});

test("Finish: Merge and PR each take the code, moving the pick clears the letters (0544, 0568)", () => {
	const finish = askState(finishQuestion("acme-api/retry", "2 commits.", "KQX"));
	expect(text(askLines(finish, 60, 18))).toContain("Type KQX for Merge");
	expect(press(finish, key("return"))).toBeUndefined();
	press(finish, key("k"));
	press(finish, key("right"));
	expect(finish.typed).toBe("");
	expect(text(askLines(finish, 60, 18))).toContain("Type KQX for PR");
	for (const c of "kq") press(finish, key(c));
	expect(press(finish, key("x"))).toEqual({ choice: "PR", typed: "KQX" });
	const cancel = askState(finish.view);
	press(cancel, key("left"));
	expect(press(cancel, key("enter"))).toEqual({ choice: "Cancel" });
});

test("New thread: opens on the brief with a project picked; type-to-find, the caret and the hint follow the field (0581)", () => {
	// No project focused this run: the service's pick may be absent, and the form still opens on the brief.
	const form = askState(newThreadQuestion(["acme-api", "acme-web", "client-a"], undefined));
	expect(form.field).toBe("brief");
	expect(press(form, key("return"))).toBeUndefined();
	expect(form.note).toBe("Write the brief first.");
	for (const c of "Fix it") press(form, key(c === " " ? "space" : c.toLowerCase(), c));
	let lines = text(askLines(form, 60, 18));
	expect(lines).toContain("Brief    Fix it▏");
	expect(lines.at(-1)).toBe("Tab project · Enter start · Esc cancel");
	press(form, key("tab"));
	lines = text(askLines(form, 60, 18));
	expect(lines).toContain("Brief    Fix it");
	expect(lines.at(-1)).toBe("Type to find · ↑↓ project · Enter brief · Esc cancel");
	// A letter jumps to the next project starting with it; the same letter again goes on to the next such one.
	press(form, key("c"));
	expect(form.project).toBe(2);
	press(form, key("a"));
	expect(form.project).toBe(0);
	press(form, key("a"));
	expect(form.project).toBe(1);
	press(form, key("up"));
	press(form, key("tab"));
	expect(press(form, key("enter"))).toEqual({ choice: "Start", project: "acme-api", brief: "Fix it" });
	// A paste goes into the brief as one line.
	paste(form, "and\nthe test");
	expect(form.brief).toBe("Fix itand the test");
});

test("a 10-line brief at 84×18 keeps the buttons, the code, its echo and the hint on screen, and PgDn reads on (0578)", () => {
	const brief = Array.from({ length: 10 }, (_, i) => `Line ${i + 1} of the boss's brief for the export client.`);
	const view = allowQuestion(
		{
			message: 'Start the thread "Export retry backoff" in acme-api?',
			detail: `The last project pane you focused is in client-a, not acme-api.\n\nThe boss's brief:\n${brief.join("\n")}`,
		},
		"KQX",
	);
	const state = askState(view);
	// The popup is 84 columns; `main.ts` keeps one column clear at each side.
	let lines = text(askLines(state, 82, 18));
	expect(lines).toHaveLength(18);
	expect(lines[0]).toBe("Jocasta: Allow or Deny");
	expect(lines).toContain(" Allow    Deny ");
	expect(lines).toContain("Type KQX for Allow");
	expect(lines).toContain("> ___");
	expect(lines.at(-1)).toBe("←→ choose · PgUp/PgDn read · Esc Deny");
	const more = lines.find((l) => l.startsWith("… "));
	expect(more).toMatch(/^… \d+ more lines · PgDn reads on$/);
	expect(lines).toContain("Line 1 of the boss's brief for the export client.");
	expect(lines).not.toContain("Line 10 of the boss's brief for the export client.");
	// PgDn until the end: the last line shows and the "more" line goes.
	for (let i = 0; i < 5; i++) press(state, key("pagedown"));
	lines = text(askLines(state, 82, 18));
	expect(lines).toHaveLength(18);
	expect(lines).toContain("Line 10 of the boss's brief for the export client.");
	expect(lines.some((l) => l.startsWith("… "))).toBe(false);
	expect(lines).toContain("> ___");
	press(state, key("pageup"));
	expect(text(askLines(state, 82, 18)).some((l) => l.startsWith("… "))).toBe(true);
	// A note shows above the hint without pushing the code off.
	state.note = wrongNote(1, "Deny");
	lines = text(askLines(state, 82, 18));
	expect(lines).toHaveLength(18);
	expect(lines).toContain("Wrong code; one try left, then it counts as Deny.");
	expect(lines).toContain("> ___");
	// Every size from tiny to wide keeps the code's echo within the rows.
	for (const height of [12, 14, 16, 18]) {
		for (const width of [40, 60, 82, 140]) {
			const shown = text(askLines(askState(view), width, height));
			expect(shown.length).toBeLessThanOrEqual(height);
			expect(shown).toContain("> ___");
			expect(shown.every((l) => columns(l) <= width)).toBe(true);
		}
	}
});

test("typed-code notes: a letter that isn't Latin says to switch the layout; one try left; the third wrong code (0578)", () => {
	const state = askState(allowQuestion({ message: "Merge acme-api/retry into main?", detail: "" }, "KQX"));
	expect(press(state, key("к", "к"))).toBeUndefined();
	expect(state.typed).toBe("");
	expect(state.note).toBe("Type the Latin letters KQX (switch the keyboard layout).");
	expect(latinNote("KQX")).toBe(state.note as string);
	// A Latin letter clears it.
	press(state, key("k"));
	expect(state.note).toBeUndefined();
	expect(wrongNote(2, "Deny")).toBe("Wrong code; nothing was done.");
	expect(wrongNote(1, "Deny")).toBe("Wrong code; one try left, then it counts as Deny.");
	expect(exhaustedNote("Deny")).toBe("Wrong code three times; Deny was taken.");
});

test("wrap keeps words whole within the width and splits a word longer than the line, keeping every character", () => {
	expect(wrap("one two three four", 9)).toEqual(["one two", "three", "four"]);
	const path = "D:\\work\\acme\\.jocasta\\worktrees\\acme-api-export-retry-backoff-with-jitter";
	const lines = wrap(`asks to delete ${path}`, 30);
	expect(lines.every((l) => columns(l) <= 30)).toBe(true);
	expect(lines.join("").replaceAll(" ", "")).toBe(`askstodelete${path}`);
	// Wide characters count two columns each.
	const wide = wrap("修复导出重试的退避逻辑", 8);
	expect(wide.every((l) => columns(l) <= 8)).toBe(true);
	expect(wide.join("")).toBe("修复导出重试的退避逻辑");
});
