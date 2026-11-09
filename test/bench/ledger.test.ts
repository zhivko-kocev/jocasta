import { expect, test } from "bun:test";
import { benchPricing, readClaudeTranscript, readOpencodeExport } from "../../bench/lib/ledger.ts";
import { candidateKeys, loadPriceTable, priceRequest, priceTableFrom } from "../../bench/lib/prices.ts";
import {
	argHash,
	CACHE_MISS_WRITE_TOKENS,
	normalizePath,
	OVERSIZED_RESULT_BYTES,
	pathsOf,
	readOmpSession,
	summarizeSession,
	trialLedger,
} from "../../src/shared/ledger.ts";

const T0 = Date.parse("2026-11-09T10:00:00Z");
const iso = (minutes: number) => new Date(T0 + minutes * 60_000).toISOString();
const lines = (entries: unknown[]) => entries.map((entry) => JSON.stringify(entry)).join("\n");

/** A small table: one model with a 1-hour cache write rate and a 200k tier, one without. */
const prices = priceTableFrom(
	{
		"claude-test": {
			input_cost_per_token: 1e-6,
			output_cost_per_token: 1e-5,
			cache_read_input_token_cost: 1e-7,
			cache_creation_input_token_cost: 2e-6,
			cache_creation_input_token_cost_above_1hr: 3e-6,
			input_cost_per_token_above_200k_tokens: 2e-6,
			output_cost_per_token_above_200k_tokens: 2e-5,
			max_input_tokens: 200_000,
		},
		"plain-model": { input_cost_per_token: 1e-6, output_cost_per_token: 1e-6 },
	},
	{ url: "test", commit: "test", commitDate: "test", fetchedAt: "test", sha256: "test" },
);
const pricing = benchPricing(prices);

test("the committed price table matches its pinned SHA-256 and prices the arms' model (0538)", () => {
	const table = loadPriceTable();
	expect(table.source.commit).toBe("e58354f0554cb11b0ce2f042ff625957cded398f");
	const opus = table.lookup("anthropic", "claude-opus-5-5");
	expect(opus).toMatchObject({ key: "claude-opus-5-5", input: 4e-6, output: 2e-5, cacheRead: 2e-7, cacheWrite: 5e-6 });
	expect(opus?.cacheWrite1h).toBe(8e-6);
	expect(opus?.contextWindow).toBe(1_000_000);
	// omp's model names, Claude Code's `[1m]` suffix and opencode's provider/model all find it.
	expect(table.lookup(null, "anthropic/claude-opus-5-5")?.key).toBe("claude-opus-5-5");
	expect(table.lookup("anthropic", "claude-opus-5-5[1m]")?.key).toBe("claude-opus-5-5");
	expect(table.lookup("anthropic", "no-such-model")).toBeNull();
	expect(candidateKeys("anthropic", "claude-x[1m]")).toEqual(["anthropic/claude-x", "claude-x"]);
});

test("a request's price: cache read and write apart, a 1-hour write at its rate, the 200k tier past 200k", () => {
	const price = prices.lookup(null, "claude-test");
	if (!price) throw new Error("no test price");
	expect(
		priceRequest(price, { input: 1000, cacheRead: 10_000, cacheWrite: 3000, cacheWrite1h: 1000, output: 100 }),
	).toBeCloseTo(1000 * 1e-6 + 10_000 * 1e-7 + 2000 * 2e-6 + 1000 * 3e-6 + 100 * 1e-5, 12);
	expect(priceRequest(price, { input: 250_000, cacheRead: 0, cacheWrite: 0, cacheWrite1h: 0, output: 10 })).toBeCloseTo(
		250_000 * 2e-6 + 10 * 2e-5,
		12,
	);
});

test("paths compare without omp's read selectors; edits name their files; arguments hash without omp's intent", () => {
	expect(normalizePath("./src/A.ts:50-80")).toBe("src/a.ts");
	expect(normalizePath("src\\a.ts:raw")).toBe("src/a.ts");
	expect(normalizePath("D:/x/a.ts:2-4:raw")).toBe("d:/x/a.ts");
	expect(normalizePath("D:/x/a.ts")).toBe("d:/x/a.ts");
	expect(pathsOf({ input: "[src/a.ts#1A2B]\nPUT 1.=1:\n+x\n[src/b.ts#3C4D]\nCUT 2" })).toEqual([
		"src/a.ts",
		"src/b.ts",
	]);
	expect(pathsOf({ patchText: "*** Begin Patch\n*** Update File: arc/x.ts\n@@\n*** Add File: arc/y.ts\n" })).toEqual([
		"arc/x.ts",
		"arc/y.ts",
	]);
	expect(argHash({ path: "a", i: "Reading a" }, "omp")).toBe(argHash({ path: "a", i: "Reading it again" }, "omp"));
	expect(argHash({ b: 1, a: 2 }, "claude")).toBe(argHash({ a: 2, b: 1 }, "claude"));
	expect(argHash({ path: "a", i: "x" }, "claude")).not.toBe(argHash({ path: "a", i: "y" }, "claude"));
});

/** One omp assistant message entry with usage and tool calls. */
const ompTurn = (
	minutes: number,
	usage: Record<string, unknown>,
	calls: { id: string; name: string; arguments: unknown }[] = [],
) => ({
	type: "message",
	timestamp: iso(minutes),
	message: {
		role: "assistant",
		provider: "anthropic",
		model: "claude-test",
		timestamp: T0 + minutes * 60_000,
		content: calls.map((call) => ({ type: "toolCall", ...call })),
		usage,
	},
});
const ompResult = (id: string, text: string, isError = false) => ({
	type: "message",
	timestamp: iso(0),
	message: { role: "toolResult", toolCallId: id, toolName: "x", content: [{ type: "text", text }], isError },
});

test("omp: requests, side requests, tool calls, waste and the peak context of one session file", () => {
	const text = lines([
		{ type: "title", title: "t" },
		{ type: "session", id: "s-omp", timestamp: iso(0) },
		ompTurn(0, { input: 10, output: 5, cacheRead: 0, cacheWrite: 20_000, cttl: { ephemeral1h: 0 } }, [
			{ id: "r1", name: "read", arguments: { path: "src/a.ts", i: "Reading" } },
			{ id: "r2", name: "read", arguments: { path: "src/a.ts:10-20", i: "Reading again" } },
		]),
		ompResult("r1", "x".repeat(OVERSIZED_RESULT_BYTES)),
		ompResult("r2", "y"),
		ompTurn(1, { input: 5, output: 5, cacheRead: 20_000, cacheWrite: 100 }, [
			{ id: "e1", name: "edit", arguments: { input: "[src/a.ts#1A2B]\nPUT 1.=1:\n+z" } },
			{ id: "r3", name: "read", arguments: { path: "src/a.ts" } },
			{ id: "b1", name: "bash", arguments: { command: "bun test" } },
			{ id: "b2", name: "bash", arguments: { command: "bun test" } },
		]),
		ompResult("e1", "ok"),
		ompResult("r3", "z"),
		ompResult("b1", "fail", true),
		ompResult("b2", "fail", true),
		{
			type: "model_usage",
			timestamp: iso(4),
			purpose: "cache-warm",
			provider: "anthropic",
			model: "claude-test",
			usage: { input: 2, output: 5, cacheRead: 20_100, cacheWrite: 0 },
		},
		{ type: "compaction", timestamp: iso(5), tokensBefore: 100 },
		// Twelve minutes after the last turn: a cache miss (a large write after more than 5 minutes idle).
		ompTurn(13, { input: 5, output: 5, cacheRead: 0, cacheWrite: CACHE_MISS_WRITE_TOKENS }),
	]);
	const record = readOmpSession(text, pricing);
	expect(record.session).toBe("s-omp");
	expect(record.requests.map((r) => r.purpose ?? "turn")).toEqual(["turn", "turn", "cache-warm", "turn"]);
	expect(record.tools.map((t) => [t.tool, t.path, t.resultBytes, t.error])).toEqual([
		["read", "src/a.ts", OVERSIZED_RESULT_BYTES, false],
		["read", "src/a.ts", 1, false],
		["edit", "src/a.ts", 2, false],
		["read", "src/a.ts", 1, false],
		["bash", null, 4, true],
		["bash", null, 4, true],
	]);
	const summary = summarizeSession(record, pricing);
	// The range read right after the whole file is a re-read; the read after the edit isn't.
	expect(summary.waste).toEqual({
		rereads: 1,
		repeatedCalls: 1,
		oversizedResults: 1,
		repeatedFailures: 1,
		cacheMisses: 1,
		// The cache-warm request at minute 4 is the last before the miss: 9 minutes idle.
		cacheMissIdleMinutes: [9],
		compactions: 1,
	});
	// The cache-warm request's 20,102 tokens don't count: the agent's own peak is 20,105 at minute 1.
	expect(summary.peakContext).toEqual({ tokens: 20_105, window: 200_000, percent: 10.1 });
	expect(summary.requests).toBe(4);
	expect(summary.cacheReadShare).toBeCloseTo(40_100 / (22 + 40_100 + 30_100), 12);
	expect(summary.cost).toBeCloseTo(
		record.requests.reduce((sum, r) => sum + (r.cost ?? 0), 0),
		12,
	);
});

test("Claude Code: one request per message id, `<synthetic>` skipped, results and compactions read", () => {
	const usage = {
		input_tokens: 3,
		output_tokens: 40,
		cache_read_input_tokens: 1000,
		cache_creation_input_tokens: 500,
		cache_creation: { ephemeral_1h_input_tokens: 500, ephemeral_5m_input_tokens: 0 },
	};
	const text = lines([
		{ type: "user", sessionId: "s-claude", timestamp: iso(0), message: { role: "user", content: "fix it" } },
		{
			type: "assistant",
			sessionId: "s-claude",
			timestamp: iso(0),
			message: { id: "msg_1", model: "claude-test", usage, content: [{ type: "thinking", thinking: "…" }] },
		},
		{
			type: "assistant",
			sessionId: "s-claude",
			timestamp: iso(0),
			message: {
				id: "msg_1",
				model: "claude-test",
				usage,
				content: [{ type: "tool_use", id: "tu1", name: "Read", input: { file_path: "D:\\r\\a.ts" } }],
			},
		},
		{
			type: "user",
			sessionId: "s-claude",
			timestamp: iso(1),
			message: {
				role: "user",
				content: [{ type: "tool_result", tool_use_id: "tu1", content: "abc", is_error: false }],
			},
		},
		{ type: "system", subtype: "compact_boundary", sessionId: "s-claude", timestamp: iso(2) },
		{
			type: "assistant",
			sessionId: "s-claude",
			timestamp: iso(3),
			message: { id: "synthetic", model: "<synthetic>", usage: { input_tokens: 0, output_tokens: 0 }, content: [] },
		},
	]);
	const record = readClaudeTranscript(text, pricing);
	expect(record.session).toBe("s-claude");
	expect(record.requests).toHaveLength(1);
	expect(record.requests[0]).toMatchObject({
		input: 3,
		output: 40,
		cacheRead: 1000,
		cacheWrite: 500,
		cacheWrite1h: 500,
	});
	expect(record.requests[0]?.cost).toBeCloseTo(3 * 1e-6 + 40 * 1e-5 + 1000 * 1e-7 + 500 * 3e-6, 12);
	expect(record.tools.map((t) => [t.tool, t.path, t.resultBytes])).toEqual([["Read", "d:/r/a.ts", 3]]);
	expect(record.compactions).toHaveLength(1);
});

test("opencode: each step-finish is a request (reasoning as output), tool parts are calls, errors read", () => {
	const exported = {
		info: { id: "ses_x" },
		messages: [
			{ info: { role: "user", time: { created: T0 } }, parts: [{ type: "text", text: "fix" }] },
			{
				info: { role: "assistant", providerID: "anthropic", modelID: "claude-test", time: { created: T0 + 1000 } },
				parts: [
					{ type: "step-start" },
					{
						type: "tool",
						tool: "read",
						state: {
							status: "completed",
							input: { filePath: "src/a.ts" },
							output: "abcd",
							time: { start: T0 + 2000, end: T0 + 3000 },
						},
					},
					{
						type: "step-finish",
						tokens: { input: 10, output: 5, reasoning: 7, cache: { read: 100, write: 50 } },
						cost: 0,
					},
					{
						type: "tool",
						tool: "apply_patch",
						state: {
							status: "error",
							input: { patchText: "*** Update File: src/a.ts\n" },
							error: "no",
							time: { start: T0 + 4000, end: T0 + 5000 },
						},
					},
					{
						type: "step-finish",
						tokens: { input: 1, output: 1, reasoning: 0, cache: { read: 160, write: 0 } },
						cost: 0,
					},
				],
			},
		],
	};
	const record = readOpencodeExport(JSON.stringify(exported), pricing);
	expect(record.session).toBe("ses_x");
	expect(record.requests.map((r) => [r.at - T0, r.input, r.output, r.cacheRead, r.cacheWrite])).toEqual([
		[3000, 10, 12, 100, 50],
		[5000, 1, 1, 160, 0],
	]);
	expect(record.tools.map((t) => [t.tool, t.path, t.resultBytes, t.error])).toEqual([
		["read", "src/a.ts", 4, false],
		["apply_patch", "src/a.ts", 2, true],
	]);
});

test("a trial's ledger sums its sessions, keeps the largest peak and lists models the table lacks", () => {
	const main = readOmpSession(
		lines([{ type: "session", id: "main" }, ompTurn(0, { input: 100, output: 10, cacheRead: 0, cacheWrite: 0 })]),
		pricing,
	);
	const sub = readOmpSession(
		lines([
			{ type: "session", id: "sub" },
			{
				type: "message",
				timestamp: iso(1),
				message: {
					role: "assistant",
					provider: "x",
					model: "unknown-model",
					content: [],
					usage: { input: 900, output: 1 },
				},
			},
		]),
		pricing,
		"sub/agent",
	);
	const ledger = trialLedger([main, sub], pricing);
	expect(ledger.totals).toMatchObject({ requests: 2, input: 1000, output: 11, unpricedModels: ["unknown-model"] });
	expect(ledger.totals.cost).toBeCloseTo(100 * 1e-6 + 10 * 1e-5, 12);
	expect(ledger.totals.peakContext).toEqual({ tokens: 900, window: null, percent: null });
	expect(ledger.sessions.map((s) => s.agent)).toEqual(["main", "sub/agent"]);
	expect(ledger.rows).toHaveLength(2);
	expect(priceTableFrom({}, prices.source).lookup(null, "plain-model")).toBeNull();
});
