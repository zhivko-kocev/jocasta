import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { benchPricing } from "../../bench/lib/ledger.ts";
import { loadPriceTable } from "../../bench/lib/prices.ts";
import {
	compareTotals,
	ledgerTotals,
	TOKSCALE_MISSING,
	TOKSCALE_VERSION,
	tokscaleFor,
	tokscaleTotals,
} from "../../bench/tokscale-parity.ts";
import { readOmpSession } from "../../src/shared/ledger.ts";
import { useTempDirs } from "../shared/fixture.ts";

const tempDir = useTempDirs("jocasta-parity-");

const SESSION_ID = "01b2c3d4-0000-7000-8000-00000000beef";
const ompSession = [
	{ type: "session", version: 3, id: SESSION_ID, timestamp: "2026-11-09T10:00:00.000Z", cwd: "C:\\t\\repo" },
	{
		type: "message",
		id: "a1",
		timestamp: "2026-11-09T10:00:01.000Z",
		message: {
			role: "assistant",
			provider: "anthropic",
			model: "claude-opus-5-5",
			timestamp: Date.parse("2026-11-09T10:00:01.000Z"),
			content: [{ type: "text", text: "Done." }],
			usage: { input: 12, output: 34, cacheRead: 5600, cacheWrite: 780, totalTokens: 6426 },
		},
	},
]
	.map((entry) => JSON.stringify(entry))
	.join("\n");

test("tokscale's per-session entries sum over models; opencode's reasoning joins its output", () => {
	const json = JSON.stringify({
		groupBy: "client,session,model",
		entries: [
			{ sessionId: "s1", client: "claude", model: "a", input: 1, output: 2, cacheRead: 3, cacheWrite: 4, reasoning: 9 },
			{ sessionId: "s1", client: "claude", model: "b", input: 10, output: 20, cacheRead: 30, cacheWrite: 40 },
			{
				sessionId: "ses_2",
				client: "opencode",
				model: "a",
				input: 1,
				output: 2,
				cacheRead: 0,
				cacheWrite: 0,
				reasoning: 5,
			},
			{ client: "codex", model: "x", input: 1 },
		],
	});
	expect([...tokscaleTotals(json)]).toEqual([
		["s1", { client: "claude", input: 11, output: 22, cacheRead: 33, cacheWrite: 44 }],
		["ses_2", { client: "opencode", input: 1, output: 7, cacheRead: 0, cacheWrite: 0 }],
	]);
});

test("the ledger's sessions match tokscale's by id (or an id holding it, as omp's file names do); differences are named", () => {
	const prices = loadPriceTable();
	const ledger = ledgerTotals([readOmpSession(ompSession, benchPricing(prices))]);
	expect([...ledger]).toEqual([
		[SESSION_ID, { harness: "omp", input: 12, output: 34, cacheRead: 5600, cacheWrite: 780 }],
	]);
	const same = new Map([
		[`2026-11-09T10-00-00-000Z_${SESSION_ID}`, { input: 12, output: 34, cacheRead: 5600, cacheWrite: 780 }],
	]);
	expect(compareTotals("t-a0-0", ledger, same)[0]?.differences).toEqual([]);
	const off = new Map([[SESSION_ID, { input: 12, output: 35, cacheRead: 5600, cacheWrite: 0 }]]);
	expect(compareTotals("t-a0-0", ledger, off)[0]?.differences).toEqual(["cacheWrite", "output"]);
	expect(compareTotals("t-a0-0", ledger, new Map())[0]).toMatchObject({
		tokscale: null,
		differences: ["missing in tokscale"],
	});
});

test("a run folder with nothing to compare exits 2 instead of passing (0576)", () => {
	const runDir = tempDir();
	writeFileSync(
		join(runDir, "results.jsonl"),
		`${JSON.stringify({ task: "t", arm: "a0", rep: 0, harness: "omp" })}\n${JSON.stringify({ task: "t", arm: "a4", rep: 0, harness: "claude" })}\n`,
	);
	// A sessions folder whose names match no trial, as after a rename.
	mkdirSync(join(runDir, "sessions", "t-a0-0-renamed"), { recursive: true });
	const run = spawnSync(
		process.execPath,
		[join(import.meta.dir, "..", "..", "bench", "tokscale-parity.ts"), "--out", runDir],
		{
			encoding: "utf8",
		},
	);
	expect(run.status).toBe(2);
	expect(run.stderr.trim()).toBe(
		`bench: no saved sessions to compare in ${join(runDir, "sessions")} (results.jsonl names 2 trials)`,
	);
});

const tokscale = Bun.which("tokscale");
if (tokscale === null) console.log(`tokscale parity: skipped. ${TOKSCALE_MISSING}`);

test.skipIf(tokscale === null)(
	`tokscale ${TOKSCALE_VERSION} reads an omp session as the ledger does (skipped without tokscale on PATH: bench-only, VM)`,
	() => {
		const dir = tempDir();
		writeFileSync(join(dir, `2026-11-09T10-00-00-000Z_${SESSION_ID}.jsonl`), `${ompSession}\n`);
		const ledger = ledgerTotals([readOmpSession(ompSession, benchPricing(loadPriceTable()))]);
		const rows = compareTotals("fixture", ledger, tokscaleFor([dir]));
		expect(rows.map((row) => row.differences)).toEqual([[]]);
	},
	120_000,
);
