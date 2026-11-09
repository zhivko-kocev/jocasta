// H6 (0482, 0552): RAM per pane and in total: working set and private bytes of each process tree.
//
//   bun bench/h6-memory.ts --out <dir> [--runs 3] [--threads 0,1,4,8] [--tools omp,claude,opencode]
//                          [--project <name>] [--idle 120] [--turn] [--pty-session bench-pty] [--cwd <repo>] [--dry-run]
//
// Jocasta (sandbox; `--threads` above 0 needs `--project`): for each thread count it starts that many threads as
// `jocasta thread new` does (no Allow, 0544; each brief one short turn, so a model runs: VM only), waits `--idle`
// seconds, then sums Jocasta's processes: the service's tree (`jocasta-service.json`'s pid), the sandbox herdr server's tree
// (every pane, its omp and MCP processes) and each attached surface's tree (bare `jocasta` and its herdr client,
// `<data>/surfaces/`), each part also on its own, and each thread pane's tree on its own. Count 0 is the service,
// herdr, the surfaces and the boss alone.
// Competitors: each tool in a tab of the bench herdr session, `--idle` seconds after its prompt is ready, then with
// `--turn` once more after one scripted prompt (a model call). A tool the VM runs unsigned (0506: opencode) gets no
// prompt; h6-memory.json's `skipped` says why. Windows reads private bytes; POSIX only RSS.
// Writes h6-memory.jsonl (one snapshot per row) and h6-memory.json (per condition: median and CI of each sum).
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { readServiceFile } from "../src/shared/service-file.ts";
import { BenchUsageError, flag, listFlag, numberListFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { openSandbox } from "./lib/jocasta.ts";
import {
	BENCH_TOOLS,
	type BenchTool,
	benchHerdr,
	closePane,
	launchInPane,
	readyPatterns,
	requireTrustedCwd,
	toolCommand,
	UNSIGNED,
	waitReady,
} from "./lib/launch.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";
import { jocastaProcesses, type Memory, snapshotProcesses, treeMemory, treeOf } from "./lib/proctree.ts";
import { BENCH_PTY_SESSION } from "./lib/sandbox.ts";
import { summarize } from "./lib/stats.ts";
import { startThread, waitForEnter } from "./lib/threads.ts";

export interface H6Row {
	subject: string;
	condition: string;
	run: number;
	processes: number;
	workingSet: number;
	privateBytes: number | null;
	/** Jocasta only: its parts on their own (service, herdr's server, the surfaces), which the sums above add up. */
	parts?: { service: Memory; herdr: Memory; surfaces: Memory };
	/** Per pane, for Jocasta's thread panes. */
	perPane?: { paneId: string; workingSet: number; privateBytes: number | null }[];
}

const SHORT_BRIEF = "Benchmark H6: reply with the single word ready and change nothing.";
const TURN_PROMPT = "Reply with the single word ready.";

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 3,
			valued: ["--threads", "--tools", "--project", "--idle", "--pty-session", "--cwd", "--ready"],
			booleans: ["--turn", "--skip-jocasta"],
		});
		const threadCounts = numberListFlag(args, "threads", [0, 1, 4, 8]);
		const tools = listFlag(args, "tools", BENCH_TOOLS) as BenchTool[];
		for (const tool of tools) if (!BENCH_TOOLS.includes(tool)) throw new BenchUsageError(`unknown tool ${tool}`);
		const idleMs = Number(flag(args, "idle", "120")) * 1000;
		const project = flag(args, "project");
		const turn = args.flags.get("turn") === true;
		const skipped = turn
			? tools
					.filter((tool) => UNSIGNED[tool] !== undefined)
					.map((tool) => ({ subject: tool, condition: "after one turn", reason: UNSIGNED[tool] }))
			: [];
		const skipJocasta = args.flags.get("skip-jocasta") === true;
		const ptySession = flag(args, "pty-session", BENCH_PTY_SESSION);
		const ready = readyPatterns(flag(args, "ready"));
		const cwdFlag = flag(args, "cwd");
		if (args.dryRun) {
			printPlan(
				"H6 memory",
				[
					skipJocasta
						? "Jocasta skipped"
						: `Jocasta with ${threadCounts.join(", ")} threads in ${project ?? "<--project>"} (jocasta thread new; model turns: VM only), ${idleMs / 1000} s idle, service + herdr + surface trees summed and apart, each thread pane alone`,
					`${tools.join(", ")}: one tab each in herdr session ${ptySession}, ${idleMs / 1000} s after ready${turn ? ", again after one scripted prompt" : ""}${skipped.length > 0 ? ` (no prompt for ${skipped.map((s) => s.subject).join(", ")}: unsigned, 0506)` : ""}`,
					`${args.runs} runs per condition; working set and private bytes (Windows) per process tree`,
				],
				args.out,
			);
			return;
		}
		requireTrustedCwd(tools, cwdFlag);
		const out = args.out as string;
		const started = conditions(["omp", "claude", "opencode", "herdr", ...(skipJocasta ? [] : ["jocasta"])]);
		claimRows(out, "h6-memory");
		const rows: H6Row[] = [];
		const record = (row: H6Row) => {
			rows.push(row);
			appendRow(out, "h6-memory", row);
		};
		if (!skipJocasta) {
			const jocasta = openSandbox();
			if (threadCounts.some((n) => n > 0) && !project)
				throw new BenchUsageError("thread counts above 0 need --project");
			for (let run = 0; run < args.runs; run++) {
				// Stopping a thread and removing its worktree ask him first (0484), so the bench never closes threads.
				if (run > 0) await waitForEnter(`close run ${run - 1}'s h6 threads in the sandbox, then press Enter`);
				let opened = 0;
				for (const count of [...threadCounts].sort((a, b) => a - b)) {
					for (; opened < count; opened++)
						await startThread(project as string, `h6 run ${run} thread ${opened + 1}`, SHORT_BRIEF);
					await Bun.sleep(idleMs);
					const running = readServiceFile();
					if (!running) throw new Error("no service: <data>/jocasta-service.json is missing");
					const herdr = await jocasta.herdr();
					const roots = new Map<string, number>();
					for (const pane of await herdr.panes()) {
						const root = await herdr.paneRoot(pane.paneId).catch(() => undefined);
						if (root) roots.set(pane.paneId, root);
					}
					const processes = snapshotProcesses();
					const jocastaTrees = jocastaProcesses(await jocasta.processes(processes, roots.values()), processes);
					record({
						subject: "jocasta",
						condition: `${count} threads`,
						run,
						...treeMemory(jocastaTrees.all),
						parts: jocastaTrees.parts,
						perPane: [...roots].map(([paneId, root]) => {
							const memory = treeMemory(treeOf(root, processes));
							return { paneId, workingSet: memory.workingSet, privateBytes: memory.privateBytes };
						}),
					});
				}
			}
		}
		const cwd = cwdFlag ? resolve(cwdFlag) : mkdtempSync(join(tmpdir(), "jocasta-h6-"));
		if (!cwdFlag) writeFileSync(join(cwd, "README.md"), "# h6 fixture\n");
		if (tools.length > 0) {
			const herdr = await benchHerdr(ptySession);
			const workspace = await herdr.ensureWorkspace("bench", cwd);
			for (const tool of tools) {
				for (let run = 0; run < args.runs; run++) {
					const pane = await launchInPane(herdr, workspace, toolCommand(tool), cwd);
					try {
						if ((await waitReady(ptySession, pane.paneId, ready[tool], 60_000)) === null)
							throw new Error(`${tool} never showed its ready pattern /${ready[tool]}/`);
						const measure = async (condition: string) => {
							await Bun.sleep(idleMs);
							const shell = await herdr.paneRoot(pane.paneId, 10_000).catch(() => undefined);
							if (!shell) throw new Error(`no process for ${tool}'s pane`);
							record({ subject: tool, condition, run, ...treeMemory(treeOf(shell, snapshotProcesses())) });
						};
						await measure("idle");
						if (turn && UNSIGNED[tool] === undefined) {
							await herdr.prompt(pane.paneId, TURN_PROMPT);
							await measure("after one turn");
						}
					} finally {
						await closePane(herdr, ptySession, pane);
					}
				}
			}
			herdr.close();
		}
		const groups = [...new Set(rows.map((row) => `${row.subject}|${row.condition}`))];
		const summary = groups.map((key) => {
			const [subject, condition] = key.split("|");
			const of = rows.filter((row) => row.subject === subject && row.condition === condition);
			const privates = of.map((row) => row.privateBytes);
			return {
				subject,
				condition,
				workingSetMb: summarize(of.map((row) => row.workingSet / 2 ** 20)),
				privateMb: privates.every((value) => value !== null)
					? summarize(privates.map((value) => (value as number) / 2 ** 20))
					: null,
			};
		});
		const file = writeJson(out, "h6-memory", {
			benchmark: "H6",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			idleSeconds: idleMs / 1000,
			summary,
			skipped,
		});
		console.log(`wrote ${file}`);
	});
}
