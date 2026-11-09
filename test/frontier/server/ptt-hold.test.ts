import { describe, expect, test } from "bun:test";
import { DictationToggle, RightCtrlHold, VK_RCONTROL } from "../../../src/frontier/server/ptt-hold.ts";

const A = 0x41;

describe("Right Ctrl held alone (0368)", () => {
	test("press starts, auto-repeats are ignored, release stops", () => {
		const hold = new RightCtrlHold();
		expect(hold.key(VK_RCONTROL, true)).toBe("start");
		expect(hold.key(VK_RCONTROL, true)).toBeUndefined();
		expect(hold.key(VK_RCONTROL, true)).toBeUndefined();
		expect(hold.key(VK_RCONTROL, false)).toBe("stop");
	});

	test("another key during the hold cancels once, and the release sends nothing", () => {
		const hold = new RightCtrlHold();
		expect(hold.key(VK_RCONTROL, true)).toBe("start");
		expect(hold.key(A, true)).toBe("cancel");
		expect(hold.key(A, false)).toBeUndefined();
		expect(hold.key(A, true)).toBeUndefined();
		expect(hold.key(VK_RCONTROL, false)).toBeUndefined();
		// The next hold is a fresh one.
		expect(hold.key(VK_RCONTROL, true)).toBe("start");
		expect(hold.key(VK_RCONTROL, false)).toBe("stop");
	});

	test("keys outside a hold, and a release without a press, do nothing", () => {
		const hold = new RightCtrlHold();
		expect(hold.key(A, true)).toBeUndefined();
		expect(hold.key(VK_RCONTROL, false)).toBeUndefined();
		// A key held down before Right Ctrl only spoils the hold if it repeats or another goes down during it.
		expect(hold.key(VK_RCONTROL, true)).toBe("start");
		expect(hold.key(A, false)).toBeUndefined();
		expect(hold.key(VK_RCONTROL, false)).toBe("stop");
	});
});

describe("Right Ctrl toggles omp's dictation (0556)", () => {
	test("press and release each send the toggle; a cancelled hold toggles once to stop; repeats send nothing", async () => {
		let toggles = 0;
		const dictation = new DictationToggle(async () => {
			toggles++;
		});
		await dictation.phase("start");
		await dictation.phase("start");
		expect(toggles).toBe(1);
		await dictation.phase("stop");
		expect(toggles).toBe(2);
		// A stop or cancel without a recording sends nothing: omp and the key stay in step.
		await dictation.phase("stop");
		await dictation.phase("cancel");
		expect(toggles).toBe(2);
		await dictation.phase("start");
		await dictation.phase("cancel");
		expect(toggles).toBe(4);
	});
});
