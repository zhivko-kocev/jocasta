// The pure parts of scripts/scorecard.ts (0610, 0614, 0616, 0645): clippy's, jscpd's and cargo deny's output as lines.
import { expect, test } from "bun:test";
import { clippyConfig, clippyLines, denyLine, jscpdLine } from "../../scripts/scorecard.ts";

test("clippy's diagnostics are counted per lint once each, a lint with none is 0, and the config sets each threshold", () => {
	const diagnostic = (code: string, file: string, line: number) =>
		JSON.stringify({
			reason: "compiler-message",
			message: { code: { code }, spans: [{ file_name: file, line_start: line, is_primary: true }] },
		});
	const output = [
		diagnostic("clippy::cognitive_complexity", "src/a.rs", 10),
		diagnostic("clippy::cognitive_complexity", "src/a.rs", 10),
		diagnostic("clippy::cognitive_complexity", "src/b.rs", 4),
		diagnostic("clippy::too_many_lines", "src/c.rs", 1),
		diagnostic("clippy::needless_return", "src/c.rs", 9),
		JSON.stringify({ reason: "compiler-artifact" }),
		"warning: not a json line",
	].join("\n");
	expect(clippyLines(output)).toEqual([
		"clippy cognitive_complexity >25: 2",
		"clippy too_many_lines >80: 1",
		"clippy too_many_arguments >4: 0",
	]);
	expect(clippyConfig()).toBe(
		"cognitive-complexity-threshold = 25\ntoo-many-lines-threshold = 80\ntoo-many-arguments-threshold = 4\n",
	);
});

test("jscpd's report gives the duplicated share to two places; a report without a total is an error, not 0%", () => {
	const report = JSON.stringify({
		statistics: { total: { percentage: 1.9712, clones: 65, duplicatedLines: 549, lines: 27838 } },
	});
	expect(jscpdLine("src", report)).toBe("jscpd src duplicated: 1.97% (65 clones, 549 of 27838 lines)");
	expect(() => jscpdLine("src", "{}")).toThrow("no total");
});

test("cargo deny's advisories are counted by severity, and none is said as none", () => {
	const summary = (counts: Record<string, number>) =>
		`{"fields":{"advisories":${JSON.stringify({ errors: 0, helps: 0, notes: 0, warnings: 0, ...counts })}},"type":"summary"}`;
	expect(denyLine(`{"type":"diagnostic","fields":{}}\n${summary({ errors: 1, warnings: 2 })}`)).toBe(
		"cargo deny advisories: 1 errors, 2 warnings",
	);
	expect(denyLine(summary({}))).toBe("cargo deny advisories: none");
	expect(() => denyLine("error: could not reach the advisory database")).toThrow();
});
