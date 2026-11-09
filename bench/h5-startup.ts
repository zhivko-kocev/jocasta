// H5 (0482, 0576): startup time to a usable prompt, Jocasta against omp, Claude Code and OpenCode.
//
//   bun bench/h5-startup.ts --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <fixture repo>]
//                           [--pty-session bench-pty] [--ready omp=<regex>,…] [--timeout 60] [--dry-run]
//
// Each tool starts `--runs` times "cold" (the first start after its processes were gone for 30 s; an OS-cold start
// needs a reboot per run and is done by hand) and `--runs` times warm (straight after the previous run).
//   jocasta   (sandbox: JOCASTA_DATA_DIR, JOCASTA_HERDR_SESSION, JOCASTA_CODING_AGENT_DIR) starts `jocasta` and times
//             the boss pane's screen matching omp's ready pattern (`readyMs`, the same end point as omp alone, 0576),
//             herdr reporting the boss's omp idle (`bossIdleMs`), and the new service answering `/health`
//             (`serviceMs`). Between runs it closes Jocasta the product's way (Stop everything, `/api/shutdown`) and
//             waits until `jocasta-service.json` is gone and the started surface has exited (killed after 60 s as a fallback).
//   omp, claude, opencode start in a tab of the bench herdr session (Bun's PTY is POSIX-only) in `--cwd`, timed until
//             the visible screen matches the tool's ready pattern (herdr `pane.wait_for_output`), then closed.
// Writes h5-startup.jsonl (one row per start) and h5-startup.json (per tool and mode: median, p95, CI). Rows before
// 0576 named `serviceMs` `companionMs` and had no `bossIdleMs`; `rowServiceMs` reads both.
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { BOSS_WORKSPACE, isTodayPane } from "../src/frontier/server/boss.ts";
import { type HerdrSnapshot, type PaneInfo, stopHerdrSession } from "../src/frontier/server/herdr.ts";
import { askShutdown } from "../src/frontier/server/launch.ts";
import { killTree, pidAlive } from "../src/shared/proctree.ts";
import { readServiceFile } from "../src/shared/service-file.ts";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { openSandbox, type SandboxJocasta } from "./lib/jocasta.ts";
import {
	BENCH_TOOLS,
	type BenchTool,
	benchHerdr,
	closePane,
	launchInPane,
	readyPatterns,
	requireTrustedCwd,
	toolCommand,
	waitReady,
} from "./lib/launch.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";
import { BENCH_PTY_SESSION } from "./lib/sandbox.ts";
import { summarize } from "./lib/stats.ts";
import { waitFor } from "./lib/timing.ts";

const TOOLS = ["jocasta", ...BENCH_TOOLS] as const;
type Tool = (typeof TOOLS)[number];

/** How long Jocasta's own close gets before H5 ends what is left (BEN-9). */
const CLOSE_MS = 60_000;

export interface H5Row {
	tool: Tool;
	mode: "cold" | "warm";
	run: number;
	/** To the usable prompt: the tool's ready pattern on its screen; for jocasta, on the boss pane. */
	readyMs: number | null;
	/** jocasta only: to herdr reporting the boss pane's omp idle. */
	bossIdleMs?: number | null;
	/** jocasta only: to the new service answering `/health`. */
	serviceMs?: number | null;
}

/** A row's service time, under its name since 0576 or the name rows had before (`companionMs`). */
export function rowServiceMs(row: H5Row & { companionMs?: number | null }): number | null {
	return row.serviceMs ?? row.companionMs ?? null;
}

/** Cold first, then warm, for each run index: the order of starts for one tool. */
export function schedule(runs: number): { mode: "cold" | "warm"; run: number }[] {
	return Array.from({ length: runs }, (_, run) => [
		{ mode: "cold" as const, run },
		{ mode: "warm" as const, run },
	]).flat();
}

/**
 * Closes the sandbox Jocasta as he does (Stop everything, 0549): asks the service to shut down, waits until
 * `jocasta-service.json` is gone and `surface` (the `jocasta` H5 started) has exited; past `CLOSE_MS` it ends what is left and
 * stops the session's herdr server.
 */
async function stopJocasta(jocasta: SandboxJocasta, surface: number | undefined): Promise<void> {
	const running = readServiceFile();
	if (running && (await jocasta.health(running)) === "up") await askShutdown(running);
	const closed = await waitFor(() => readServiceFile() === undefined && (surface === undefined || !pidAlive(surface)), {
		timeoutMs: CLOSE_MS,
		intervalMs: 250,
	});
	if (closed !== null) return;
	console.error(`Jocasta didn't close within ${CLOSE_MS / 1000} s; ending its processes`);
	if (surface !== undefined && pidAlive(surface)) await killTree(surface).catch(() => []);
	const left = readServiceFile();
	if (left && pidAlive(left.pid)) await killTree(left.pid).catch(() => []);
	await stopHerdrSession({ session: jocasta.sandbox.herdrSession }).catch(() => false);
}

/** The boss pane in herdr's lists: the pane in workspace "Jocasta" that isn't the today pane. */
export function bossPane<T extends Pick<PaneInfo, "workspaceId" | "label" | "agent">>(
	workspaces: readonly Pick<HerdrSnapshot["workspaces"][number], "id" | "label">[],
	panes: readonly T[],
): T | undefined {
	const home = workspaces.find((w) => w.label === BOSS_WORKSPACE)?.id;
	return panes.find((p) => home !== undefined && p.workspaceId === home && !isTodayPane(p));
}

/** How long each of H5's herdr reads may take while Jocasta starts. */
const LIST_MS = 2_000;

async function listBoss(jocasta: SandboxJocasta): Promise<PaneInfo | undefined> {
	try {
		const { workspaces, panes } = await (await jocasta.herdr()).snapshot();
		return bossPane(workspaces, panes);
	} catch {
		return undefined;
	}
}

async function startJocasta(
	jocasta: SandboxJocasta,
	readyPattern: string,
	timeoutMs: number,
): Promise<{ surface: number | undefined; row: Pick<H5Row, "readyMs" | "bossIdleMs" | "serviceMs"> }> {
	const before = readServiceFile()?.pid;
	const started = performance.now();
	const surface = spawn("jocasta", { shell: true, detached: true, stdio: "ignore", windowsHide: true });
	surface.unref();
	const since = () => performance.now() - started;
	const service = (async () => {
		const up = await waitFor(
			async () => {
				const running = readServiceFile();
				return running !== undefined && running.pid !== before && (await jocasta.health(running)) === "up";
			},
			{ timeoutMs, intervalMs: 50 },
		);
		return up === null ? null : since();
	})();
	const idle = (async () => {
		const seen = await waitFor(async () => (await listBoss(jocasta))?.agentState === "idle", {
			timeoutMs,
			intervalMs: 100,
		});
		return seen === null ? null : since();
	})();
	const ready = (async () => {
		let paneId: string | undefined;
		await waitFor(
			async () => {
				paneId = (await listBoss(jocasta))?.paneId;
				return paneId !== undefined;
			},
			{ timeoutMs, intervalMs: 100 },
		);
		if (paneId === undefined) return null;
		const left = timeoutMs - since();
		if (left <= 0) return null;
		return (await waitReady(jocasta.sandbox.herdrSession, paneId, readyPattern, left)) === null ? null : since();
	})();
	const [serviceMs, bossIdleMs, readyMs] = await Promise.all([service, idle, ready]);
	return { surface: surface.pid, row: { readyMs, bossIdleMs, serviceMs } };
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 20,
			valued: ["--tools", "--cwd", "--pty-session", "--ready", "--timeout"],
		});
		const tools = listFlag(args, "tools", TOOLS) as Tool[];
		for (const tool of tools) if (!TOOLS.includes(tool)) throw new BenchUsageError(`unknown tool ${tool}`);
		const ready = readyPatterns(flag(args, "ready"));
		const ptySession = flag(args, "pty-session", BENCH_PTY_SESSION);
		const timeoutMs = Number(flag(args, "timeout", "60")) * 1000;
		const cwdFlag = flag(args, "cwd");
		if (args.dryRun) {
			printPlan(
				"H5 startup",
				[
					`${args.runs} cold and ${args.runs} warm starts of: ${tools.join(", ")}; limit ${timeoutMs / 1000} s each`,
					...tools.map((tool) =>
						tool === "jocasta"
							? `jocasta: start \`jocasta\`, time the boss pane matching /${ready.omp}/ (readyMs), the boss idle (bossIdleMs) and /health (serviceMs); close it with Stop everything between runs`
							: `${tool}: a tab in herdr session ${ptySession}, timed to the screen matching /${ready[tool as BenchTool]}/`,
					),
					`working folder: ${cwdFlag ?? (tools.includes("claude") ? "none given: Claude Code needs --cwd, a folder trusted once" : "a fresh temp folder with a README")}`,
				],
				args.out,
			);
			return;
		}
		requireTrustedCwd(tools, cwdFlag);
		const out = args.out as string;
		const jocasta = tools.includes("jocasta") ? openSandbox({ herdrTimeoutMs: LIST_MS }) : undefined;
		claimRows(out, "h5-startup");
		const started = conditions(["omp", "claude", "opencode", "herdr", ...(jocasta ? ["jocasta"] : [])]);
		const cwd = cwdFlag ? resolve(cwdFlag) : mkdtempSync(join(tmpdir(), "jocasta-h5-"));
		if (!cwdFlag) writeFileSync(join(cwd, "README.md"), "# h5 fixture\n");
		const rows: H5Row[] = [];
		for (const tool of tools) {
			if (tool === "jocasta" && jocasta) {
				let surface: number | undefined;
				for (const { mode, run } of schedule(args.runs)) {
					await stopJocasta(jocasta, surface);
					if (mode === "cold") await Bun.sleep(30_000);
					const result = await startJocasta(jocasta, ready.omp, timeoutMs);
					surface = result.surface;
					const row: H5Row = { tool, mode, run, ...result.row };
					rows.push(row);
					appendRow(out, "h5-startup", row);
				}
				await stopJocasta(jocasta, surface);
				continue;
			}
			const herdr = await benchHerdr(ptySession);
			const workspace = await herdr.ensureWorkspace("bench", cwd);
			// Resolved before any start is timed (`toolCommand`: herdr can't start npm's extensionless shim).
			const command = toolCommand(tool as BenchTool);
			for (const { mode, run } of schedule(args.runs)) {
				if (mode === "cold") await Bun.sleep(30_000);
				const t0 = performance.now();
				const pane = await launchInPane(herdr, workspace, command, cwd);
				const waited = await waitReady(ptySession, pane.paneId, ready[tool as BenchTool], timeoutMs);
				const row: H5Row = { tool, mode, run, readyMs: waited === null ? null : performance.now() - t0 };
				rows.push(row);
				appendRow(out, "h5-startup", row);
				await closePane(herdr, ptySession, pane);
			}
			herdr.close();
		}
		const times = (of: readonly H5Row[], pick: (row: H5Row) => number | null | undefined) =>
			summarize(of.map((row) => pick(row) ?? null).filter((ms): ms is number => ms !== null));
		const summary = Object.fromEntries(
			tools.map((tool) => [
				tool,
				Object.fromEntries(
					(["cold", "warm"] as const).map((mode) => {
						const of = rows.filter((row) => row.tool === tool && row.mode === mode);
						return [
							mode,
							{
								ready: times(of, (row) => row.readyMs),
								failed: of.filter((row) => row.readyMs === null).length,
								...(tool === "jocasta"
									? { bossIdle: times(of, (row) => row.bossIdleMs), service: times(of, rowServiceMs) }
									: {}),
							},
						];
					}),
				),
			]),
		);
		const file = writeJson(out, "h5-startup", {
			benchmark: "H5",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			readyPatterns: ready,
			summary,
		});
		console.log(`wrote ${file}`);
	});
}
