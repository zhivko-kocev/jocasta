import { expect, test } from "bun:test";
import { invoiceTitle } from "./a.ts";
import { dueReportName } from "./b.ts";
import { hoursSummary } from "./c.ts";
import { capitalize, clamp, isWeekend, kebabCase, round, sum, truncate } from "./utils.ts";

test("string helpers", () => {
	expect(capitalize("acme")).toBe("Acme");
	expect(kebabCase(" Client A Report ")).toBe("client-a-report");
	expect(truncate("example-org", 20)).toBe("example-org");
	expect(truncate("example-org", 5)).toBe("exam…");
});

test("date helpers", () => {
	expect(isWeekend(new Date("2026-10-10T12:00:00Z"))).toBe(true);
	expect(isWeekend(new Date("2026-10-12T12:00:00Z"))).toBe(false);
});

test("math helpers", () => {
	expect(clamp(120, 1, 90)).toBe(90);
	expect(sum([1, 2, 3.5])).toBe(6.5);
	expect(round(2.345, 1)).toBe(2.3);
});

test("callers", () => {
	const issued = new Date("2026-10-01T09:00:00Z");
	expect(invoiceTitle("acme", issued)).toBe("Acme invoice, 2026-10-01");
	expect(dueReportName("Client A Report", issued, 200)).toBe("client-a-report-2026-12-30.md");
	expect(
		hoursSummary("Migration for client-a", [
			{ day: new Date("2026-10-09T09:00:00Z"), hours: 2.25 },
			{ day: new Date("2026-10-10T09:00:00Z"), hours: 4 },
			{ day: new Date("2026-10-12T09:00:00Z"), hours: 1.5 },
		]),
	).toBe("Migration f…: 3.8 h");
});
