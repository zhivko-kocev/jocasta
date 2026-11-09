import { expect, test } from "bun:test";
import type { Dashboard, Hud, Today } from "../../../src/frontier/protocol.ts";
import { KEY_LINE, type Line, render, threadPanes, tokens } from "../../../src/frontier/today/view.ts";

const now = new Date(2026, 10, 9, 14, 30);
const text = (line: Line) => line.map((s) => s.text).join("");

const today: Today = {
	date: "2026-11-09",
	calendar: [
		{ time: "all day", title: "Release week" },
		{ time: "09:00", title: "Standup" },
		{ time: "16:00", title: "Review with client-a" },
	],
	needsYou: [
		{ kind: "blocked", text: "Retry — which backoff?", paneId: "w2:p1", project: "acme-api" },
		{ kind: "review", text: "example-org/web#12 — review requested" },
	],
	landed: [{ title: "Docs pass", project: "acme-api", line: "merged into main as 1234567" }],
	underway: [
		{
			paneId: "w3:p1",
			title: "Export v2",
			project: "acme-api",
			line: "writing tests",
			state: "working",
			since: now.getTime() - 25 * 60_000,
			cost: 1.5,
			context: 83_400,
		},
		{
			paneId: "w4:p1",
			title: "Cleanup",
			project: "client-a",
			line: "",
			state: "idle",
			since: now.getTime() - 3_600_000,
		},
	],
	next: [],
};
const hud: Hud = {
	running: 1,
	blocked: 1,
	awaiting: 2,
	spendToday: 3.2,
	quota: [
		{ window: "5h", used: 0.84 },
		{ window: "7d", used: 0.18 },
	],
};
const dashboard: Dashboard = {
	news: [
		{
			heading: "Hacker News",
			items: [
				{ title: "A headline", source: "hn", url: "https://example.org/1", at: "2026-11-09T10:00:00Z" },
				{ title: "Another", source: "hn", url: "https://example.org/2", at: "2026-11-09T09:00:00Z" },
			],
		},
	],
	newsError: null,
	weatherError: null,
	weather: {
		place: "Example City",
		tempC: 11.6,
		code: 2,
		summary: "Partly cloudy",
		highC: 14,
		lowC: 6,
		at: "2026-11-09T13:00:00Z",
	},
	machine: { cpu: 0.23, ram: { used: 6, total: 10 }, disk: { free: 1, total: 2 } },
	services: [],
	activity: { events: [], worklog: [], summaries: [] },
	leftovers: [],
};

test("today: header, needs you first, underway with state, time, cost and context, landed, next, news while rows remain, keys (0555)", () => {
	const lines = render({ up: true, today, hud, dashboard, selected: "w3:p1" }, now, 100, 30).map(text);
	expect(lines[0]).toBe("Mon 9 Nov 14:30 · 12° Partly cloudy · cpu 23% · mem 60% · 5h 84% · 7d 18% · $3.20 today");
	const order = ["NEEDS YOU", "UNDERWAY", "LANDED TODAY", "NEXT", "NEWS"].map((h) => lines.indexOf(h));
	expect(order).toEqual([...order].sort((a, b) => a - b));
	expect(order.every((i) => i > 0)).toBe(true);
	expect(lines).toContain("  acme-api/Retry — which backoff?");
	const working = lines.find((l) => l.includes("acme-api/Export v2"));
	expect(working).toStartWith("› ");
	expect(working).toEndWith("working  25 min  $1.50  83k");
	expect(lines.find((l) => l.includes("client-a/Cleanup"))).toEndWith("idle  1 h");
	expect(lines).toContain("  acme-api/Docs pass (merged into main as 1234567)");
	// Only what is still ahead today, all-day first.
	expect(lines).toContain("  all day Release week");
	expect(lines).toContain("  16:00 Review with client-a");
	expect(lines).not.toContain("  09:00 Standup");
	expect(lines).toContain("  Hacker News: A headline");
	expect(lines).toHaveLength(30);
	expect(lines.at(-1)).toBe(KEY_LINE);
	// No greeting.
	expect(lines.join("\n")).not.toMatch(/good (morning|afternoon|evening)/i);
});

test("today: news only while rows remain; a long list scrolls to keep the picked thread in view; his last key's outcome shows", () => {
	const short = render({ up: true, today, hud, dashboard }, now, 100, 15).map(text);
	expect(short).not.toContain("NEWS");
	expect(short).toHaveLength(15);
	const many: Today = {
		...today,
		underway: Array.from({ length: 30 }, (_, i) => ({
			paneId: `w${i}:p1`,
			title: `Thread ${i}`,
			project: "acme-api",
			line: "",
			state: "working" as const,
		})),
	};
	const lines = render(
		{
			up: true,
			today: many,
			hud,
			selected: "w25:p1",
			said: { text: "Stop: acme-api/Thread 25 isn't working.", isError: true },
		},
		now,
		60,
		16,
	).map(text);
	expect(lines.some((l) => l.startsWith("› acme-api/Thread 25"))).toBe(true);
	expect(lines.at(-2)).toBe("Stop: acme-api/Thread 25 isn't working.");
	expect(lines.every((l) => l.length <= 60)).toBe(true);
});

test("today: a service that doesn't answer says so; threads are picked in the pane's order; context reads short", () => {
	expect(text(render({ up: false }, now, 60, 5)[0] as Line)).toBe("Jocasta's service isn't answering; trying again…");
	expect(threadPanes(today)).toEqual(["w2:p1", "w3:p1", "w4:p1"]);
	expect(threadPanes(undefined)).toEqual([]);
	expect([tokens(950), tokens(83_400), tokens(1_250_000)]).toEqual(["950", "83k", "1.3M"]);
});
