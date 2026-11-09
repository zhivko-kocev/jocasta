import { expect, test } from "bun:test";
import { Inbox } from "../../src/shared/events/inbox.ts";
import { EventQueue, type NewEvent } from "../../src/shared/events/queue.ts";
import { isQuiet, parseQuietHours } from "../../src/shared/events/quiet.ts";
import { CATALOG_MIGRATIONS, openCatalog, openMeDb } from "../../src/shared/schema.ts";
import { useTempJocasta } from "../shared/fixture.ts";

useTempJocasta();

const review = (n: number): NewEvent => ({
	sourceId: `github:review:acme/api#${n}`,
	source: "github",
	kind: "review",
	priority: "normal",
	text: `review requested: change ${n} (acme/api #${n})`,
	url: `https://github.com/acme/api/pull/${n}`,
});

test("catalog.db v5 is the event queue (0298, 0340's ledger)", () => {
	expect(CATALOG_MIGRATIONS[4]).toContain("CREATE TABLE events");
	const db = openCatalog();
	try {
		expect(db.query<{ user_version: number }, []>("PRAGMA user_version").get()?.user_version).toBe(
			CATALOG_MIGRATIONS.length,
		);
	} finally {
		db.close();
	}
});

test("the queue deduplicates by source id; a poller's set resolves what went away; a cause that comes back is new (0298)", () => {
	const db = openCatalog();
	let now = new Date("2026-10-08T09:00:00Z");
	const queue = new EventQueue(db, () => now);
	try {
		expect(queue.add(review(1))?.id).toBeNumber();
		expect(queue.add(review(1))).toBeUndefined();
		expect(queue.sync("github", ["review", "pipeline"], [review(1), review(2)]).map((r) => r.source_id)).toEqual([
			"github:review:acme/api#2",
		]);
		// Review 1 is done: it leaves the set and is resolved.
		expect(queue.sync("github", ["review"], [review(2)])).toEqual([]);
		expect(queue.open().map((r) => r.source_id)).toEqual(["github:review:acme/api#2"]);
		expect(queue.bySource("github:review:acme/api#1")?.resolved_at).not.toBeNull();
		// Requested again later: a fresh event, so it is raised again.
		expect(queue.sync("github", ["review"], [review(1), review(2)])).toHaveLength(1);

		// The one reminder (0303): raised, unresolved, 30 minutes old, not yet reminded.
		const row = queue.bySource("github:review:acme/api#2");
		queue.markRaised(row?.id as number);
		expect(queue.dueReminders(30 * 60_000)).toEqual([]);
		now = new Date(now.getTime() + 30 * 60_000);
		expect(queue.dueReminders(30 * 60_000).map((r) => r.id)).toEqual([row?.id as number]);
		queue.markReminded(row?.id as number);
		expect(queue.dueReminders(30 * 60_000)).toEqual([]);
	} finally {
		db.close();
	}
});

test("quiet hours parse like 22:00-08:00, wrap past midnight, and a bad value names the key (0298)", () => {
	const quiet = parseQuietHours("22:00-08:00");
	const at = (h: number, m = 0) => new Date(2026, 9, 8, h, m);
	expect(isQuiet(quiet, at(23))).toBe(true);
	expect(isQuiet(quiet, at(7, 59))).toBe(true);
	expect(isQuiet(quiet, at(8))).toBe(false);
	expect(isQuiet(quiet, at(21, 59))).toBe(false);
	expect(isQuiet(parseQuietHours("13:00-14:00"), at(13, 30))).toBe(true);
	expect(isQuiet(parseQuietHours(undefined), at(23))).toBe(false);
	expect(() => parseQuietHours("late")).toThrow("frontier.quietHours");
	expect(() => parseQuietHours("25:00-08:00")).toThrow("frontier.quietHours");
});

test("the inbox holds notes for a project's next work and reminders at a time (0302, me.db v1)", () => {
	const db = openMeDb();
	const now = new Date(2026, 9, 8, 9, 0);
	const inbox = new Inbox(db, () => now);
	try {
		const note = inbox.add("try the new parser", { projectId: "p1" });
		const reminder = inbox.add("call the bank", { dueAt: new Date(2026, 9, 8, 15, 0) });
		expect(() => inbox.add("nowhere", {})).toThrow("needs a project or a time");
		expect(inbox.notes("p1").map((n) => n.text)).toEqual(["try the new parser"]);
		expect(inbox.reminders(new Date(2026, 9, 8), new Date(2026, 9, 9)).map((r) => r.id)).toEqual([reminder.id]);
		expect(inbox.due()).toEqual([]);
		expect(inbox.done(note.id)).toBe(true);
		expect(inbox.done(note.id)).toBe(false);
		expect(inbox.notes("p1")).toEqual([]);
	} finally {
		db.close();
	}
});
