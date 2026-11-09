import { Database } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import {
	CACHE_MISS_WRITE_TOKENS,
	ledgerLine,
	ompPricing,
	readOmpSession,
	summarizeSession,
	type ThreadLedger,
	threadLedger,
	type Waste,
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
function writeModelsDb(rows: Record<string, { id: string; contextWindow: number }[]>): void {
	const db = new Database(join(root, "agent", "models.db"), { create: true });
	db.exec("CREATE TABLE model_cache (provider_id TEXT PRIMARY KEY, models TEXT NOT NULL, updated_at INTEGER)");
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
