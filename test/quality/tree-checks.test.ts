import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { removeTree } from "../shared/remove.ts";
import { treeFindings } from "./tree-checks.ts";

// 0610's syntax-tree checks over `src`: flag parameters, functions that only forward, exported items without a
// comment. They fail on new cases only; today's are the baseline, `tree-baseline.txt`, one key a line.
const ROOT = join(import.meta.dir, "..", "..");

/** The keys of `found` beyond what `baseline` holds, counting repeats. */
function beyond(found: string[], baseline: string[]): string[] {
	const left = new Map<string, number>();
	for (const key of baseline) left.set(key, (left.get(key) ?? 0) + 1);
	return found.filter((key) => {
		const count = left.get(key) ?? 0;
		left.set(key, count - 1);
		return count <= 0;
	});
}

test("src has no flag parameter, forwarding function or undocumented export beyond the baseline (0610)", () => {
	const baseline = readFileSync(join(import.meta.dir, "tree-baseline.txt"), "utf8")
		.split("\n")
		.filter(Boolean);
	expect(beyond(treeFindings(ROOT, join(ROOT, "src")), baseline)).toEqual([]);
});

test("each check finds its case, and a commented, branch-free, working function passes (0610)", () => {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-tree-"));
	try {
		writeFileSync(
			join(dir, "a.ts"),
			[
				"export function loud(text: string, shout: boolean): string {",
				"\treturn shout ? text.toUpperCase() : text;",
				"}",
				"/** Says it. */",
				"export class Speaker {",
				"\tsay(text: string): string {",
				"\t\treturn loud(text, false);",
				"\t}",
				"\t/** The same, louder. */",
				"\tyell(text: string): string {",
				"\t\treturn this.say(text);",
				"\t}",
				"}",
				"/** Trimmed. */",
				"export const trimmed = (text: string): string => text.trim();",
			].join("\n"),
		);
		expect(treeFindings(dir, dir)).toEqual([
			"a.ts flag-parameter loud(shout)",
			"a.ts forwarding Speaker.yell",
			"a.ts undocumented-export Speaker.say",
			"a.ts undocumented-export loud",
		]);
	} finally {
		removeTree(dir);
	}
});
