import { expect, test } from "bun:test";
import { dayLabel, dayStart, hhmm, localDay, midnightOf, weekday } from "../../src/shared/day.ts";

test("local days and their midnights", () => {
	const at = new Date(2026, 10, 3, 9, 5).getTime();
	expect(localDay(at)).toBe("2026-11-03");
	expect(dayStart("2026-11-03")).toBe(new Date(2026, 10, 3).getTime());
	expect(dayStart("2026-11-03", 1)).toBe(new Date(2026, 10, 4).getTime());
	expect(midnightOf(at)).toBe(new Date(2026, 10, 3).getTime());
});

test("HH:MM, the weekday and the day label as every page says them", () => {
	const at = new Date(2026, 10, 3, 9, 5);
	expect(hhmm(at)).toBe("09:05");
	expect(hhmm(at.getTime())).toBe("09:05");
	expect(weekday(at)).toBe("Tue");
	expect(dayLabel(at)).toBe("Tue 3 Nov");
	expect(dayLabel("2026-11-03", { year: true })).toBe("Tue 3 Nov 2026");
});
