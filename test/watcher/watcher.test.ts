import { expect, test } from "bun:test";
import { appendFileSync, mkdirSync, readFileSync, renameSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { searchHistory } from "../../src/frontier/server/history.ts";
import { startWatcher } from "../../src/frontier/server/watcher.ts";
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
	indexed: number;
}

function setup() {
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const stateFile = join(env.data, "watcher.json");
	const watcher = startWatcher({
		sessionsDir,
		stateFile,
		db,
		projectFor: (cwd) => (cwd.startsWith(PROJECT) ? { projectId: "p-demo" } : undefined),
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
				"SELECT session_file, entry_id, kind, cwd, at, summary, indexed FROM summaries ORDER BY at, entry_id",
			)
			.all();
	/** Everything the history holds, whatever its project. */
	const history = () =>
		db
			.query<{ project_id: string; source: string; text: string; at: number }, []>(
				"SELECT project_id, source, text, at FROM history ORDER BY at, text",
			)
			.all();
	return { db, sessionsDir, stateFile, watcher, rows, history };
}

test("soft and handoff compactions with text become rows and history lines; shake without text and subagent transcripts don't", async () => {
	const { sessionsDir, stateFile, watcher, rows, history } = setup();
	const cwdDir = join(sessionsDir, "--C--work--demo--");
	const stem = "2026-10-13T10-00-00-000Z_01a1";
	const file = writeSession(cwdDir, stem, [
		titleSlot("demo"),
		header("s1", PROJECT),
		user("u1", "add a README line"),
		assistant("a1", [{ type: "text", text: "Done." }], "stop"),
		compaction("c1", "Soft summary of the work done.\nShort.", "soft", "2026-10-13T10:05:00.000Z"),
		compaction("c2", "", "shake", "2026-10-13T10:06:00.000Z"),
		compaction("c3", "Handoff document for the next run.", "handoff", "2026-10-13T10:07:00.000Z"),
		compaction("c4", "Legacy summary of the earlier work.", undefined, "2026-10-13T10:08:00.000Z"),
	]);
	// A subagent's transcript in `<stem>/<AgentId>.jsonl` and an advisor's beside it.
	writeSession(join(cwdDir, stem), "0-Explore", [
		header("s2", PROJECT),
		compaction("x1", "Subagent summary of its search.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	writeSession(join(cwdDir, stem), "__advisor", [
		header("s3", PROJECT),
		compaction("x2", "Advisor summary of the whole run.", "soft", "2026-10-13T10:05:00.000Z"),
	]);

	await watcher.scan();

	expect(rows()).toEqual([
		{
			session_file: file,
			entry_id: "c1",
			kind: "compact",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:05:00.000Z"),
			summary: "Soft summary of the work done.\nShort.",
			indexed: 1,
		},
		{
			session_file: file,
			entry_id: "c3",
			kind: "handoff",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:07:00.000Z"),
			summary: "Handoff document for the next run.",
			indexed: 1,
		},
		{
			session_file: file,
			entry_id: "c4",
			kind: "compact",
			cwd: PROJECT,
			at: Date.parse("2026-10-13T10:08:00.000Z"),
			summary: "Legacy summary of the earlier work.",
			indexed: 1,
		},
	]);
	// Only lines long enough to find: the 6-character "Short." stays out.
	expect(history()).toEqual([
		{
			project_id: "p-demo",
			source: `compact ${stem}`,
			text: "Soft summary of the work done.",
			at: Date.parse("2026-10-13T10:05:00.000Z"),
		},
		{
			project_id: "p-demo",
			source: `handoff ${stem}`,
			text: "Handoff document for the next run.",
			at: Date.parse("2026-10-13T10:07:00.000Z"),
		},
		{
			project_id: "p-demo",
			source: `compact ${stem}`,
			text: "Legacy summary of the earlier work.",
			at: Date.parse("2026-10-13T10:08:00.000Z"),
		},
	]);
	expect(JSON.parse(readFileSync(stateFile, "utf8"))).toMatchObject({ lastScan: NOW });

	// A second scan of the same file inserts nothing new and indexes nothing again.
	utimesSync(file, NOW / 1000, NOW / 1000);
	await watcher.scan();
	expect(rows()).toHaveLength(3);
	expect(history()).toHaveLength(3);
});

test("a summary is found by its words in its project's history, and in no other project's", async () => {
	const { db, sessionsDir, watcher } = setup();
	writeSession(join(sessionsDir, "--C--work--demo--"), "findable", [
		header("s1", PROJECT),
		compaction("c1", "The importer streams rows to avoid loading the file.", "soft", "2026-10-13T10:01:00.000Z"),
	]);
	await watcher.scan();
	expect(searchHistory(db, "p-demo", "importer streams", 5).map((hit) => hit.text)).toEqual([
		"The importer streams rows to avoid loading the file.",
	]);
	expect(searchHistory(db, "p-other", "importer streams", 5)).toEqual([]);
});

test("summaries outside watched projects are stored but never indexed", async () => {
	const { sessionsDir, watcher, rows, history } = setup();
	writeSession(join(sessionsDir, "--C--scratch--"), "2026-10-13T10-00-00-000Z_01a2", [
		titleSlot("scratch"),
		header("s1", OUTSIDE),
		compaction("c1", "Scratch summary of an unrelated run.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	await watcher.scan();
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.cwd, r.indexed])).toEqual([["c1", OUTSIDE, 0]]);
	expect(history()).toEqual([]);
});

test("a session moved to a sibling file gives its copied compactions once; its own new ones are rows (0464)", async () => {
	const { sessionsDir, watcher, rows, history } = setup();
	const cwdDir = join(sessionsDir, "--C--work--demo--");
	const old = writeSession(cwdDir, "2026-10-13T10-00-00-000Z_old1", [
		titleSlot("demo"),
		header("old1", PROJECT),
		compaction("c1", "Soft summary before the move.", "soft", "2026-10-13T10:05:00.000Z"),
	]);
	const moved = writeSession(cwdDir, "2026-10-13T10-10-00-000Z_new2", [
		titleSlot("demo"),
		JSON.stringify({ type: "session", version: 3, id: "new2", parentSession: "old1", cwd: PROJECT }),
		compaction("c1", "Soft summary before the move.", "soft", "2026-10-13T10:05:00.000Z"),
		JSON.stringify({ type: "model_usage", id: "u9", usage: { cost: { total: 0.1 } } }),
		JSON.stringify({ type: "memory_recall", id: "m9", text: "recalled" }),
		compaction("c2", "Handoff after the move.", "handoff", "2026-10-13T10:12:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.session_file, r.entry_id])).toEqual([
		[old, "c1"],
		[moved, "c2"],
	]);
	expect(history().map((line) => line.text)).toEqual(["Soft summary before the move."]);
});

test("remote and snapcompact summaries are stored but not indexed (0400)", async () => {
	const { sessionsDir, watcher, rows, history } = setup();
	writeSession(join(sessionsDir, "--C--work--demo--"), "methods", [
		header("s1", PROJECT),
		compaction("c1", "Remote summary of the whole run.", "remote", "2026-10-13T10:01:00.000Z"),
		compaction("c2", "Snap summary of the whole run.", "snapcompact", "2026-10-13T10:02:00.000Z"),
		compaction("c3", "Soft summary of the whole run.", "soft", "2026-10-13T10:03:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.indexed])).toEqual([
		["c1", 1],
		["c2", 1],
		["c3", 1],
	]);
	expect(history().map((line) => line.text)).toEqual(["Soft summary of the whole run."]);
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

test("snapcompact transcript archives never reach the history, whatever their method", async () => {
	const { sessionsDir, watcher, rows, history } = setup();
	writeSession(join(sessionsDir, "--C--work--demo--"), "archives", [
		header("s1", PROJECT),
		compaction("c1", ARCHIVE, undefined, "2026-10-13T10:01:00.000Z"),
		compaction("c2", ARCHIVE, "soft", "2026-10-13T10:02:00.000Z"),
		compaction("c3", "Soft summary of the whole run.", "soft", "2026-10-13T10:03:00.000Z"),
	]);
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.indexed])).toEqual([
		["c1", 1],
		["c2", 1],
		["c3", 1],
	]);
	expect(history().map((line) => line.text)).toEqual(["Soft summary of the whole run."]);
});

test("an archive already waiting in catalog.db is marked indexed without lines and no longer holds the queue", async () => {
	const seed = openCatalog();
	const insert = seed.query(
		"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, indexed) VALUES (?, ?, 'compact', ?, ?, ?, 0)",
	);
	insert.run("old.jsonl", "a1", PROJECT, NOW - 7_200_000, ARCHIVE);
	insert.run("old.jsonl", "a2", PROJECT, NOW - 3_600_000, "Older soft summary of the run.");
	seed.close();
	const { watcher, rows, history } = setup();
	await watcher.scan();
	expect(rows().map((r) => [r.entry_id, r.indexed])).toEqual([
		["a1", 1],
		["a2", 1],
	]);
	expect(history().map((line) => line.text)).toEqual(["Older soft summary of the run."]);
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
	const { sessionsDir, stateFile, watcher, rows } = setup();
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

test("a summary is indexed under the project stored with it once its cwd folder is gone, and one from before projects were stored under the project its cwd names (0440 (4))", async () => {
	const seed = openCatalog();
	const insert = seed.query(
		"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, indexed, project_id) VALUES (?, ?, 'compact', ?, ?, ?, 0, ?)",
	);
	insert.run("kept.jsonl", "a1", "C:\\gone\\folder", NOW - 7_200_000, "Kept summary of a removed folder.", "p-demo");
	insert.run("old.jsonl", "a2", PROJECT, NOW - 3_600_000, "Older summary from before projects were stored.", null);
	seed.close();
	const { watcher, history } = setup();
	await watcher.scan();
	expect(history().map((line) => [line.project_id, line.text])).toEqual([
		["p-demo", "Kept summary of a removed folder."],
		["p-demo", "Older summary from before projects were stored."],
	]);
});

test("scans that change nothing leave watcher.json alone", async () => {
	const { sessionsDir, stateFile, watcher } = setup();
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

test("a session omp rewrote through a rename, no shorter than before, is read again from its start (SRV-B-3)", async () => {
	let clock = NOW;
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const watcher = startWatcher({
		sessionsDir,
		stateFile: join(env.data, "watcher.json"),
		db,
		projectFor: () => ({ projectId: "p-demo" }),
		intervalMs: 3_600_000,
		now: () => clock,
		onError: () => {},
	});
	env.defer(() => {
		watcher.stop();
		db.close();
	});
	const dir = join(sessionsDir, "--C--work--demo--");
	const longTurn = assistant("a1", [{ type: "text", text: "x".repeat(5000) }], "length");
	const file = writeSession(dir, "rewritten", [header("s1", PROJECT), longTurn]);
	await watcher.scan();
	// omp's discard: the long turn dropped (re-journaled small), then the recovery's compaction; the whole file written
	// to a temp file and renamed over it, longer than the offset the last scan kept.
	const discarded = JSON.stringify({
		type: "model_usage",
		id: "m9",
		parentId: null,
		timestamp: "2026-10-13T10:02:00.000Z",
		purpose: "discarded-turn",
		usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, cost: { total: 0.01 } },
	});
	const temp = writeSession(dir, "rewritten.tmp", [
		header("s1", PROJECT),
		discarded,
		compaction("c1", `## Goal\n${"The task so far. ".repeat(400)}`, "soft", "2026-10-13T10:03:00.000Z"),
	]);
	expect(statSync(temp).size).toBeGreaterThan(statSync(file).size);
	renameSync(temp, file);
	clock = NOW + 60_000;
	utimesSync(file, clock / 1000, clock / 1000);
	await watcher.scan();
	const rows = db.query<{ entry_id: string }, []>("SELECT entry_id FROM summaries").all();
	expect(rows).toEqual([{ entry_id: "c1" }]);
});

test("watcher.json is written when its files change, at most every 5 minutes while only offsets move, and at stop (PERF-3)", async () => {
	let clock = NOW;
	const db = openCatalog();
	const sessionsDir = join(env.root, "sessions");
	const stateFile = join(env.data, "watcher.json");
	const watcher = startWatcher({
		sessionsDir,
		stateFile,
		db,
		projectFor: () => undefined,
		intervalMs: 3_600_000,
		now: () => clock,
		onError: () => {},
	});
	env.defer(() => db.close());
	const dir = join(sessionsDir, "--C--work--demo--");
	const file = writeSession(dir, "busy", [header("s1", PROJECT)]);
	const offset = () => JSON.parse(readFileSync(stateFile, "utf8")).files[file]?.offset;
	await watcher.scan();
	expect(offset()).toBe(statSync(file).size);
	// An offset moved a minute later: kept in memory only.
	appendFileSync(file, `${compaction("c1", "First.", "soft", "2026-10-13T10:01:00.000Z")}\n`);
	clock += 60_000;
	utimesSync(file, clock / 1000, clock / 1000);
	await watcher.scan();
	expect(offset()).toBeLessThan(statSync(file).size);
	// A new file is written at once.
	const other = writeSession(dir, "new", [header("s2", PROJECT)], clock);
	clock += 1_000;
	await watcher.scan();
	expect(offset()).toBe(statSync(file).size);
	expect(JSON.parse(readFileSync(stateFile, "utf8")).files[other]).toBeDefined();
	// Moved offsets wait for 5 minutes, or the stop.
	appendFileSync(file, `${compaction("c2", "Second.", "soft", "2026-10-13T10:02:00.000Z")}\n`);
	clock += 60_000;
	utimesSync(file, clock / 1000, clock / 1000);
	await watcher.scan();
	expect(offset()).toBeLessThan(statSync(file).size);
	watcher.stop();
	expect(offset()).toBe(statSync(file).size);
});
