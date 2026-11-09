// H5 (0482): startup time to a usable prompt, Jocasta against omp, Claude Code and OpenCode.
//
//   bun bench/h5-startup.ts --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <fixture repo>]
//                           [--pty-session bench-pty] [--ready omp=<regex>,…] [--timeout 60] [--dry-run]
//
// Each tool starts `--runs` times "cold" (the first start after its processes were gone for 30 s; an OS-cold start
// needs a reboot per run and is done by hand) and `--runs` times warm (straight after the previous run).
//   jocasta   (sandbox: JOCASTA_DATA_DIR, JOCASTA_HERDR_SESSION) stops the sandbox's companion and herdr, starts
//             `jocasta`, and times the companion answering `/health` (its page is served; first paint itself is
//             inside the window) and the boss pane ready (herdr reports omp idle in workspace "Jocasta").
//   omp, claude, opencode start in a tab of the bench herdr session (Bun's PTY is POSIX-only) in `--cwd`, timed until
//             the visible screen matches the tool's ready pattern (herdr `pane.wait_for_output`), then closed.
// Writes h5-startup.jsonl (one row per start) and h5-startup.json (per tool and mode: median, p95, CI).
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { TOKEN_COOKIE } from "../src/frontier/protocol.ts";
import { readFrontierFile } from "../src/frontier/server/frontier-file.ts";
import { stopHerdrSession } from "../src/frontier/server/herdr.ts";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { herdrRequest } from "./lib/herdr.ts";
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
import { appendRow, conditions, printPlan, writeJson } from "./lib/output.ts";
import { isAlive, killTree } from "./lib/proctree.ts";
import { requireSandbox } from "./lib/sandbox.ts";
import { summarize } from "./lib/stats.ts";
import { waitFor } from "./lib/timing.ts";

const TOOLS = ["jocasta", ...BENCH_TOOLS] as const;
type Tool = (typeof TOOLS)[number];

export interface H5Row {
	tool: Tool;
	mode: "cold" | "warm";
	run: number;
	/** To the usable prompt; for jocasta, to the boss pane ready. */
	readyMs: number | null;
	/** jocasta only: to the companion answering. */
	companionMs?: number | null;
}

/** Cold first, then warm, for each run index: the order of starts for one tool. */
export function schedule(runs: number): { mode: "cold" | "warm"; run: number }[] {
	return Array.from({ length: runs }, (_, run) => [
		{ mode: "cold" as const, run },
		{ mode: "warm" as const, run },
	]).flat();
}

async function stopJocasta(session: string): Promise<void> {
	const frontier = readFrontierFile();
	if (frontier && isAlive(frontier.pid)) {
		killTree(frontier.pid);
		await waitFor(() => !isAlive(frontier.pid), { timeoutMs: 15_000 });
	}
	await stopHerdrSession({ session }).catch(() => false);
}

async function startJocasta(
	session: string,
	timeoutMs: number,
): Promise<{ companionMs: number | null; readyMs: number | null }> {
	const before = readFrontierFile()?.pid;
	const started = performance.now();
	spawn("jocasta", { shell: true, detached: true, stdio: "ignore", windowsHide: true }).unref();
	const companion = await waitFor(
		async () => {
			const frontier = readFrontierFile();
			if (!frontier || frontier.pid === before) return false;
			const health = await fetch(`http://127.0.0.1:${frontier.port}/health`, {
				headers: { cookie: `${TOKEN_COOKIE}=${frontier.token}` },
			}).catch(() => undefined);
			return health?.ok === true;
		},
		{ timeoutMs, intervalMs: 50 },
	);
	const companionMs = companion === null ? null : performance.now() - started;
	const boss = await waitFor(
		async () => {
			try {
				const { workspaces } = await herdrRequest<{ workspaces: { workspace_id: string; label: string }[] }>(
					session,
					"workspace.list",
					{},
					2_000,
				);
				const home = workspaces.find((w) => w.label === "Jocasta")?.workspace_id;
				const { panes } = await herdrRequest<{
					panes: { workspace_id: string; agent?: string | null; agent_status?: string }[];
				}>(session, "pane.list", {}, 2_000);
				return panes.some((p) => p.workspace_id === home && p.agent === "omp" && p.agent_status === "idle");
			} catch {
				return false;
			}
		},
		{ timeoutMs, intervalMs: 100 },
	);
	return { companionMs, readyMs: boss === null ? null : performance.now() - started };
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
		const ptySession = flag(args, "pty-session", "bench-pty");
		const timeoutMs = Number(flag(args, "timeout", "60")) * 1000;
		const cwdFlag = flag(args, "cwd");
		if (args.dryRun) {
			printPlan(
				"H5 startup",
				[
					`${args.runs} cold and ${args.runs} warm starts of: ${tools.join(", ")}; limit ${timeoutMs / 1000} s each`,
					...tools.map((tool) =>
						tool === "jocasta"
							? "jocasta: stop the sandbox companion and herdr, start `jocasta`, time /health and the boss pane idle"
							: `${tool}: a tab in herdr session ${ptySession}, timed to the screen matching /${ready[tool as BenchTool]}/`,
					),
					`working folder: ${cwdFlag ?? (tools.includes("claude") ? "none given: Claude Code needs --cwd, a folder trusted once" : "a fresh temp git repo")}`,
				],
				args.out,
			);
			return;
		}
		requireTrustedCwd(tools, cwdFlag);
		const out = args.out as string;
		const started = conditions(["omp", "claude", "opencode", "herdr"]);
		const cwd = cwdFlag ? resolve(cwdFlag) : mkdtempSync(join(tmpdir(), "jocasta-h5-"));
		if (!cwdFlag) writeFileSync(join(cwd, "README.md"), "# h5 fixture\n");
		const rows: H5Row[] = [];
		for (const tool of tools) {
			if (tool === "jocasta") {
				const { herdrSession } = requireSandbox();
				for (const { mode, run } of schedule(args.runs)) {
					if (mode === "cold") {
						await stopJocasta(herdrSession);
						await Bun.sleep(30_000);
					} else {
						await stopJocasta(herdrSession);
					}
					const result = await startJocasta(herdrSession, timeoutMs);
					const row: H5Row = { tool, mode, run, readyMs: result.readyMs, companionMs: result.companionMs };
					rows.push(row);
					appendRow(out, "h5-startup", row);
				}
				await stopJocasta(herdrSession);
				continue;
			}
			const herdr = await benchHerdr(ptySession);
			const workspace = await herdr.ensureWorkspace("bench", cwd);
			// Resolved before any start is timed (`toolCommand`: herdr can't start npm's extensionless shim).
			const command = toolCommand(tool);
			for (const { mode, run } of schedule(args.runs)) {
				if (mode === "cold") await Bun.sleep(30_000);
				const t0 = performance.now();
				const pane = await launchInPane(herdr, workspace, command, cwd);
				const waited = await waitReady(ptySession, pane.paneId, ready[tool], timeoutMs);
				const row: H5Row = { tool, mode, run, readyMs: waited === null ? null : performance.now() - t0 };
				rows.push(row);
				appendRow(out, "h5-startup", row);
				await closePane(ptySession, pane);
			}
			herdr.close();
		}
		const summary = Object.fromEntries(
			tools.map((tool) => [
				tool,
				Object.fromEntries(
					(["cold", "warm"] as const).map((mode) => {
						const of = rows.filter((row) => row.tool === tool && row.mode === mode);
						return [
							mode,
							{
								ready: summarize(of.map((row) => row.readyMs).filter((ms): ms is number => ms !== null)),
								failed: of.filter((row) => row.readyMs === null).length,
								...(tool === "jocasta"
									? {
											companion: summarize(
												of.map((row) => row.companionMs ?? null).filter((ms): ms is number => ms !== null),
											),
										}
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
