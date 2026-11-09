import { expect, test } from "bun:test";
import { appendFileSync, mkdirSync, readFileSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Episode, startWatcher, type WatcherOptions } from "../../src/frontier/server/watcher.ts";
import { openCatalog } from "../../src/shared/schema.ts";
import { useTempJocasta } from "../shared/fixture.ts";

const env = useTempJocasta();

// Synthetic lines in omp 18.10.3's session shapes (see the header of src/frontier/server/watcher.ts).
const NOW = Date.parse("2026-10-13T12:00:00Z");
const PROJECT = "C:\\work\\demo";
const OUTSIDE = "C:\\scratch";

const titleSlot = (title: string) =>
	JSON.stringify({ type: "title", v: 1, title, source: "auto", updatedAt: "2026-10-13T10:00:00.000Z", pad: "" });
const header = (id: string, cwd: string) =>
	JSON.stringify({ type: "session", version: 3, id, timestamp: "2026-10-13T10:00:00.000Z", cwd, title: "demo" });
const user = (id: string, text: string) =>
	JSON.stringify({
		type: "message",
		id,
		parentId: null,
		timestamp: "2026-10-13T10:01:00.000Z",
		message: { role: "user", content: [{ type: "text", text }], timestamp: NOW },
	});
const assistant = (id: string, content: object[], stopReason: string) =>
	JSON.stringify({
		type: "message",
		id,
		parentId: "u1",
		timestamp: "2026-10-13T10:02:00.000Z",
		message: {
			role: "assistant",
			content,
			api: "anthropic-messages",
			provider: "anthropic",
			model: "model-a",
			usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2 },
			stopReason,
			timestamp: NOW,
		},
	});
const compaction = (id: string, summary: string, method: string | undefined, timestamp: string) =>
	JSON.stringify({
		type: "compaction",
		id,
		parentId: "a1",
		timestamp,
		summary,
		firstKeptEntryId: "u1",
		tokensBefore: 120000,
		tokensAfter: 4000,
		...(method ? { method } : {}),
		details: { readFiles: [], modifiedFiles: [] },
		fromExtension: false,
	});

function writeSession(dir: string, stem: string, lines: string[], mtime = NOW - 60_000): string {
	mkdirSync(dir, { recursive: true });
	const file = join(dir, `${stem}.jsonl`);
	writeFileSync(file, `${lines.join("\n")}\n`);
	utimesSync(file, mtime / 1000, mtime / 1000);
	return file;
}

interface Row {
	session_file: string;
	entry_id: string;
	kind: string;
	cwd: string;
	at: number;
	summary: string;
	sent: number;
}

function setup(post: WatcherOptions["post"]) {
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const stateFile = join(env.data, "watcher.json");
	const watcher = startWatcher({
		sessionsDir,
		stateFile,
		db,
		projectFor: (cwd) => (cwd.startsWith(PROJECT) ? { projectId: "p-demo" } : undefined),
		post,
		intervalMs: 3_600_000,
		now: () => NOW,
		onError: () => {},
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	const rows = () =>
		db
			.query<Row, []>(
				"SELECT session_file, entry_id, kind, cwd, at, summary, sent FROM summaries ORDER BY at, entry_id",
			)
			.all();
	return { db, sessionsDir, stateFile, watcher, rows };
}

test("soft and handoff compactions with text become rows; shake without text and subagent transcripts don't", async () => {
	const posted: [string, Episode][] = [];
	const { sessionsDir, stateFile, watcher, rows } = setup(async (projectId, episode) => {
		posted.push([projectId, episode]);
	});
	const cwdDir = join(sessionsDir, "--C--work--demo--");
	const stem = "2026-10-13T10-00-00-000Z_01a1";
	const file = writeSession(cwdDir, stem, [
		titleSlot("demo"),
		header("s1", PROJECT),
		user("u1", "add a README line"),
		assistant("a1", [{ type: "text", text: "Done." }], "stop"),
		compaction("c1", "Soft summary of the work.", "soft", "2026-10-13T10:05:00.000Z"),
		compaction("c2", "", "shake", "2026-10-13T10:06:00.000Z"),
		compaction("c3", "Handoff document.", "handoff", "2026-10-13T10:07:00.000Z"),
		compaction("c4", "Legacy summary.", undefined, "2026-10-13T10:08:00.000Z"),
	]);
	// A subagent's transcript in `<stem>/<AgentId>.jsonl` and an advisor's beside it.
	writeSession(join(cwdDir, stem), "0-Explore", [
		header("s2", PROJECT),
		compaction("x1", "Subagent summary.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	writeSession(join(cwdDir, stem), "__advisor", [
		header("s3", PROJECT),
		compaction("x2", "Advisor summary.", "soft", "2026-10-13T10:05:00.000Z"),
	]);

	await watcher.scan();

	expect(rows()).toEqual([
		{
			session_file: file,
			entry_id: "c1",
			kind: "compact",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:05:00.000Z"),
			summary: "Soft summary of the work.",
			sent: 1,
		},
		{
			session_file: file,
			entry_id: "c3",
			kind: "handoff",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:07:00.000Z"),
			summary: "Handoff document.",
			sent: 1,
		},
		{
			session_file: file,
			entry_id: "c4",
			kind: "compact",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:08:00.000Z"),
			summary: "Legacy summary.",
			sent: 1,
		},
	]);
	expect(posted).toEqual([
		[
			"p-demo",
			{
				name: `compact ${stem}`,
				body: "Soft summary of the work.",
				source: "omp-session",
				at: "2026-10-13T10:05:00.000Z",
			},
		],
		[
			"p-demo",
			{ name: `handoff ${stem}`, body: "Handoff document.", source: "omp-session", at: "2026-10-13T10:07:00.000Z" },
		],
		[
			"p-demo",
			{ name: `compact ${stem}`, body: "Legacy summary.", source: "omp-session", at: "2026-10-13T10:08:00.000Z" },
		],
	]);
	expect(JSON.parse(readFileSync(stateFile, "utf8"))).toMatchObject({ lastScan: NOW });

	// A second scan of the same file inserts nothing new and posts nothing again.
	utimesSync(file, NOW / 1000, NOW / 1000);
	await watcher.scan();
	expect(rows()).toHaveLength(3);
	expect(posted).toHaveLength(3);
});

test("summaries outside watched projects are stored but never posted", async () => {
	const posted: Episode[] = [];
	const { sessionsDir, watcher, rows } = setup(async (_, episode) => {
		posted.push(episode);
	});
	writeSession(join(sessionsDir, "--C--scratch--"), "2026-10-13T10-00-00-000Z_01a2", [
		titleSlot("scratch"),
		header("s1", OUTSIDE),
		compaction("c1", "Scratch summary.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	await watcher.scan();
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.cwd, r.sent])).toEqual([["c1", OUTSIDE, 0]]);
	expect(posted).toEqual([]);
});

test("a session moved to a sibling file gives its copied compactions once; its own new ones are rows (0464)", async () => {
	const posted: Episode[] = [];
	const { sessionsDir, watcher, rows } = setup(async (_, episode) => {
		posted.push(episode);
	});
	const cwdDir = join(sessionsDir, "--C--work--demo--");
	const old = writeSession(cwdDir, "2026-10-13T10-00-00-000Z_old1", [
		titleSlot("demo"),
		header("old1", PROJECT),
		compaction("c1", "Soft summary.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	const moved = writeSession(cwdDir, "2026-10-13T10-10-00-000Z_new2", [
		titleSlot("demo"),
		JSON.stringify({ type: "session", version: 3, id: "new2", parentSession: "old1", cwd: PROJECT }),
		compaction("c1", "Soft summary.", "soft", "2026-10-13T10:05:00.000Z"),
		JSON.stringify({ type: "model_usage", id: "u9", usage: { cost: { total: 0.1 } } }),
		JSON.stringify({ type: "memory_recall", id: "m9", text: "recalled" }),
		compaction("c2", "Handoff after the move.", "handoff", "2026-10-13T10:12:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.session_file, r.entry_id])).toEqual([
		[old, "c1"],
		[moved, "c2"],
	]);
	expect(posted.map((e) => e.body)).toEqual(["Soft summary.", "Handoff after the move."]);
});

test("a failing post leaves the row unsent, stops the scan's posting, and the retry waits", async () => {
	let down = true;
	let clock = NOW;
	const attempts: string[] = [];
	const posted: Episode[] = [];
	const errors: unknown[] = [];
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const stateFile = join(env.data, "watcher.json");
	const watcher = startWatcher({
		sessionsDir,
		stateFile,
		db,
		projectFor: () => ({ projectId: "p-demo" }),
		post: async (_, episode) => {
			attempts.push(episode.body);
			if (down) throw new Error("graphiti down");
			posted.push(episode);
		},
		intervalMs: 3_600_000,
		now: () => clock,
		onError: (e) => errors.push(e),
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	writeSession(join(sessionsDir, "--C--work--demo--"), "2026-10-13T10-00-00-000Z_01a3", [
		titleSlot("demo"),
		header("s1", PROJECT),
		compaction("c1", "Summary to retry.", "soft", "2026-10-13T10:05:00.000Z"),
		compaction("c2", "Second summary.", "soft", "2026-10-13T10:06:00.000Z"),
	]);
	await watcher.scan();
	const sent = () => db.query<{ sent: number }, []>("SELECT sent FROM summaries ORDER BY entry_id").all();
	expect(sent()).toEqual([{ sent: 0 }, { sent: 0 }]);
	// One attempt per scan: the second row isn't tried against a graph that is down (CR-13).
	expect(attempts).toEqual(["Summary to retry."]);
	expect(errors.map((e) => (e instanceof Error ? e.message : e)).join("\n")).toContain("graphiti down");
	expect(JSON.parse(readFileSync(stateFile, "utf8"))).toMatchObject({ lastScan: NOW });

	// Within the wait, a scan doesn't post at all.
	down = false;
	clock = NOW + 10_000;
	await watcher.scan();
	expect(attempts).toHaveLength(1);

	// After it, the unsent rows go; the file is older than lastScan, so it isn't reread.
	clock = NOW + 31_000;
	await watcher.scan();
	expect(sent()).toEqual([{ sent: 1 }, { sent: 1 }]);
	expect(posted.map((e) => e.body)).toEqual(["Summary to retry.", "Second summary."]);
});

test("a backlog is posted a few episodes per scan", async () => {
	const posted: Episode[] = [];
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const watcher = startWatcher({
		sessionsDir,
		stateFile: join(env.data, "watcher.json"),
		db,
		projectFor: () => ({ projectId: "p-demo" }),
		post: async (_, episode) => {
			posted.push(episode);
		},
		intervalMs: 3_600_000,
		now: () => NOW,
		onError: () => {},
		postsPerScan: 1,
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	writeSession(join(sessionsDir, "--C--work--demo--"), "backlog", [
		header("s1", PROJECT),
		...[1, 2, 3, 4].map((n) => compaction(`c${n}`, `Summary ${n}.`, "soft", `2026-10-13T10:0${n}:00.000Z`)),
	]);
	// The watcher's first scan (at its start) and this one: one episode each, oldest first.
	await watcher.scan();
	expect(posted.map((e) => e.body)).toEqual(["Summary 1.", "Summary 2."]);
	await watcher.scan();
	expect(posted.map((e) => e.body)).toEqual(["Summary 1.", "Summary 2.", "Summary 3."]);
});

test("remote and snapcompact summaries are stored but not sent to the graph (0400)", async () => {
	const posted: Episode[] = [];
	const { sessionsDir, watcher, rows } = setup(async (_, episode) => {
		posted.push(episode);
	});
	writeSession(join(sessionsDir, "--C--work--demo--"), "methods", [
		header("s1", PROJECT),
		compaction("c1", "Remote summary.", "remote", "2026-10-13T10:01:00.000Z"),
		compaction("c2", "Snap summary.", "snapcompact", "2026-10-13T10:02:00.000Z"),
		compaction("c3", "Soft summary.", "soft", "2026-10-13T10:03:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.sent])).toEqual([
		["c1", 1],
		["c2", 1],
		["c3", 1],
	]);
	expect(posted.map((e) => e.body)).toEqual(["Soft summary."]);
});

// A synthetic snapcompact transcript archive (omp's `snapcompact-summary.md` shape): a preamble, a scope legend naming
// an archived reasoning scope, a file list and a HISTORY section.
const ARCHIVE = [
	"Resume prior conversation. Earlier turns archived under HISTORY below, oldest→newest.",
	"",
	"Archived transcript scopes:",
	"- `¶user:`, `¶think:`, `¶ai:`, `¶call:`: user, assistant reasoning, assistant reply, tool call.",
	"",
	"FILES",
	"===================",
	"README.md (Read)",
	"",
	"HISTORY",
	"===================",
].join("\n");

test("snapcompact transcript archives never go to the graph, whatever their method", async () => {
	const posted: Episode[] = [];
	const { sessionsDir, watcher, rows } = setup(async (_, episode) => {
		posted.push(episode);
	});
	writeSession(join(sessionsDir, "--C--work--demo--"), "archives", [
		header("s1", PROJECT),
		compaction("c1", ARCHIVE, undefined, "2026-10-13T10:01:00.000Z"),
		compaction("c2", ARCHIVE, "soft", "2026-10-13T10:02:00.000Z"),
		compaction("c3", "Soft summary.", "soft", "2026-10-13T10:03:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.sent])).toEqual([
		["c1", 1],
		["c2", 1],
		["c3", 1],
	]);
	expect(posted.map((e) => e.body)).toEqual(["Soft summary."]);
});

test("an archive already waiting in catalog.db is marked sent without a post and no longer holds the queue", async () => {
	const seed = openCatalog();
	const insert = seed.query(
		"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, sent) VALUES (?, ?, 'compact', ?, ?, ?, 0)",
	);
	insert.run("old.jsonl", "a1", PROJECT, NOW - 7_200_000, ARCHIVE);
	insert.run("old.jsonl", "a2", PROJECT, NOW - 3_600_000, "Older soft summary.");
	seed.close();
	const posted: Episode[] = [];
	const { watcher, rows } = setup(async (_, episode) => {
		posted.push(episode);
	});
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.sent])).toEqual([
		["a1", 1],
		["a2", 1],
	]);
	expect(posted.map((e) => e.body)).toEqual(["Older soft summary."]);
});

test("a modified file is read from where the last scan stopped (CR-03)", async () => {
	let clock = NOW;
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const stateFile = join(env.data, "watcher.json");
	const watcher = startWatcher({
		sessionsDir,
		stateFile,
		db,
		projectFor: () => undefined,
		post: async () => {},
		intervalMs: 3_600_000,
		now: () => clock,
		onError: () => {},
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	const file = writeSession(join(sessionsDir, "--C--work--demo--"), "grow", [
		header("s1", PROJECT),
		compaction("c1", "First.", "soft", "2026-10-13T10:01:00.000Z"),
	]);
	await watcher.scan();
	const offset = JSON.parse(readFileSync(stateFile, "utf8")).files[file].offset;
	expect(offset).toBe(readFileSync(file).length);
	// The rows already read are gone from the table; only the appended line comes back.
	db.run("DELETE FROM summaries");
	appendFileSync(file, `${compaction("c2", "Second.", "soft", "2026-10-13T10:02:00.000Z")}\n`);
	clock = NOW + 60_000;
	utimesSync(file, clock / 1000, clock / 1000);
	await watcher.scan();
	const rows = db.query<{ entry_id: string; cwd: string }, []>("SELECT entry_id, cwd FROM summaries").all();
	expect(rows).toEqual([{ entry_id: "c2", cwd: PROJECT }]);
});

test("files not modified since lastScan are not reread; modified ones are", async () => {
	const { sessionsDir, stateFile, watcher, rows } = setup(async () => {});
	mkdirSync(env.data, { recursive: true });
	writeFileSync(stateFile, `${JSON.stringify({ lastScan: NOW - 30_000 })}\n`);
	const dir = join(sessionsDir, "--C--work--demo--");
	writeSession(
		dir,
		"old",
		[header("s1", PROJECT), compaction("c1", "Old summary.", "soft", "2026-10-13T09:00:00.000Z")],
		NOW - 60_000,
	);
	writeSession(
		dir,
		"new",
		[header("s2", PROJECT), compaction("c2", "New summary.", "soft", "2026-10-13T11:59:50.000Z")],
		NOW - 10_000,
	);
	await watcher.scan();
	expect(rows().map((r) => r.summary)).toEqual(["New summary."]);
});

test("a summary posts to the project stored with it once its cwd folder is gone (0440 (4))", async () => {
	const seed = openCatalog();
	// A row from before v7: its project is resolved at its first post attempt and stored.
	seed
		.query(
			"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, sent) VALUES ('old.jsonl', 'a1', 'compact', ?, ?, 'Older summary.', 0)",
		)
		.run(PROJECT, NOW - 7_200_000);
	seed.close();
	let clock = NOW;
	let gone = false;
	let down = true;
	const posted: [string, string][] = [];
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const watcher = startWatcher({
		sessionsDir,
		stateFile: join(env.data, "watcher.json"),
		db,
		projectFor: (cwd) => (!gone && cwd.startsWith(PROJECT) ? { projectId: "p-demo" } : undefined),
		post: async (projectId, episode) => {
			if (down) throw new Error("graphiti down");
			posted.push([projectId, episode.body]);
		},
		intervalMs: 3_600_000,
		now: () => clock,
		onError: () => {},
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	writeSession(join(sessionsDir, "--C--work--demo--"), "kept", [
		header("s1", PROJECT),
		compaction("c1", "Kept summary.", "soft", "2026-10-13T10:01:00.000Z"),
	]);
	await watcher.scan();
	const projects = () =>
		db
			.query<{ entry_id: string; project_id: string | null }, []>(
				"SELECT entry_id, project_id FROM summaries ORDER BY at",
			)
			.all();
	expect(projects()).toEqual([
		{ entry_id: "a1", project_id: "p-demo" },
		{ entry_id: "c1", project_id: "p-demo" },
	]);
	expect(posted).toEqual([]);

	// The folder is gone: `projectFor` no longer knows the cwd, the stored project still takes the posts.
	gone = true;
	down = false;
	clock = NOW + 31_000;
	await watcher.scan();
	expect(posted).toEqual([
		["p-demo", "Older summary."],
		["p-demo", "Kept summary."],
	]);
});

test("scans that change nothing leave watcher.json alone", async () => {
	const { sessionsDir, stateFile, watcher } = setup(async () => {});
	writeSession(join(sessionsDir, "--C--work--demo--"), "still", [
		header("s1", PROJECT),
		compaction("c1", "Summary.", "soft", "2026-10-13T10:01:00.000Z"),
	]);
	await watcher.scan();
	const offsets = readFileSync(stateFile, "utf8");
	const old = (NOW - 3_600_000) / 1000;
	utimesSync(stateFile, old, old);
	await watcher.scan();
	await watcher.scan();
	expect(statSync(stateFile).mtimeMs).toBe(old * 1000);
	expect(readFileSync(stateFile, "utf8")).toBe(offsets);
});
