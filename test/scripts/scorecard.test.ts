// The pure parts of scripts/scorecard.ts (0610, 0614, 0616): Biome's, jscpd's and `bun audit`'s output as lines.
import { expect, test } from "bun:test";
import { auditLine, biomeConfig, biomeLines, jscpdLine } from "../../scripts/scorecard.ts";

test("Biome's diagnostics are counted per rule, a rule with none is 0, and the config reports each at its threshold", () => {
	const diagnostic = (category: string) => ({ category, severity: "warning" });
	const json = `{"summary":{},"diagnostics":${JSON.stringify([
		diagnostic("lint/complexity/noExcessiveCognitiveComplexity"),
		diagnostic("lint/complexity/noExcessiveCognitiveComplexity"),
		diagnostic("lint/style/noExcessiveLinesPerFile"),
		diagnostic("lint/style/noNonNullAssertion"),
	])}}`;
	expect(biomeLines(`warning: experimental reporter\n${json}`)).toEqual([
		"biome src complexity/noExcessiveCognitiveComplexity >25: 2",
		"biome src complexity/noExcessiveLinesPerFunction >80: 0",
		"biome src complexity/useMaxParams >4: 0",
		"biome src style/noExcessiveLinesPerFile >800: 1",
	]);
	const rules = JSON.parse(biomeConfig()).linter.rules;
	expect(rules.recommended).toBe(false);
	expect(rules.complexity.noExcessiveCognitiveComplexity).toEqual({
		level: "warn",
		options: { maxAllowedComplexity: 25 },
	});
	expect(rules.complexity.useMaxParams.options).toEqual({ max: 4 });
	expect(rules.style.noExcessiveLinesPerFile.options).toEqual({ maxLines: 800 });
});

test("jscpd's report gives the duplicated share to two places; a report without a total is an error, not 0%", () => {
	const report = JSON.stringify({
		statistics: { total: { percentage: 1.9712, clones: 65, duplicatedLines: 549, lines: 27838 } },
	});
	expect(jscpdLine("test", report)).toBe("jscpd test duplicated: 1.97% (65 clones, 549 of 27838 lines)");
	expect(() => jscpdLine("test", "{}")).toThrow("no total");
});

test("bun audit's advisories are counted by severity, and none is said as none", () => {
	const advisory = (severity: string) => ({ id: 1, title: "x", severity });
	const found = JSON.stringify({
		dompurify: [advisory("critical"), advisory("high"), advisory("high")],
		other: [advisory("low")],
	});
	expect(auditLine(found)).toBe("bun audit: 1 critical, 2 high, 1 low");
	expect(auditLine("{}")).toBe("bun audit: none");
	expect(() => auditLine("error: could not reach the registry")).toThrow();
});
