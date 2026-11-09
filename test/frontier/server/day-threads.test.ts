// What his day shows about threads and quota (0456) and the weekly review's schedule (0470), on a Day with stand-in
// hooks: the service fills them from herdr and git.
import { expect, test } from "bun:test";
import type { Leftover, ServerMessage } from "../../../src/frontier/protocol.ts";
import { Day, type DayThread } from "../../../src/frontier/server/day.ts";
import type { Project } from "../../../src/shared/projects.ts";
import { openCatalog } from "../../../src/shared/schema.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { writeSettings } from "./helpers.ts";

const env = useTempJocasta();

const HOUR = 3_600_000;

function project(id: string, name: string): Project {
	return { id, identity: `root:${id}`, name, mainPath: `/code/${name}`, git: true, paths: [] };
}

interface Stand {
	day: Day;
	sent: ServerMessage[];
	alerts: string[];
	threads: DayThread[];
	leftovers: Leftover[];
	reviews: string[];
	review: (p: Project) => Promise<string | null>;
	usage: string;
	at: Date;
}

/** A Day on a fake clock with stand-in hooks; stopped when the test ends. */
function stand(at: Date, projects: Project[] = [project("p1", "acme-api")]): Stand {
	const catalog = openCatalog();
	const s: Stand = {
		sent: [],
		alerts: [],
		threads: [],
		leftovers: [],
		reviews: [],
		review: async () => null,
		usage: JSON.stringify({ reports: [] }),
		at,
	} as unknown as Stand;
	s.day = new Day(
		catalog,
		{
			send: (m) => s.sent.push(m),
			alert: (text) => s.alerts.push(text),
			notify: () => {},
			projects: () => projects,
			threads: () => s.threads,
			sessionsByProject: () => new Map(),
			prepareSessions: async () => {},
			services: async () => [],
			projectName: () => null,
			leftovers: async () => s.leftovers,
			weeklyReview: (p) => {
				s.reviews.push(p.name);
				return s.review(p);
			},
		},
		{ pollers: false, now: () => s.at, usage: async () => s.usage },
	);
	env.defer(() => {
		void s.day.stop();
		catalog.close();
	});
	return s;
}

const thread = (over: Partial<DayThread>): DayThread => ({
	paneId: "w1:p1",
	title: "t",
	project: "acme-api",
	line: "",
	state: "working",
	changedAt: 0,
	...over,
});

const toasts = (s: Stand) => s.sent.filter((m): m is Extract<ServerMessage, { type: "toast" }> => m.type === "toast");

test("Awaiting you lists reviewed, unfinished threads oldest first with their wait, past a day too; Landed keeps only unreviewed ones (0456)", () => {
	const now = new Date(2026, 9, 23, 14, 0);
	const s = stand(now);
	s.threads = [
		thread({
			paneId: "a",
			title: "Export v1",
			state: "done",
			line: "done and reviewed: no notes",
			reviewedAt: now.getTime() - 3 * HOUR,
			changedAt: now.getTime() - 3 * HOUR,
		}),
		thread({
			paneId: "b",
			title: "Docs pass",
			state: "done",
			line: "done and reviewed: 2 notes",
			reviewedAt: now.getTime() - 50 * HOUR,
			changedAt: now.getTime() - 50 * HOUR,
		}),
		thread({ paneId: "c", title: "Retry", state: "done", line: "in review", changedAt: now.getTime() - HOUR }),
		thread({ paneId: "d", title: "Export v2", state: "working", line: "writing tests", overlaps: ["Export v1"] }),
	];
	const today = s.day.today();
	expect(today.needsYou).toEqual([
		{
			kind: "reviewed",
			text: "Docs pass — done and reviewed: 2 notes",
			since: new Date(now.getTime() - 50 * HOUR).toISOString(),
			paneId: "b",
			project: "acme-api",
		},
		{
			kind: "reviewed",
			text: "Export v1 — done and reviewed: no notes",
			since: new Date(now.getTime() - 3 * HOUR).toISOString(),
			paneId: "a",
			project: "acme-api",
		},
	]);
	expect(today.landed.map((t) => t.paneId)).toEqual(["c"]);
	// Overlaps travel with the thread wherever it appears; Underway carries its state and since when (0555).
	expect(today.underway).toEqual([
		{
			paneId: "d",
			title: "Export v2",
			project: "acme-api",
			line: "writing tests",
			overlaps: ["Export v1"],
			state: "working",
			since: expect.any(Number),
		},
	]);
	const text = s.day.todayText();
	expect(text).toContain(
		"Awaiting you acme-api/Docs pass — done and reviewed: 2 notes (waiting 2 d) · acme-api/Export v1 — done and reviewed: no notes (waiting 3 h)",
	);
	expect(text).toContain("Underway     acme-api/Export v2 (overlaps Export v1)");
});

test("a thread merged or sent as a PR stays in Landed since yesterday, saying how it landed, across a restart (0263, 0300)", () => {
	const now = new Date(2026, 10, 24, 17, 0);
	const s = stand(new Date(now.getTime() - 50 * HOUR), [project("p1", "demo"), project("p2", "client-a")]);
	s.day.landed("w0:p1", "Old work", "p1", "merged into main as 1111111");
	s.at = new Date(now.getTime() - 2 * HOUR);
	s.day.landed("w4:p1", "Add subtract", "p1", "merged into main as 6796653");
	s.at = new Date(now.getTime() - HOUR);
	s.day.landed("w6:p1", "Add farewell — v2", "p2", "PR open");
	s.at = now;
	s.threads = [
		thread({
			paneId: "w5:p1",
			title: "Add multiply",
			project: "demo",
			state: "done",
			line: "in review",
			changedAt: now.getTime() - HOUR,
		}),
	];
	const landed = [
		{ paneId: "w5:p1", title: "Add multiply", project: "demo", line: "in review" },
		{ title: "Add subtract", project: "demo", line: "merged into main as 6796653" },
		{ title: "Add farewell — v2", project: "client-a", line: "PR open" },
	];
	expect(s.day.today().landed).toEqual(landed);
	expect(s.day.todayText()).toContain(
		"Landed       demo/Add multiply · demo/Add subtract (merged into main as 6796653) · client-a/Add farewell — v2 (PR open)",
	);
	// The page heard of each as it landed.
	const pushed = s.sent.filter((m): m is Extract<ServerMessage, { type: "today" }> => m.type === "today").at(-1);
	expect(pushed?.today.landed).toEqual(landed.slice(1));
	// A restart: the catalog keeps them.
	const again = stand(now, [project("p1", "demo"), project("p2", "client-a")]);
	expect(again.day.today().landed).toEqual(landed.slice(1));
});

test("thread lines held in quiet hours wait in the queue across a restart and go to the boss oldest first once they end (0298, 0470)", () => {
	writeSettings(env.config, { frontier: { quietHours: "22:00-08:00" } });
	const night = stand(new Date(2026, 9, 24, 23, 0));
	night.day.hold("client-a/Add farewell — he committed by hand on its branch (1 commit) · offer once to save a lesson");
	night.at = new Date(2026, 9, 24, 23, 30);
	night.day.hold("demo/Add divide — finished its turn");
	night.day.tick();
	expect(night.alerts).toEqual([]);
	// Restarted before morning: the queue kept them.
	const morning = stand(new Date(2026, 9, 25, 7, 59));
	morning.day.tick();
	expect(morning.alerts).toEqual([]);
	morning.at = new Date(2026, 9, 25, 8, 0);
	morning.day.tick();
	morning.day.tick();
	expect(morning.alerts).toEqual([
		"client-a/Add farewell — he committed by hand on its branch (1 commit) · offer once to save a lesson",
		"demo/Add divide — finished its turn",
	]);
});

test("the quota: one toast when a window passes 80%, again only after it went back under; a failing omp usage is quiet (0456)", async () => {
	const s = stand(new Date(2026, 9, 23, 14, 0));
	const window = (used: number) =>
		JSON.stringify({ reports: [{ limits: [{ window: { id: "5h" }, amount: { usedFraction: used } }] }] });
	s.usage = window(0.5);
	await s.day.readQuota();
	expect(s.day.quota()).toEqual([{ window: "5h", used: 0.5 }]);
	expect(toasts(s)).toEqual([]);
	s.usage = window(0.84);
	await s.day.readQuota();
	await s.day.readQuota();
	expect(toasts(s)).toEqual([
		{ type: "toast", reason: "event", level: "warning", priority: "urgent", text: "Subscription quota: 5h at 84%" },
	]);
	s.usage = window(0.1);
	await s.day.readQuota();
	s.usage = window(0.9);
	await s.day.readQuota();
	expect(toasts(s).map((t) => t.text)).toEqual(["Subscription quota: 5h at 84%", "Subscription quota: 5h at 90%"]);
	// omp missing or logged out: the last windows stay, nothing is toasted.
	s.usage = "not json";
	await s.day.readQuota();
	expect(s.day.quota()).toEqual([{ window: "5h", used: 0.9 }]);
	expect(toasts(s)).toHaveLength(2);
});

test("a quota that reads 80% is past the threshold: 0.796 gets its toast, saying 80% (0456)", async () => {
	const s = stand(new Date(2026, 9, 23, 14, 0));
	s.usage = JSON.stringify({ reports: [{ limits: [{ window: { id: "5h" }, amount: { usedFraction: 0.796 } }] }] });
	await s.day.readQuota();
	expect(toasts(s).map((t) => t.text)).toEqual(["Subscription quota: 5h at 80%"]);
});

test("a due reminder filed for a project keeps the project on its Awaiting-you row; one without stays bare (0432)", () => {
	const s = stand(new Date(2026, 9, 23, 9, 0), [project("p1", "acme-api")]);
	s.day.remind("Stretch and check the build", "09:30", project("p1", "acme-api"));
	s.day.remind("call the bank", "09:45");
	s.at = new Date(2026, 9, 23, 10, 0);
	expect(s.day.today().needsYou).toEqual([
		{ kind: "reminder", text: "Stretch and check the build", time: "09:30", project: "acme-api" },
		{ kind: "reminder", text: "call the bank", time: "09:45" },
	]);
});

test("leftover worktrees reach the dashboard as the service lists them (0456)", async () => {
	const s = stand(new Date(2026, 9, 23, 14, 0));
	expect(s.day.dashboard().leftovers).toEqual([]);
	s.leftovers = [{ project: "acme-api", folder: "/code/acme-api.wt/retry", branch: "retry", ahead: 2, dirty: 1 }];
	await s.day.refreshLeftovers();
	expect(s.day.dashboard().leftovers).toEqual(s.leftovers);
	const pushed = s.sent.filter((m) => m.type === "dashboard").at(-1);
	expect(pushed?.type === "dashboard" && pushed.dashboard.leftovers).toEqual(s.leftovers);
});

test("the weekly review runs from Monday 10:00 outside quiet hours, once per project and week; its result is an event (0470)", async () => {
	const p1 = project("p1", "acme-api");
	const p2 = project("p2", "client-a");
	// Monday 26 Oct 2026, 09:30.
	const s = stand(new Date(2026, 9, 26, 9, 30), [p1, p2]);
	s.review = async (p) => (p.id === "p1" ? "Two modules now parse dates their own way." : null);
	s.day.tick();
	expect(s.reviews).toEqual([]);
	s.at = new Date(2026, 9, 26, 10, 0);
	s.day.tick();
	await Bun.sleep(0);
	expect(s.reviews).toEqual(["acme-api", "client-a"]);
	expect(s.alerts).toEqual(["Weekly review: acme-api — Two modules now parse dates their own way."]);
	// Delivered once: never reminded of, and not run again this week, even by a new Day (a restart).
	s.at = new Date(2026, 9, 27, 11, 0);
	s.day.tick();
	await Bun.sleep(0);
	expect(s.reviews).toEqual(["acme-api", "client-a"]);
	expect(s.alerts).toHaveLength(1);
	expect(s.day.queue.open().filter((r) => r.kind === "weekly-review")).toEqual([]);
	const again = stand(new Date(2026, 9, 28, 12, 0), [p1]);
	again.review = s.review;
	again.day.tick();
	await Bun.sleep(0);
	expect(again.reviews).toEqual([]);
	// The next week runs again.
	s.at = new Date(2026, 10, 2, 10, 5);
	s.day.tick();
	await Bun.sleep(0);
	expect(s.reviews).toEqual(["acme-api", "client-a", "acme-api", "client-a"]);
});
