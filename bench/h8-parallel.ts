// H8 (0482, extended by 0543 K11): parallel isolation and overlap detection, in a sandbox Jocasta (VM: the threads'
// agents call the model).
//
//   bun bench/h8-parallel.ts --out <dir> --project <name> --repo <main checkout> [--threads 2,4,8] [--runs 1]
//                            [--layer on|off] [--timeout 60] [--make-fixture] [--dry-run]
//
// `--make-fixture` writes the fixture into `--repo` (an empty folder that becomes a git repo; register it as a sandbox
// project before the run). Threads come in pairs: an even pair both edit the same file (`src/mod<k>.ts`), an odd pair
// edits one interface from both sides (`src/shared.ts` and `src/use<k>.ts`), in the style of "Passes Alone, Fails
// Together". Each thread starts with `thread_start` (boss key in JOCASTA_BOSS_KEY); threads open in plan mode, so he
// approves each plan. The script polls `thread_status` until every thread of the run is finished (0529: idle or done
// with a commit on its branch; a fresh thread reads idle before its brief lands and a waiting plan reads done), then
// measures:
//   wrong-thread edits  files a thread changed outside its own assignment, plus any change in the main checkout;
//   flagged before merge  pairs whose `thread_status` line names the partner under "overlaps" (0456);
//   conflicts at merge  pairs whose branches conflict in `git merge-tree --write-tree`;
//   found only at merge  conflicts the overlaps view didn't flag.
// K11 (`--layer on|off`, 0543): 4 threads at once, 3 runs, with the harness-OS layer on and then off in the sandbox
// (switched by hand; the flag labels the run). Each run also measures the cost per finished thread (each thread pane's
// omp session file, as herdr reports it, priced by the benchmark's ledger at the pinned table), the peak working set
// and private bytes of the service's and herdr's process trees (threads included; sampled every 10 s), and, layer on,
// the run's shared check hits and index reuse, as the sandbox service counts them in its data dir.
// Writes h8-parallel.jsonl (one row per thread count and run) and h8-parallel.json.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { checksDir, readCheckHits } from "../src/frontier/server/checks.ts";
import { readIndexReuse } from "../src/frontier/server/code-index.ts";
import { readFrontierFile } from "../src/frontier/server/frontier-file.ts";
import { connectHerdr, type PaneInfo } from "../src/frontier/server/herdr.ts";
import { readOmpSession, trialLedger } from "../src/shared/ledger.ts";
import { BenchUsageError, flag, numberListFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { herdrRequest, paneShellPid } from "./lib/herdr.ts";
import { benchPricing } from "./lib/ledger.ts";
import { appendRow, conditions, printPlan, writeJson } from "./lib/output.ts";
import { loadPriceTable } from "./lib/prices.ts";
import { snapshotProcesses, treeMemory, treeOf } from "./lib/proctree.ts";
import { callTool, requireSandbox } from "./lib/sandbox.ts";

/** K11's run (0543): 4 threads at once, 3 runs per layer state. */
export const K11_THREADS = 4;
export const K11_RUNS = 3;
const MEMORY_SAMPLE_MS = 10_000;

/** The layer's shared-service counters (K11), as the service counts them. */
export interface LayerCounters {
	checkHits: number;
	indexReuse: number;
}

/**
 * K11's shared check hits and index reuse so far in a Jocasta data dir: the check runner logs one line per result
 * reused or run joined (`<data>/checks/hits.log`, 0542, 0559), the code index one per query its existing index served
 * (`<data>/code-index/reuse.log`, 0562). A run's own counts are the difference across it.
 */
export function readLayerCounters(dataDir: string): LayerCounters {
	return { checkHits: readCheckHits(checksDir(dataDir)), indexReuse: readIndexReuse(dataDir) };
}

const samePath = (a: string, b: string) =>
	resolve(a).replaceAll("\\", "/").toLowerCase() === resolve(b).replaceAll("\\", "/").toLowerCase();

/**
 * Each thread's omp session files: the session file herdr reports for the pane whose folder is the thread's worktree,
 * and its subagents' files in the folder beside it (`<session>/…/*.jsonl`).
 */
export function threadSessionFiles(
	panes: readonly Pick<PaneInfo, "cwd" | "sessionPath">[],
	worktrees: ReadonlyMap<string, string>,
): Map<string, string[]> {
	const files = new Map<string, string[]>();
	for (const [branch, worktree] of worktrees) {
		const pane = panes.find((p) => p.cwd !== undefined && p.sessionPath !== undefined && samePath(p.cwd, worktree));
		if (!pane?.sessionPath) continue;
		const beside = pane.sessionPath.replace(/\.jsonl$/, "");
		const subagents = existsSync(beside)
			? readdirSync(beside, { recursive: true, encoding: "utf8" })
					.filter((name) => name.endsWith(".jsonl"))
					.map((name) => join(beside, name))
			: [];
		files.set(branch, [pane.sessionPath, ...subagents]);
	}
	return files;
}
/** The fixture: one shared interface file, a module and a user per pair, and a package file. */
export function fixtureFiles(maxThreads: number): Record<string, string> {
	const pairs = Math.ceil(maxThreads / 2);
	const files: Record<string, string> = {
		"package.json": `${JSON.stringify({ name: "acme-parallel", private: true, type: "module" }, null, 2)}\n`,
		"src/shared.ts": Array.from(
			{ length: pairs },
			(_, k) => `export interface Item${k} {\n\tid: string;\n\tlabel: string;\n}\n`,
		).join("\n"),
	};
	for (let k = 0; k < pairs; k++) {
		files[`src/mod${k}.ts`] = `export function base${k}(value: number): number {\n\treturn value;\n}\n`;
		files[`src/use${k}.ts`] =
			`import type { Item${k} } from "./shared.ts";\n\nexport function show${k}(item: Item${k}): string {\n\treturn item.label;\n}\n`;
	}
	return files;
}

export interface Assignment {
	thread: number;
	pair: number;
	title: string;
	files: string[];
	brief: string;
}

/** Thread i's assignment among n: pair ⌊i/2⌋; even pairs share a file, odd pairs share an interface. */
export function assignments(n: number, run: number): Assignment[] {
	return Array.from({ length: n }, (_, thread) => {
		const pair = Math.floor(thread / 2);
		const first = thread % 2 === 0;
		const title = `h8 r${run} n${n} t${thread}`;
		if (pair % 2 === 0) {
			const file = `src/mod${pair}.ts`;
			return {
				thread,
				pair,
				title,
				files: [file],
				brief: first
					? `Benchmark H8: in ${file}, change base${pair} so it returns value * 2. Change no other file. Commit when done.`
					: `Benchmark H8: in ${file}, change base${pair} so it returns value + 1. Change no other file. Commit when done.`,
			};
		}
		return first
			? {
					thread,
					pair,
					title,
					files: ["src/shared.ts"],
					brief: `Benchmark H8: in src/shared.ts, rename Item${pair}'s field label to title. Change no other file. Commit when done.`,
				}
			: {
					thread,
					pair,
					title,
					files: [`src/use${pair}.ts`],
					brief: `Benchmark H8: in src/use${pair}.ts, make show${pair} return the label in upper case. Change no other file. Commit when done.`,
				};
	});
}

export interface ThreadLine {
	title: string;
	branch: string;
	state: string;
	overlaps: string[];
}

/** One `thread_status` line: `<project>/<title> [<branch>] <state>[ — …][ · overlaps <title>: files; …]`. */
export function parseThreadLine(line: string): ThreadLine | null {
	const match = /^[^/]+\/(.+?) \[([^\]]+)\] (done|plan ready|awaiting you|idle|working|blocked|unknown)(.*)$/.exec(
		line.trim(),
	);
	if (!match) return null;
	const overlapsAt = (match[4] ?? "").indexOf(" · overlaps ");
	const overlaps =
		overlapsAt < 0
			? []
			: (match[4] ?? "")
					.slice(overlapsAt + " · overlaps ".length)
					.split("; ")
					.map((part) => part.slice(0, part.lastIndexOf(": ")).trim())
					.filter((title) => title !== "");
	return { title: match[1] ?? "", branch: match[2] ?? "", state: match[3] ?? "", overlaps };
}

/**
 * A thread is finished when it is idle or done and its branch has a commit beyond `main` (0529): each brief ends
 * "Commit when done", while a fresh thread reads idle before its brief lands and a waiting plan reads done.
 */
export function finished(line: ThreadLine, commitsAhead: number): boolean {
	return (line.state === "idle" || line.state === "done") && commitsAhead > 0;
}

export interface PairResult {
	pair: number;
	flagged: boolean;
	conflict: boolean;
}

export interface RunScore {
	threads: number;
	wrongThreadEdits: number;
	mainCheckoutChanges: number;
	pairs: PairResult[];
	flaggedBeforeMerge: number;
	conflictsAtMerge: number;
	foundOnlyAtMerge: number;
}

/** K11's measures of one run (0543); null fields where the run has none. */
export interface K11Measures {
	layer: "on" | "off";
	finishedThreads: number;
	/** Dollars over every thread's sessions at the pinned price table. */
	costUsd: number;
	costPerFinishedThread: number | null;
	/** Models the price table lacks; their requests aren't in `costUsd`. */
	unpricedModels: string[];
	peakWorkingSetMb: number | null;
	peakPrivateMb: number | null;
	checkHits: number | null;
	indexReuse: number | null;
}

export function score(
	work: readonly Assignment[],
	lines: readonly ThreadLine[],
	changed: ReadonlyMap<string, readonly string[]>,
	conflicts: ReadonlyMap<number, boolean>,
	mainChanges: number,
): RunScore {
	const byTitle = new Map(lines.map((line) => [line.title, line]));
	let wrong = 0;
	for (const assignment of work) {
		const files = changed.get(assignment.title) ?? [];
		wrong += files.filter((file) => !assignment.files.includes(file)).length;
	}
	const pairs = [...new Set(work.map((a) => a.pair))].map((pair) => {
		const [a, b] = work.filter((x) => x.pair === pair);
		const flagged =
			a !== undefined &&
			b !== undefined &&
			((byTitle.get(a.title)?.overlaps.includes(b.title) ?? false) ||
				(byTitle.get(b.title)?.overlaps.includes(a.title) ?? false));
		return { pair, flagged, conflict: conflicts.get(pair) ?? false };
	});
	return {
		threads: work.length,
		wrongThreadEdits: wrong,
		mainCheckoutChanges: mainChanges,
		pairs,
		flaggedBeforeMerge: pairs.filter((p) => p.flagged).length,
		conflictsAtMerge: pairs.filter((p) => p.conflict).length,
		foundOnlyAtMerge: pairs.filter((p) => p.conflict && !p.flagged).length,
	};
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: ["--project", "--repo", "--threads", "--timeout", "--layer"],
			booleans: ["--make-fixture"],
		});
		const layerText = flag(args, "layer");
		if (layerText !== undefined && layerText !== "on" && layerText !== "off")
			throw new BenchUsageError(`--layer is on or off, not "${layerText}"`);
		const layer = layerText as "on" | "off" | undefined;
		const counts = numberListFlag(args, "threads", layer ? [K11_THREADS] : [2, 4, 8]);
		const runs = layer && !process.argv.includes("--runs") ? K11_RUNS : args.runs;
		const project = flag(args, "project");
		const repoFlag = flag(args, "repo");
		const timeoutMs = Number(flag(args, "timeout", "60")) * 60_000;
		if (args.dryRun) {
			printPlan(
				"H8 parallel isolation",
				[
					args.flags.get("make-fixture") === true
						? `write the fixture into ${repoFlag ?? "<--repo>"} for up to ${Math.max(...counts)} threads`
						: "use the existing fixture",
					`${counts.join(", ")} threads × ${runs} run(s) in ${project ?? "<--project>"} via thread_start (model calls; he approves each plan: VM only)`,
					...assignments(Math.min(4, Math.max(...counts)), 0).map((a) => `thread ${a.thread}: ${a.brief}`),
					`poll thread_status up to ${timeoutMs / 60_000} min until all are idle or done with a commit on their branch`,
					"measure wrong-thread edits, overlaps flagged before merge, merge-tree conflicts, conflicts found only at merge",
					...(layer
						? [
								`K11 (0543): the layer ${layer} in the sandbox (set by hand); cost per finished thread from each pane's omp session at the pinned price table; peak memory of the service's and herdr's trees every ${MEMORY_SAMPLE_MS / 1000} s`,
								layer === "on"
									? "shared check hits and index reuse from the layer build (needed: a layer-on run stops without it)"
									: "no shared check hits or index reuse with the layer off",
							]
						: []),
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		if (!project || !repoFlag) throw new BenchUsageError("--project and --repo are required");
		const repo = resolve(repoFlag);
		const git = (cwd: string, gitArgs: string[]) =>
			spawnSync("git", gitArgs, { cwd, encoding: "utf8", windowsHide: true });
		if (args.flags.get("make-fixture") === true) {
			for (const [path, text] of Object.entries(fixtureFiles(Math.max(...counts)))) {
				mkdirSync(join(repo, path, ".."), { recursive: true });
				writeFileSync(join(repo, path), text);
			}
			for (const gitArgs of [
				["init", "-q", "-b", "main"],
				["add", "-A"],
				["-c", "user.name=Bench", "-c", "user.email=bench@example.org", "commit", "-q", "-m", "h8 fixture"],
			])
				git(repo, gitArgs);
			console.log(
				`fixture in ${repo}; add it as a sandbox project named ${project}, then run again without --make-fixture`,
			);
			return;
		}
		const sandbox = requireSandbox();
		const bossKey = process.env.JOCASTA_BOSS_KEY;
		if (!bossKey) throw new BenchUsageError("thread_start and thread_status need the boss key in JOCASTA_BOSS_KEY");
		const prices = layer ? loadPriceTable() : undefined;
		const started = conditions(["omp", "herdr", "git"]);
		const scores: (RunScore & { run: number } & Partial<K11Measures>)[] = [];
		for (const n of counts) {
			for (let run = 0; run < runs; run++) {
				const work = assignments(n, run);
				const countedBefore = layer === "on" ? readLayerCounters(sandbox.dataDir) : null;
				let peakWorkingSet: number | null = null;
				let peakPrivate: number | null = null;
				let sampling = layer !== undefined;
				const sampler = (async () => {
					while (sampling) {
						try {
							const frontier = readFrontierFile();
							const { panes } = await herdrRequest<{ panes: { pane_id: string }[] }>(
								sandbox.herdrSession,
								"pane.list",
								{},
							);
							const shell = panes[0] ? await paneShellPid(sandbox.herdrSession, panes[0].pane_id) : null;
							const processes = snapshotProcesses();
							const herdrServer = shell === null ? undefined : processes.find((info) => info.pid === shell)?.ppid;
							const seen = new Map<number, (typeof processes)[number]>();
							for (const root of [frontier?.pid, herdrServer])
								if (root !== undefined) for (const info of treeOf(root, processes)) seen.set(info.pid, info);
							const memory = treeMemory([...seen.values()]);
							peakWorkingSet = Math.max(peakWorkingSet ?? 0, memory.workingSet);
							if (memory.privateBytes !== null) peakPrivate = Math.max(peakPrivate ?? 0, memory.privateBytes);
						} catch (error) {
							// One missed sample lowers the peak's resolution, not the run.
							console.error(`memory sample failed: ${(error as Error).message}`);
						}
						await Bun.sleep(MEMORY_SAMPLE_MS);
					}
				})();
				await Promise.all(
					work.map(async (assignment) => {
						const reply = await callTool(
							"thread_start",
							{ project, title: assignment.title, brief: assignment.brief },
							{ bossKey },
						);
						if (reply.isError) throw new Error(`thread_start ${assignment.title}: ${reply.text}`);
					}),
				);
				const deadline = Date.now() + timeoutMs;
				let lines: ThreadLine[] = [];
				for (;;) {
					const status = await callTool("thread_status", {}, { bossKey });
					lines = status.text
						.split("\n")
						.map(parseThreadLine)
						.filter((line): line is ThreadLine => line !== null && work.some((a) => a.title === line.title));
					const ahead = (branch: string) =>
						Number(git(repo, ["rev-list", "--count", `main..${branch}`]).stdout.trim()) || 0;
					const done = lines.filter((line) => finished(line, ahead(line.branch))).length;
					if (lines.length === work.length && done === work.length) break;
					if (Date.now() > deadline) {
						console.log(`run ${run} with ${n} threads: timed out with ${done}/${work.length} finished`);
						break;
					}
					await Bun.sleep(10_000);
				}
				// Each thread's changed files: its branch's commits since main plus its worktree's uncommitted files.
				const worktrees = new Map<string, string>();
				let current: string | undefined;
				for (const line of git(repo, ["worktree", "list", "--porcelain"]).stdout.split("\n")) {
					if (line.startsWith("worktree ")) current = line.slice("worktree ".length).trim();
					if (line.startsWith("branch refs/heads/") && current)
						worktrees.set(line.slice("branch refs/heads/".length).trim(), current);
				}
				const changed = new Map<string, string[]>();
				for (const line of lines) {
					const committed = git(repo, ["diff", "--name-only", `main...${line.branch}`]).stdout;
					const path = worktrees.get(line.branch);
					const loose = path
						? git(path, ["status", "--porcelain"])
								.stdout.split("\n")
								.map((l) => l.slice(3))
						: [];
					changed.set(line.title, [
						...new Set([...committed.split("\n"), ...loose].map((f) => f.trim()).filter((f) => f !== "")),
					]);
				}
				const conflicts = new Map<number, boolean>();
				for (const pair of new Set(work.map((a) => a.pair))) {
					const [a, b] = work.filter((x) => x.pair === pair).map((x) => lines.find((l) => l.title === x.title)?.branch);
					if (!a || !b) continue;
					conflicts.set(pair, git(repo, ["merge-tree", "--write-tree", a, b]).status === 1);
				}
				const mainChanges = git(repo, ["status", "--porcelain"])
					.stdout.split("\n")
					.filter((l) => l.trim() !== "").length;
				const result: RunScore & { run: number } & Partial<K11Measures> = {
					...score(work, lines, changed, conflicts, mainChanges),
					run,
				};
				sampling = false;
				await sampler;
				if (layer && prices) {
					const ahead = (branch: string) =>
						Number(git(repo, ["rev-list", "--count", `main..${branch}`]).stdout.trim()) || 0;
					const finishedThreads = lines.filter((line) => finished(line, ahead(line.branch))).length;
					const herdr = await connectHerdr({ session: sandbox.herdrSession });
					const panes = await herdr.panes();
					herdr.close();
					const ours = new Map([...worktrees].filter(([branch]) => lines.some((line) => line.branch === branch)));
					const records = [...threadSessionFiles(panes, ours).values()]
						.flat()
						.map((path) => readOmpSession(readFileSync(path, "utf8"), benchPricing(prices)));
					const ledger = trialLedger(records, benchPricing(prices));
					const countedAfter = countedBefore ? readLayerCounters(sandbox.dataDir) : null;
					Object.assign(result, {
						layer,
						finishedThreads,
						costUsd: ledger.totals.cost,
						costPerFinishedThread: finishedThreads === 0 ? null : ledger.totals.cost / finishedThreads,
						unpricedModels: ledger.totals.unpricedModels,
						peakWorkingSetMb: peakWorkingSet === null ? null : peakWorkingSet / 2 ** 20,
						peakPrivateMb: peakPrivate === null ? null : peakPrivate / 2 ** 20,
						checkHits: countedBefore && countedAfter ? countedAfter.checkHits - countedBefore.checkHits : null,
						indexReuse: countedBefore && countedAfter ? countedAfter.indexReuse - countedBefore.indexReuse : null,
					} satisfies K11Measures);
				}
				scores.push(result);
				appendRow(out, "h8-parallel", result);
				console.log(
					`${n} threads, run ${run}: ${result.wrongThreadEdits} wrong-thread edits, ${result.flaggedBeforeMerge} flagged, ${result.conflictsAtMerge} conflicts`,
				);
				if (run < runs - 1 || n !== counts.at(-1)) {
					// thread_finish and worktree_remove ask him first (0484); the bench leaves closing to him.
					console.log("close this run's h8 threads, then press Enter");
					for await (const _line of console) break;
				}
			}
		}
		const file = writeJson(out, "h8-parallel", {
			benchmark: "H8",
			layer: layer ?? null,
			conditions: { ...started, finishedAt: new Date().toISOString() },
			byThreads: counts.map((n) => {
				const of = scores.filter((s) => s.threads === n);
				const measured = of.filter((s) => s.costUsd !== undefined);
				const totalCost = measured.reduce((total, s) => total + (s.costUsd ?? 0), 0);
				const totalFinished = measured.reduce((total, s) => total + (s.finishedThreads ?? 0), 0);
				const sum = (
					key:
						| "wrongThreadEdits"
						| "flaggedBeforeMerge"
						| "conflictsAtMerge"
						| "foundOnlyAtMerge"
						| "mainCheckoutChanges",
				) => of.reduce((total, s) => total + s[key], 0);
				return {
					threads: n,
					runs: of.length,
					wrongThreadEdits: sum("wrongThreadEdits"),
					mainCheckoutChanges: sum("mainCheckoutChanges"),
					pairs: of.reduce((total, s) => total + s.pairs.length, 0),
					flaggedBeforeMerge: sum("flaggedBeforeMerge"),
					conflictsAtMerge: sum("conflictsAtMerge"),
					foundOnlyAtMerge: sum("foundOnlyAtMerge"),
					...(layer
						? {
								costPerFinishedThread: totalFinished === 0 ? null : totalCost / totalFinished,
								peakWorkingSetMb: Math.max(...measured.map((s) => s.peakWorkingSetMb ?? 0)),
								checkHits: layer === "on" ? measured.reduce((total, s) => total + (s.checkHits ?? 0), 0) : null,
								indexReuse: layer === "on" ? measured.reduce((total, s) => total + (s.indexReuse ?? 0), 0) : null,
							}
						: {}),
				};
			}),
		});
		console.log(`wrote ${file}`);
	});
}
