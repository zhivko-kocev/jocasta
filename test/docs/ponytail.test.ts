// 0612: Ponytail's two rewritten lines and the removed one, in the shipped file and in the eval variant's copy.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..", "..");

for (const file of ["config/ponytail.md", "evals/variants/ponytail-important-if.md"]) {
	test(`${file} carries 0612's three changes`, () => {
		// The header comment names the old lines on purpose; only the prompt text below it is checked.
		const full = readFileSync(join(ROOT, file), "utf8");
		const text = full.slice(full.indexOf("# Ponytail, lazy senior dev mode"));
		expect(text).toContain(
			"- Add nothing the task does not need. Keep the structure the codebase has; when the task shows that structure is wrong, fix the structure, not a special case around it. A one-liner that needs decoding is not short.\n",
		);
		expect(text).toContain(
			"- Comment each exported function, class and type for its caller: what it does, units, edge cases, errors. Inside a function, comment only the why the code cannot show.\n",
		);
		for (const old of [
			"shortest working diff",
			"Keep the structure the codebase already has",
			"Comment only the why the code cannot show, in one line",
			"ponytail repo itself",
		])
			expect(text).not.toContain(old);
	});
}
