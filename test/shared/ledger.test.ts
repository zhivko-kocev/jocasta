import { Database } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import {
	CACHE_MISS_WRITE_TOKENS,
	forgetStaleSessions,
	type LedgerPricing,
	ledgerLine,
	ompModelCost,
	ompPricing,
	readOmpSession,
	type SessionRecord,
	summarizeSession,
	type ThreadLedger,
	threadLedger,
	threadSessionRecords,
	trialLedger,
	type Waste,
	wasteOf,
} from "../../src/shared/ledger.ts";

const T0 = Date.parse("2026-11-09T10:00:00Z");
const iso = (minutes: number) => new Date(T0 + minutes * 60_000).toISOString();
const lines = (entries: unknown[]) => `${entries.map((entry) => JSON.stringify(entry)).join("\n")}\n`;

/** One omp assistant turn with usage (omp's `cost.total` included when given) and tool calls. */
const turn = (
	minutes: number,
	usage: { input?: number; cacheRead?: number; cacheWrite?: number; output?: number; cost?: number },
	calls: { id: string; name: string; arguments: unknown }[] = [],
	model = { provider: "anthropic", model: "claude-test" },
) => ({
	type: "message",
	timestamp: iso(minutes),
	message: {
		role: "assistant",
		...model,
		timestamp: T0 + minutes * 60_000,
		content: calls.map((call) => ({ type: "toolCall", ...call })),
		usage: {
			input: usage.input ?? 0,
			cacheRead: usage.cacheRead ?? 0,
			cacheWrite: usage.cacheWrite ?? 0,
			output: usage.output ?? 0,
			...(usage.cost === undefined ? {} : { cost: { total: usage.cost } }),
		},
	},
});
const result = (id: string) => ({
	type: "message",
	timestamp: iso(0),
	message: { role: "toolResult", toolCallId: id, toolName: "x", content: [{ type: "text", text: "ok" }] },
});
const read = (id: string, path: string) => ({ id, name: "read", arguments: { path, i: "Reading" } });

let root = "";
const savedAgentDir = process.env.PI_CODING_AGENT_DIR;

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), "jocasta-ledger-"));
	process.env.PI_CODING_AGENT_DIR = join(root, "agent");
	mkdirSync(join(root, "agent"), { recursive: true });
});

afterEach(() => {
	if (savedAgentDir === undefined) delete process.env.PI_CODING_AGENT_DIR;
	else process.env.PI_CODING_AGENT_DIR = savedAgentDir;
	removeTree(root);
});

/** omp's model cache with `rows` (provider id → models). */
function writeModelsDb(
	rows: Record<string, { id: string; contextWindow?: number; cost?: Record<string, number> }[]>,
): void {
	const db = new Database(join(root, "agent", "models.db"), { create: true });
	db.exec(
		"CREATE TABLE IF NOT EXISTS model_cache (provider_id TEXT PRIMARY KEY, models TEXT NOT NULL, updated_at INTEGER)",
	);
	db.exec("DELETE FROM model_cache");
	for (const [provider, models] of Object.entries(rows))
		db.query("INSERT INTO model_cache (provider_id, models, updated_at) VALUES (?, ?, 0)").run(
			provider,
			JSON.stringify(models.map((m) => ({ ...m, name: m.id, maxTokens: 32_000 }))),
		);
	db.close();
}

/**
 * A thread worth 0546's line: $1.84 over the main session and a subagent, the main session's peak 82k of a 200k
 * window, cache read 71%, 3 re-reads, 1 repeated call, 1 cache miss, 2 compactions; plus a scratch copy that counts not.
 */
function writeThread(): string {
	const sessions = join(root, "agent", "sessions", "--d--work--acme--");
	const file = join(sessions, "2026-11-09T10-00-00-000Z_s-main.jsonl");
	const artifacts = file.replace(/\.jsonl$/, "");
	mkdirSync(join(artifacts, "0-Explore"), { recursive: true });
	writeFileSync(
		file,
		lines([
			{ type: "session", id: "s-main", timestamp: iso(0) },
			turn(0, { input: 1000, cacheWrite: 20_000, cost: 0.5 }, [read("r1", "src/a.ts"), read("r2", "src/a.ts:1-9")]),
			result("r1"),
			result("r2"),
			turn(1, { input: 1000, cacheRead: 60_000, cacheWrite: 21_000, cost: 0.6 }, [
				read("r3", "src/a.ts"),
				{ id: "b1", name: "bash", arguments: { command: "bun test" } },
				{ id: "b2", name: "bash", arguments: { command: "bun test" } },
			]),
			result("r3"),
			result("b1"),
			result("b2"),
			{ type: "compaction", timestamp: iso(2), tokensBefore: 82_000 },
			// Nine minutes idle, then a large cache write: a cache miss.
			turn(10, { cacheWrite: CACHE_MISS_WRITE_TOKENS, cost: 0.34 }),
		]),
	);
	// The subagent's context is larger than the main session's; the peak is the main session's all the same.
	writeFileSync(
		join(artifacts, "0-Explore", "0-Explore.jsonl"),
		lines([
			{ type: "session", id: "s-sub", timestamp: iso(2) },
			turn(2, { cacheRead: 100_000, cacheWrite: 12_350, cost: 0.4 }, [read("s1", "src/c.ts"), read("s2", "src/c.ts")]),
			result("s1"),
			result("s2"),
			{ type: "compaction", timestamp: iso(3), tokensBefore: 112_350 },
		]),
	);
	writeFileSync(join(artifacts, "0-Explore.omp-gc-compress-123.jsonl"), lines([turn(4, { input: 9, cost: 99 })]));
	return file;
}

const noWaste: Waste = {
	rereads: 0,
	repeatedCalls: 0,
	oversizedResults: 0,
	repeatedFailures: 0,
	cacheMisses: 0,
	cacheMissIdleMinutes: [],
	compactions: 0,
};

test("omp's own pricing: a request costs its usage.cost.total; one without is unpriced", () => {
	writeModelsDb({ anthropic: [{ id: "claude-test", contextWindow: 200_000 }] });
	const record = readOmpSession(
		lines([
			{ type: "session", id: "s" },
			turn(0, { input: 10, cacheWrite: 1000, output: 5, cost: 0.25 }),
			{
				type: "model_usage",
				timestamp: iso(1),
				purpose: "cache-warm",
				provider: "anthropic",
				model: "claude-test",
				usage: { input: 1, cacheRead: 150_000, output: 1, cost: { total: 0.03 } },
			},
			turn(2, { input: 20 }, [], { provider: "acme", model: "no-cost" }),
		]),
		ompPricing,
	);
	expect(record.requests.map((r) => r.cost)).toEqual([0.25, 0.03, null]);
	const summary = summarizeSession(record, ompPricing);
	expect(summary.cost).toBeCloseTo(0.28, 12);
	expect(summary.unpricedModels).toEqual(["no-cost"]);
	// The cache-warm request is larger but a side request; the agent's own peak is 1,010 tokens of claude-test's window.
	expect(summary.peakContext).toEqual({ tokens: 1010, window: 200_000, percent: 0.5 });
});

test("a model's window: provider equal or `<provider>:`-prefixed, model id equal; unknown otherwise", () => {
	writeModelsDb({
		anthropic: [{ id: "claude-test", contextWindow: 200_000 }],
		"github-copilot:models-v2:abc123": [{ id: "gpt-test", contextWindow: 1_000_000 }],
		"github-copilotx": [{ id: "other", contextWindow: 5 }],
	});
	expect(ompPricing.window("anthropic", "claude-test")).toBe(200_000);
	expect(ompPricing.window("github-copilot", "gpt-test")).toBe(1_000_000);
	expect(ompPricing.window("github-copilot", "other")).toBeNull();
	expect(ompPricing.window("anthropic", "gpt-test")).toBeNull();
	expect(ompPricing.window(null, "claude-test")).toBeNull();
});

test("a thread's ledger: spend, cache read and waste over all its transcripts, peak from the main session (0546)", () => {
	writeModelsDb({ anthropic: [{ id: "claude-test", contextWindow: 200_000 }] });
	const file = writeThread();
	const ledger = threadLedger([file]);
	expect(ledger).toBeDefined();
	if (!ledger) return;
	expect(ledger.spend).toBeCloseTo(1.84, 12);
	expect(ledger.requests).toBe(4);
	expect(ledger.cacheReadShare).toBeCloseTo(160_000 / 225_350, 12);
	expect(ledger.peakContext).toEqual({ tokens: 82_000, window: 200_000, percent: 41 });
	expect(ledger.waste).toMatchObject({ rereads: 3, repeatedCalls: 1, cacheMisses: 1, compactions: 2 });
	expect(ledgerLine(ledger)).toBe(
		"spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 repeated call · 1 cache miss · 2 compactions",
	);
});

test("a thread's files are parsed again only when their size or mtime changes", () => {
	writeModelsDb({});
	const file = writeThread();
	// A whole-millisecond mtime, so setting it back is exact.
	const when = new Date(T0);
	utimesSync(file, when, when);
	expect(threadLedger([file])?.spend).toBeCloseTo(1.84, 12);
	// Same size and mtime: the cached parse stands.
	writeFileSync(file, readFileSync(file, "utf8").replace('"total":0.5}', '"total":0.9}'));
	utimesSync(file, when, when);
	expect(threadLedger([file])?.spend).toBeCloseTo(1.84, 12);
	utimesSync(file, when, new Date(T0 + 5000));
	expect(threadLedger([file])?.spend).toBeCloseTo(2.24, 12);
});

test("a thread's ledger covers every session of it: a new session's spend adds, a moved session's copies count once, the peak is the largest session's own (0538, 0546)", () => {
	writeModelsDb({ anthropic: [{ id: "claude-test", contextWindow: 200_000 }] });
	const first = writeThread();
	const sessions = join(root, "agent", "sessions", "--d--work--acme--");
	// "Approve and execute" goes on in a new session, which names no parent.
	const executing = turn(20, { input: 1000, cacheRead: 150_000, cacheWrite: 1000, cost: 0.22 });
	const second = join(sessions, "2026-11-09T10-20-00-000Z_s-exec.jsonl");
	writeFileSync(second, lines([{ type: "session", id: "s-exec", timestamp: iso(20) }, executing]));
	// That session moved to a sibling file, which copies what it held and goes on.
	const third = join(sessions, "2026-11-09T10-25-00-000Z_s-moved.jsonl");
	writeFileSync(
		third,
		lines([
			{ type: "session", id: "s-moved", parentSession: "s-exec", timestamp: iso(25) },
			executing,
			turn(30, { input: 10, cost: 0.1 }),
		]),
	);
	const ledger = threadLedger([first, second, third]);
	expect(ledger?.spend).toBeCloseTo(2.16, 12);
	expect(ledger?.requests).toBe(6);
	expect(ledger?.peakContext).toEqual({ tokens: 152_000, window: 200_000, percent: 76 });
	expect(ledger?.waste).toMatchObject({ rereads: 3, repeatedCalls: 1, cacheMisses: 1, compactions: 2 });
	// The current session alone, as before: the earlier one's spend is missing.
	expect(threadLedger([third])?.spend).toBeCloseTo(0.32, 12);
});

test("no models.db: the peak shows tokens only; no session file: no ledger", () => {
	const file = writeThread();
	const ledger = threadLedger([file]);
	expect(ledger?.peakContext).toEqual({ tokens: 82_000, window: null, percent: null });
	expect(ledger && ledgerLine(ledger)).toStartWith("spend $1.84 · peak context 82k · cache read 71% · ");
	expect(threadLedger([join(root, "missing.jsonl")])).toBeUndefined();
});

test("the ledger line: two-decimal spend, M-sized windows, nothing shown for what is zero, singular and plural", () => {
	const base: ThreadLedger = { spend: 0, requests: 0, cacheReadShare: null, peakContext: null, waste: noWaste };
	expect(ledgerLine(base)).toBe("spend $0.00");
	expect(
		ledgerLine({
			...base,
			spend: 12.3456,
			requests: 3,
			cacheReadShare: 0.5,
			peakContext: { tokens: 82_400, window: 1_000_000, percent: 8.2 },
			waste: { ...noWaste, rereads: 1, repeatedCalls: 2, cacheMisses: 2, cacheMissIdleMinutes: [6, 7], compactions: 1 },
		}),
	).toBe(
		"spend $12.35 · peak context 8% (82k of 1M) · cache read 50% · 1 re-read · 2 repeated calls · 2 cache misses · 1 compaction",
	);
	expect(
		ledgerLine({
			...base,
			spend: 0.1,
			requests: 1,
			cacheReadShare: 0,
			peakContext: { tokens: 1_234_567, window: 2_000_000, percent: 61.7 },
		}),
	).toBe("spend $0.10 · peak context 62% (1.2M of 2M) · cache read 0%");
	expect(ledgerLine({ ...base, peakContext: { tokens: 400, window: null, percent: null } })).toBe(
		"spend $0.00 · peak context 0k",
	);
});

/** Per-token prices (dollars per million: input 3, cache read 0.3, cache write 3.75, output 15), as fork-copies.ts priced. */
const perToken: LedgerPricing = {
	cost: (r) => (r.input * 3 + r.cacheRead * 0.3 + r.cacheWrite * 3.75 + r.output * 15) / 1e6,
	window: () => null,
};

/** SRV-B-1's session (fork-copies.ts): two reads of one file, a compaction, a third turn; `cost` maps omp's cost. */
const forkHistory = (cost: (dollars: number) => number) => [
	turn(0, { input: 1000, cacheWrite: 20_000, output: 500, cost: cost(0.5) }, [read("r1", "src/a.ts")]),
	result("r1"),
	turn(1, { input: 1000, cacheRead: 60_000, cacheWrite: 1000, output: 500, cost: cost(0.6) }, [read("r2", "src/a.ts")]),
	result("r2"),
	{ type: "compaction", timestamp: iso(2), summary: "s", tokensBefore: 82_000 },
	turn(3, { input: 1000, cacheRead: 30_000, output: 500, cost: cost(0.2) }),
];

/** Every `.jsonl` under a trial's folder, read as the benchmark reads it (agent `main` at the top, else its path). */
function trialRecords(dir: string): SessionRecord[] {
	return readdirSync(dir, { recursive: true, withFileTypes: true })
		.filter((e) => e.isFile() && e.name.endsWith(".jsonl"))
		.map((e) => {
			const rel = relative(dir, join(e.parentPath, e.name)).replaceAll("\\", "/");
			return readOmpSession(
				readFileSync(join(e.parentPath, e.name), "utf8"),
				perToken,
				rel.includes("/") ? rel.replace(/\.jsonl$/, "") : "main",
			);
		});
}

test.each([
	["a /tan clone", "Tan-1.jsonl", { id: "tan-1", parentSession: "s-main" }],
	[
		"a seance subtask fork",
		"3-Consult.jsonl",
		{ id: "seance-1", parentSession: "s-main", seanceSource: "s-main.jsonl" },
	],
])(
	"%s nested in the session's artifacts counts its inherited history once, in the product and the benchmark (SRV-B-1)",
	(_name, file, header) => {
		const sessions = join(root, "agent", "sessions", "--d--work--acme--");
		const main = join(sessions, "2026-11-09T10-00-00-000Z_s-main.jsonl");
		const artifacts = main.replace(/\.jsonl$/, "");
		mkdirSync(artifacts, { recursive: true });
		writeFileSync(main, lines([{ type: "session", id: "s-main", timestamp: iso(0) }, ...forkHistory((d) => d)]));
		expect(threadLedger([main])?.requests).toBe(3);
		expect(trialLedger(threadSessionRecords([main], perToken), perToken).totals.cost).toBeCloseTo(0.13725, 12);
		// omp 18.18.6 writes the fork beside the parent's subagents: the parent's history again, cost zeroed, tokens kept.
		writeFileSync(
			join(artifacts, file),
			lines([
				{ type: "session", ...header, timestamp: iso(5) },
				...forkHistory(() => 0),
				turn(6, { input: 500, cacheRead: 10_000, output: 200, cost: 0.05 }),
			]),
		);
		const product = threadLedger([main]);
		expect(product?.requests).toBe(4);
		expect(product && ledgerLine(product)).toBe(
			"spend $1.35 · peak context 62k · cache read 80% · 1 re-read · 1 compaction",
		);
		const expected = { requests: 4, input: 3500, cacheRead: 100_000, cacheWrite: 21_000, output: 1700 };
		for (const records of [threadSessionRecords([main], perToken), trialRecords(sessions).reverse()]) {
			const { totals } = trialLedger(records, perToken);
			expect(totals).toMatchObject(expected);
			expect(totals.cost).toBeCloseTo(0.14475, 12);
			expect(totals.waste).toMatchObject({ rereads: 1, compactions: 1 });
		}
	},
);

test("a session omp moved to a sibling counts its copies once, its artifacts' copies too; the entry that moved it counts (SRV-B-1)", () => {
	const sessions = join(root, "agent", "sessions", "--d--work--acme--");
	const old = join(sessions, "2026-11-09T10-20-00-000Z_s-exec.jsonl");
	const moved = join(sessions, "2026-11-09T10-25-00-000Z_s-moved.jsonl");
	mkdirSync(sessions, { recursive: true });
	const executing = turn(20, { input: 1000, cacheRead: 150_000, cacheWrite: 1000, cost: 0.22 });
	writeFileSync(old, lines([{ type: "session", id: "s-exec", timestamp: iso(20) }, executing]));
	const subagent = lines([
		{ type: "session", id: "s-sub", parentSession: old, timestamp: iso(21) },
		turn(21, { input: 100, cost: 0.1 }, [read("s1", "src/c.ts"), read("s2", "src/c.ts")]),
		result("s1"),
		result("s2"),
	]);
	for (const dir of [old, moved]) {
		mkdirSync(join(dir.replace(/\.jsonl$/, ""), "0-Explore"), { recursive: true });
		writeFileSync(join(dir.replace(/\.jsonl$/, ""), "0-Explore", "0-Explore.jsonl"), subagent);
	}
	// The turn whose write moved the session is only in the new file, though it predates that file's header.
	writeFileSync(
		moved,
		lines([
			{ type: "session", id: "s-moved", parentSession: "s-exec", continuationOf: "s-exec", timestamp: iso(25) },
			executing,
			turn(24, { input: 10, cost: 0.03 }),
			turn(30, { input: 10, cost: 0.1 }),
		]),
	);
	const product = threadLedger([old, moved]);
	expect(product?.requests).toBe(4);
	expect(product?.spend).toBeCloseTo(0.45, 12);
	expect(product?.waste.rereads).toBe(1);
	for (const records of [threadSessionRecords([old, moved], perToken), trialRecords(sessions).reverse()]) {
		const { totals } = trialLedger(records, perToken);
		expect(totals).toMatchObject({ requests: 4, input: 1120, cacheRead: 150_000, cacheWrite: 1000 });
		expect(totals.waste.rereads).toBe(1);
	}
});

test("Jocasta's own /handoff isn't a compaction; omp's are (0582)", () => {
	const record = readOmpSession(
		lines([
			{ type: "session", id: "s" },
			turn(0, { input: 10, cost: 0.1 }),
			{ type: "compaction", method: "soft", timestamp: iso(1) },
			{ type: "compaction", method: "handoff", timestamp: iso(2) },
		]),
		ompPricing,
	);
	expect(wasteOf(record).compactions).toBe(1);
	const sessions = join(root, "agent", "sessions", "--d--work--acme--");
	mkdirSync(sessions, { recursive: true });
	const file = join(sessions, "2026-11-09T10-00-00-000Z_s.jsonl");
	writeFileSync(
		file,
		lines([
			{ type: "session", id: "s" },
			turn(0, { input: 10, cost: 0.1 }),
			{ type: "compaction", method: "handoff", timestamp: iso(2) },
		]),
	);
	const ledger = threadLedger([file]);
	expect(ledger && ledgerLine(ledger)).toBe("spend $0.10 · peak context 0k · cache read 0%");
});

test("a cache miss is measured against the same model's previous request; a side call on another model doesn't count", () => {
	const record = readOmpSession(
		lines([
			{ type: "session", id: "s" },
			turn(0, { input: 10 }),
			{
				type: "model_usage",
				timestamp: iso(7),
				purpose: "classifier",
				provider: "openai",
				model: "small",
				usage: { cacheWrite: 2 * CACHE_MISS_WRITE_TOKENS },
			},
			turn(9, { cacheWrite: CACHE_MISS_WRITE_TOKENS }),
		]),
		ompPricing,
	);
	expect(wasteOf(record)).toMatchObject({ cacheMisses: 1, cacheMissIdleMinutes: [9] });
});

test("models.db is read again when it changes; one that fails to open isn't kept", () => {
	const db = join(root, "agent", "models.db");
	expect(ompPricing.window("anthropic", "claude-test")).toBeNull();
	// An empty file opens but has no model_cache table yet.
	writeFileSync(db, "");
	expect(ompPricing.window("anthropic", "claude-test")).toBeNull();
	writeModelsDb({ anthropic: [{ id: "claude-test", contextWindow: 200_000 }] });
	utimesSync(db, new Date(T0), new Date(T0));
	expect(ompPricing.window("anthropic", "claude-test")).toBe(200_000);
	writeModelsDb({ anthropic: [{ id: "claude-test", contextWindow: 1_000_000 }] });
	utimesSync(db, new Date(T0), new Date(T0 + 5000));
	expect(ompPricing.window("anthropic", "claude-test")).toBe(1_000_000);
});

test("omp's price of a model: per million tokens, provider equal or `<provider>:`-prefixed, any provider when null", () => {
	writeModelsDb({
		anthropic: [
			{ id: "claude-test", contextWindow: 200_000, cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 } },
			{ id: "no-price", contextWindow: 200_000 },
		],
		"github-copilot:models-v2:abc123": [{ id: "gpt-test", cost: { input: 2, output: 8 } }],
	});
	expect(ompModelCost("anthropic", "claude-test")).toEqual({ input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 });
	expect(ompModelCost("github-copilot", "gpt-test")).toEqual({ input: 2, output: 8, cacheRead: 0, cacheWrite: 0 });
	expect(ompModelCost(null, "gpt-test")).toEqual({ input: 2, output: 8, cacheRead: 0, cacheWrite: 0 });
	expect(ompModelCost("anthropic", "gpt-test")).toBeNull();
	expect(ompModelCost("anthropic", "no-price")).toBeNull();
	// A model with a price but no window has no window.
	expect(ompPricing.window("github-copilot", "gpt-test")).toBeNull();
});

test("forgetting stale sessions drops kept parses of files unchanged since then, except those kept", () => {
	writeModelsDb({});
	const file = writeThread();
	const when = new Date(T0);
	utimesSync(file, when, when);
	expect(threadLedger([file])?.spend).toBeCloseTo(1.84, 12);
	// Same size and mtime: only a forgotten parse sees the change.
	writeFileSync(file, readFileSync(file, "utf8").replace('"total":0.5}', '"total":0.9}'));
	utimesSync(file, when, when);
	forgetStaleSessions(T0 + 1, (path) => path === file);
	expect(threadLedger([file])?.spend).toBeCloseTo(1.84, 12);
	forgetStaleSessions(T0);
	expect(threadLedger([file])?.spend).toBeCloseTo(1.84, 12);
	forgetStaleSessions(T0 + 1);
	expect(threadLedger([file])?.spend).toBeCloseTo(2.24, 12);
});
