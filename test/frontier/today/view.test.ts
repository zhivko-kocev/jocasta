import { expect, test } from "bun:test";
import type { Dashboard, Hud, Today } from "../../../src/frontier/protocol.ts";
import { KEY_LINE, render, SAID_MS, type TodayState, tokens } from "../../../src/frontier/today/view.ts";
import { columns, type Line } from "../../../src/frontier/tui.ts";

const now = new Date(2026, 10, 9, 14, 30);
const text = (line: Line) => line.map((s) => s.text).join("");
const lines = (state: TodayState, width: number, height: number, at = now) =>
	render(state, at, width, height).lines.map(text);

const today: Today = {
	date: "2026-11-09",
	calendar: [
		{ time: "all day", title: "Release week" },
		{ time: "09:00", title: "Standup" },
		{ time: "14:00", end: "15:00", title: "Client-a sync", url: "https://meet.example.org/a" },
		{ time: "16:00", title: "Review with client-a" },
	],
	needsYou: [
		{ kind: "blocked", text: "Retry — which backoff?", paneId: "w2:p1", project: "acme-api" },
		{ kind: "review", text: "example-org/web#12 — review requested", url: "https://example.org/web/pull/12" },
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
	next: [{ id: 7, text: "look at the slow search endpoint", project: "acme-api" }],
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

test("today: header, needs you first with kind glyphs, underway, landed, calendar, his notes and news while rows remain, keys (0555, 0581)", () => {
	const shown = lines({ up: true, today, hud, dashboard, selected: "w3:p1" }, 100, 34);
	expect(shown[0]).toBe("Mon 9 Nov 14:30 · 12° Partly cloudy · cpu 23% · mem 60% · 5h 84% · 7d 18% · $3.20 today");
	const order = ["NEEDS YOU", "UNDERWAY", "LANDED TODAY", "CALENDAR", "NEXT", "NEWS"].map((h) => shown.indexOf(h));
	expect(order).toEqual([...order].sort((a, b) => a - b));
	expect(order.every((i) => i > 0)).toBe(true);
	expect(shown).toContain("  ? acme-api/Retry — which backoff?");
	// A review request's glyph says it opens.
	expect(shown).toContain("  ↗ example-org/web#12 — review requested");
	const working = shown.find((l) => l.includes("acme-api/Export v2"));
	expect(working).toStartWith("› ");
	expect(working).toEndWith("working  25 min  $1.50  83k");
	expect(shown.find((l) => l.includes("client-a/Cleanup"))).toEndWith("idle  1 h");
	expect(shown).toContain("  acme-api/Docs pass (merged into main as 1234567)");
	// What is still ahead, all-day first; a started meeting stays as "now" until its end, its link marked.
	expect(shown).toContain("  all day Release week");
	expect(shown).toContain("  now   Client-a sync ↗");
	expect(shown).toContain("  16:00 Review with client-a");
	expect(shown).not.toContain("  09:00 Standup");
	expect(shown).toContain("  acme-api: look at the slow search endpoint (#7)");
	expect(shown).toContain("  Hacker News: A headline ↗");
	expect(shown).toHaveLength(34);
	expect(shown.at(-1)).toBe(KEY_LINE);
	expect(KEY_LINE).toBe("↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New");
	// No greeting.
	expect(shown.join("\n")).not.toMatch(/good (morning|afternoon|evening)/i);
	// The meeting ended: gone.
	expect(lines({ up: true, today, hud }, 100, 34, new Date(2026, 10, 9, 15, 0))).not.toContain(
		"  now   Client-a sync ↗",
	);
});

test("today: rows Enter acts on, in drawn order: threads go to their pane, link rows open (0581)", () => {
	const { picks } = render({ up: true, today, hud, dashboard }, now, 100, 34);
	expect(picks.map((p) => p.key)).toEqual([
		"w2:p1",
		"https://example.org/web/pull/12",
		"w3:p1",
		"w4:p1",
		"https://meet.example.org/a",
		"https://example.org/1",
		"https://example.org/2",
	]);
	expect(picks[0]).toEqual({ key: "w2:p1", paneId: "w2:p1" });
	expect(picks[1]).toEqual({ key: "https://example.org/web/pull/12", url: "https://example.org/web/pull/12" });
	// A picked link row carries the marker like a thread.
	const shown = lines({ up: true, today, hud, dashboard, selected: "https://meet.example.org/a" }, 100, 34);
	expect(shown.find((l) => l.includes("Client-a sync"))).toStartWith("› now   Client-a sync ↗");
	// News only in spare rows: the shorter pane draws none and offers none.
	expect(
		render({ up: true, today, hud, dashboard }, now, 100, 16).picks.some((p) => p.key.includes("example.org/1")),
	).toBe(false);
});

test("today: news only while rows remain; the list scrolls only as far as the picked row needs; his last key's outcome", () => {
	const short = lines({ up: true, today, hud, dashboard }, 100, 15);
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
	const said = { text: "Stop: acme-api/Thread 25 isn't working.", isError: true, at: now.getTime() };
	const shown = lines({ up: true, today: many, hud, selected: "w25:p1", said }, 60, 16);
	expect(shown.some((l) => l.startsWith("› acme-api/Thread 25"))).toBe(true);
	expect(shown.at(-2)).toBe("Stop: acme-api/Thread 25 isn't working.");
	expect(shown.every((l) => columns(l) <= 60)).toBe(true);
	// A picked row already in view: no scroll, NEEDS YOU keeps its heading.
	const top = lines({ up: true, today: many, hud, selected: "w1:p1" }, 60, 16);
	expect(top).toContain("NEEDS YOU");
	expect(top.some((l) => l.startsWith("› acme-api/Thread 1 "))).toBe(true);
	// The outcome wraps over two rows at most, and goes after a minute.
	const long = {
		text: `${"Started acme-api/Fix the flaky login test: ".repeat(4)}end.`,
		isError: false,
		at: now.getTime(),
	};
	const wrapped = lines({ up: true, today, hud, said: long }, 40, 20);
	expect(wrapped.at(-3)).toStartWith("Started acme-api/Fix the flaky login");
	expect(wrapped.at(-2)).toEndWith("…");
	const later = new Date(now.getTime() + SAID_MS);
	expect(lines({ up: true, today, hud, said: long }, 40, 20, later).join("\n")).not.toContain("Started");
});

test("today at 40 columns: titles keep 18 columns before facts give way, the header yields to two rows, the key line drops keys (0581)", () => {
	const hudHigh: Hud = {
		...hud,
		spendToday: 48.73,
		quota: [
			{ window: "5h", used: 0.91, resetsAt: new Date(2026, 10, 9, 16, 30).toISOString() },
			{ window: "7d", used: 0.97, resetsAt: new Date(2026, 10, 13, 9, 0).toISOString() },
		],
	};
	const narrow: Today = {
		...today,
		underway: [
			{
				paneId: "w3:p1",
				title: "Fix login redirect loop",
				project: "acme-api",
				line: "",
				state: "working",
				since: now.getTime() - 12 * 60_000,
				cost: 0.84,
				context: 83_000,
			},
			{
				paneId: "w5:p1",
				title: "修复导出重试的退避逻辑",
				project: "acme-api",
				line: "",
				state: "working",
				since: now.getTime() - 48 * 60_000,
			},
		],
	};
	const machineDash: Dashboard = {
		...dashboard,
		machine: { cpu: 0.07, ram: { used: 6, total: 10 }, disk: { free: 1, total: 2 } },
		news: [],
	};
	const shown = lines({ up: true, today: narrow, hud: hudHigh, dashboard: machineDash }, 40, 30);
	expect(shown.every((l) => columns(l) <= 40)).toBe(true);
	expect(shown.slice(0, 2)).toEqual(["Mon 9 Nov 14:30 · cpu 7% · mem 60%", "5h 91% (16:30) · 7d 97% · $48.73 today"]);
	expect(shown).toContain("  acme-api/Fix login re… working  12 min");
	const wide = shown.find((l) => l.includes("修复"));
	expect(wide).toEndWith("working  48 min");
	expect(shown.at(-1)).toBe("↑↓ pick · Enter go/open · f Finish");
	// Every line fits from 20 to 120 columns.
	for (const width of [20, 30, 35, 60, 120]) {
		const at = lines({ up: true, today: narrow, hud: hudHigh, dashboard }, width, 24);
		expect(at.every((l) => columns(l) <= width)).toBe(true);
		expect(at).toHaveLength(24);
	}
});

test("today: connecting at first; offline keeps the time and says since when; quiet hours and a broken service in the header; empty states hint the key (0581)", () => {
	expect(lines({ up: false }, 60, 5)[0]).toBe("Connecting to Jocasta…");
	const down = new Date(2026, 10, 9, 14, 28).getTime();
	expect(lines({ up: false, seen: true, downSince: down }, 80, 5)[0]).toBe(
		"14:30 · Jocasta isn't answering (since 14:28); trying again…",
	);
	const services: Dashboard["services"] = [
		{ name: "Graphiti", state: "down", note: "Docker isn't running" },
		{ name: "whiteboard", state: "up" },
	];
	const header = lines({ up: true, hud: { ...hud, quiet: true }, dashboard: { ...dashboard, services } }, 200, 5)[0];
	expect(header).toStartWith("Mon 9 Nov 14:30 · quiet · Graphiti down (Docker isn't running) · ");
	const empty: Today = { ...today, needsYou: [], underway: [], landed: [], calendar: [], next: [] };
	const shown = lines({ up: true, today: empty, hud }, 60, 14);
	expect(shown).toContain("  nothing waits on you");
	expect(shown).toContain("  no thread at work · n starts one");
	expect(shown).toContain("  nothing more on the calendar today");
	expect(shown).not.toContain("NEXT");
	expect([tokens(950), tokens(83_400), tokens(1_250_000)]).toEqual(["950", "83k", "1.3M"]);
});
