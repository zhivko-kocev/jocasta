// The ledger's parser check (0538): the benchmark's ledger totals per session against tokscale's, bench-only.
//
//   bun bench/tokscale-parity.ts --out <task-benchmark runDir> [--dry-run]
//
// For every trial of the run (`results.jsonl` names each trial's harness), the ledger reads its saved sessions
// (`sessions/<task>-<arm>-<rep>/`) and sums uncached input, cache read, cache write and output per session; tokscale
// reads the same sessions where it finds them: omp's saved folders through `TOKSCALE_EXTRA_DIRS=omp:<dir>,…` (omp ran
// with `--session-dir` in the trial root), Claude Code's transcripts and opencode's database in their default places on
// the VM, and `tokscale models --json --group-by client,session,model` gives its totals per session. Tokens must match
// exactly; dollars aren't compared (tokscale prices with today's LiteLLM, the ledger with the pinned table). Writes
// tokscale-parity.jsonl (one row per session, rewritten on each check) and tokscale-parity.json; exits 1 on any
// mismatch and 2 when no saved session was compared.
//
// tokscale is approved for the benchmark only (0538) and isn't installed on his PC: pinned v4.21.0 (2026-11-02, MIT),
// the Windows binary from https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0, on PATH in the VM. Without it the
// script (and its test) say so and stop.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { removeTree } from "../src/shared/fsx.ts";
import type { LedgerHarness, SessionRecord } from "../src/shared/ledger.ts";
import { BenchUsageError, parseBenchArgs, runScript } from "./lib/args.ts";
import { benchPricing, readTrialSessions } from "./lib/ledger.ts";
import { conditions, printPlan, writeJson, writeRows } from "./lib/output.ts";
import { loadPriceTable } from "./lib/prices.ts";

export const TOKSCALE_VERSION = "4.21.0";
export const TOKSCALE_RELEASE = "https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0";
/** What a skipped check says. */
export const TOKSCALE_MISSING = `tokscale is not on PATH: the parity check is bench-only (0538); install tokscale ${TOKSCALE_VERSION} from ${TOKSCALE_RELEASE} in the VM`;

export interface SessionTotals {
	input: number;
	cacheRead: number;
	cacheWrite: number;
	output: number;
}

const zero = (): SessionTotals => ({ input: 0, cacheRead: 0, cacheWrite: 0, output: 0 });

/**
 * tokscale's `models --json --group-by client,session,model` summed per session over its models. opencode's
 * reasoning tokens join its output, as the ledger counts them; Claude Code's and omp's output already holds them.
 */
export function tokscaleTotals(json: string): Map<string, SessionTotals & { client: string }> {
	const parsed = JSON.parse(json) as { entries?: Record<string, unknown>[] };
	const totals = new Map<string, SessionTotals & { client: string }>();
	const num = (value: unknown) => (typeof value === "number" ? value : 0);
	for (const entry of parsed.entries ?? []) {
		if (typeof entry.sessionId !== "string") continue;
		const client = String(entry.client ?? "");
		const sum = totals.get(entry.sessionId) ?? { ...zero(), client };
		sum.input += num(entry.input);
		sum.cacheRead += num(entry.cacheRead);
		sum.cacheWrite += num(entry.cacheWrite);
		sum.output += num(entry.output) + (client === "opencode" ? num(entry.reasoning) : 0);
		totals.set(entry.sessionId, sum);
	}
	return totals;
}

/** The ledger's totals per session id (a Claude Code subagent's transcript carries its parent's id, as in tokscale). */
export function ledgerTotals(
	records: readonly SessionRecord[],
): Map<string, SessionTotals & { harness: LedgerHarness }> {
	const totals = new Map<string, SessionTotals & { harness: LedgerHarness }>();
	for (const record of records) {
		const sum = totals.get(record.session) ?? { ...zero(), harness: record.harness };
		for (const row of record.requests) {
			sum.input += row.input;
			sum.cacheRead += row.cacheRead;
			sum.cacheWrite += row.cacheWrite;
			sum.output += row.output;
		}
		totals.set(record.session, sum);
	}
	return totals;
}

export interface ParityRow {
	trial: string;
	session: string;
	harness: LedgerHarness;
	ledger: SessionTotals;
	/** tokscale's totals for the session; null when it doesn't list it. */
	tokscale: SessionTotals | null;
	/** The fields that differ, or `missing in tokscale`. */
	differences: string[];
}

const FIELDS = ["input", "cacheRead", "cacheWrite", "output"] as const;

/**
 * One row per ledger session: tokscale's session whose id is the ledger's or contains it (omp's file names carry the
 * session id after a timestamp), and the token fields that differ.
 */
export function compareTotals(
	trial: string,
	ledger: ReadonlyMap<string, SessionTotals & { harness: LedgerHarness }>,
	tokscale: ReadonlyMap<string, SessionTotals>,
): ParityRow[] {
	return [...ledger].map(([session, ours]) => {
		const theirs = tokscale.get(session) ?? [...tokscale].find(([id]) => id.includes(session))?.[1] ?? null;
		const { harness, ...counts } = ours;
		return {
			trial,
			session,
			harness,
			ledger: counts,
			tokscale:
				theirs === null
					? null
					: { input: theirs.input, cacheRead: theirs.cacheRead, cacheWrite: theirs.cacheWrite, output: theirs.output },
			differences: theirs === null ? ["missing in tokscale"] : FIELDS.filter((field) => theirs[field] !== ours[field]),
		};
	});
}

/** Windows caps an environment block's variable at 32,767 characters; folders go to tokscale in runs under this. */
const EXTRA_DIRS_LIMIT = 24_000;

/** `tokscale models --json --group-by client,session,model` with `extraDirs` added, in a throwaway config folder. */
export function runTokscale(extraDirs: readonly string[]): string {
	const tokscale = Bun.which("tokscale");
	if (tokscale === null) throw new BenchUsageError(TOKSCALE_MISSING);
	const config = mkdtempSync(join(tmpdir(), "tokscale-"));
	try {
		const result = spawnSync(tokscale, ["models", "--json", "--group-by", "client,session,model"], {
			encoding: "utf8",
			windowsHide: true,
			maxBuffer: 256 * 1024 * 1024,
			timeout: 600_000,
			env: { ...process.env, TOKSCALE_CONFIG_DIR: config, TOKSCALE_EXTRA_DIRS: extraDirs.join(",") },
		});
		if (result.status !== 0) throw new Error(`tokscale models failed: ${result.stderr || result.error?.message}`);
		return result.stdout;
	} finally {
		removeTree(config);
	}
}

/** tokscale's totals over every run of `omp:<dir>` folders that fits {@link EXTRA_DIRS_LIMIT}; one run without any. */
export function tokscaleFor(ompDirs: readonly string[]): Map<string, SessionTotals> {
	const batches: string[][] = [[]];
	for (const dir of ompDirs) {
		const entry = `omp:${dir}`;
		const batch = batches.at(-1) as string[];
		if (batch.length > 0 && batch.join(",").length + entry.length + 1 > EXTRA_DIRS_LIMIT) batches.push([entry]);
		else batch.push(entry);
	}
	const totals = new Map<string, SessionTotals>();
	for (const batch of batches) for (const [id, sum] of tokscaleTotals(runTokscale(batch))) totals.set(id, sum);
	return totals;
}

if (import.meta.main)
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), { defaultRuns: 1 });
		if (args.dryRun) {
			printPlan(
				"tokscale parity",
				[
					"per trial of the run: the ledger's per-session tokens from sessions/<trial>/",
					`tokscale ${TOKSCALE_VERSION} models --json --group-by client,session,model, omp's saved folders as TOKSCALE_EXTRA_DIRS`,
					"compare uncached input, cache read, cache write and output exactly; any mismatch exits 1, nothing compared exits 2",
					Bun.which("tokscale") === null ? TOKSCALE_MISSING : `tokscale at ${Bun.which("tokscale")}`,
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const resultsFile = join(out, "results.jsonl");
		if (!existsSync(resultsFile)) throw new BenchUsageError(`no task-benchmark results at ${resultsFile}`);
		const harnessOf = new Map<string, LedgerHarness>();
		for (const line of readFileSync(resultsFile, "utf8").split(/\r?\n/)) {
			if (line.trim() === "") continue;
			const row = JSON.parse(line) as { task: string; arm: string; rep: number; harness: LedgerHarness };
			harnessOf.set(`${row.task}-${row.arm}-${row.rep}`, row.harness);
		}
		const sessionsRoot = join(out, "sessions");
		const trials = existsSync(sessionsRoot) ? readdirSync(sessionsRoot).filter((name) => harnessOf.has(name)) : [];
		const prices = loadPriceTable();
		const started = conditions(["tokscale", "omp", "claude", "opencode"]);
		const ledgers = trials.map((trial) => {
			const harness = harnessOf.get(trial) as LedgerHarness;
			return {
				trial,
				harness,
				totals: ledgerTotals(readTrialSessions(join(sessionsRoot, trial), harness, benchPricing(prices))),
			};
		});
		// Nothing to compare is no pass (0576): a wrong run folder or renamed session folders exit 2.
		if (ledgers.every((l) => l.totals.size === 0))
			throw new BenchUsageError(
				`no saved sessions to compare in ${sessionsRoot} (results.jsonl names ${harnessOf.size} trials)`,
			);
		if (Bun.which("tokscale") === null) throw new BenchUsageError(TOKSCALE_MISSING);
		const tokscale = tokscaleFor(ledgers.filter((l) => l.harness === "omp").map((l) => join(sessionsRoot, l.trial)));
		const rows = ledgers.flatMap((l) => compareTotals(l.trial, l.totals, tokscale));
		// Derived from the run's own files: each check rewrites its rows.
		writeRows(out, "tokscale-parity", rows);
		const mismatched = rows.filter((row) => row.differences.length > 0);
		const file = writeJson(out, "tokscale-parity", {
			check: "0538 tokscale parity",
			tokscaleVersion: TOKSCALE_VERSION,
			conditions: { ...started, finishedAt: new Date().toISOString() },
			sessions: rows.length,
			mismatched: mismatched.length,
			byHarness: Object.fromEntries(
				(["omp", "claude", "opencode"] as const).map((h) => [
					h,
					{
						sessions: rows.filter((r) => r.harness === h).length,
						mismatched: mismatched.filter((r) => r.harness === h).length,
					},
				]),
			),
		});
		console.log(`${rows.length - mismatched.length}/${rows.length} sessions match tokscale; wrote ${file}`);
		if (mismatched.length > 0) process.exitCode = 1;
	});
