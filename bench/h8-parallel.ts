// H8 (0482, extended by 0543 K11, 0576): parallel isolation and overlap detection, in a sandbox Jocasta (VM: the
// threads' agents call the model).
//
//   bun bench/h8-parallel.ts --out <dir> --project <name> --repo <main checkout> [--threads 2,4,8] [--runs 1]
//                            [--layer on|off] [--timeout 60] [--make-fixture] [--dry-run]
//
// `--make-fixture` writes the fixture into `--repo` (an empty folder that becomes a git repo; register it as a sandbox
// project before the run). Threads come in pairs: an even pair both edit the same file (`src/mod<k>.ts`), an odd pair
// edits one interface from both sides (`src/shared.ts` and `src/use<k>.ts`), in the style of "Passes Alone, Fails
// Together": the fixture's `test/pair<k>.test.ts` calls `show<k>(makeItem<k>())`, which passes with either side alone
// and throws with both. Each thread starts as `jocasta thread new` does (no Allow, 0544); threads open in plan mode,
// so he approves each plan. The script reads `jocasta thread list` until every thread of the run is finished (0529:
// idle or done with a commit on its branch; a fresh thread reads idle before its brief lands and a waiting plan reads
// done), then measures:
//   wrong-thread edits  files a thread changed outside its own assignment, plus any change in the main checkout;
//   flagged before merge  pairs whose `thread list` line names the partner under "overlaps" (0456);
//   conflicts at merge  pairs whose branches conflict in `git merge-tree --write-tree`;
//   semantic failures  pairs that merge cleanly but whose merged tree, checked out in a temp worktree, fails `bun test`;
//   found only at merge  conflicts and semantic failures the overlaps view didn't flag.
// K11 (`--layer on|off`, 0543, 0576): 4 threads at once, 3 runs, with the harness-OS layer on and then off: the sandbox
// service is started with `JOCASTA_LAYER=off` for `--layer off`, and H8 reads the layer's state from `/health` and
// refuses a mismatch. Each run also measures the cost per finished thread (every omp session of each thread since the
// run started, its planning session included, with their subagents, priced by the benchmark's ledger at the pinned
// table), the peak working set and private bytes of Jocasta's processes (the service's, herdr's server's with the
// threads, and the attached surfaces' trees; sampled every 10 s), and, layer on, the run's shared check hits and index
// reuse, as the sandbox service counts them in its data dir.
// Writes h8-parallel.jsonl (one row per thread count and run) and h8-parallel.json.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { checksDir, readCheckHits } from "../src/frontier/server/checks.ts";
import { readIndexReuse } from "../src/frontier/server/code-index.ts";
import type { PaneInfo } from "../src/frontier/server/herdr.ts";
import { SessionIndex } from "../src/frontier/server/sessions.ts";
import { removeTree } from "../src/shared/fsx.ts";
import { threadSessionRecords, trialLedger } from "../src/shared/ledger.ts";
import { samePath } from "../src/shared/pathkey.ts";
import { runProgramSync } from "../src/shared/process.ts";
import { BenchUsageError, flag, numberListFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { openSandbox } from "./lib/jocasta.ts";
import { benchPricing } from "./lib/ledger.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";
import { loadPriceTable } from "./lib/prices.ts";
import { jocastaProcesses, snapshotProcesses, treeMemory } from "./lib/proctree.ts";
import { requireServiceSwitch } from "./lib/sandbox.ts";
import { listThreads, startThread, type ThreadLine, waitForEnter } from "./lib/threads.ts";

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

/**
 * Each thread's omp session files since `since` (epoch ms), as the service's own ledger finds them (0538, 0546): the
 * session herdr reports for the pane in the thread's worktree, followed where omp moved it, and every other session in
 * its folder that began since, oldest first. omp starts a new session on "Approve and execute", so the session herdr
 * reports at the end is only the execution's; the planning one sits beside it.
 */
export function threadSessionFiles(
	panes: readonly Pick<PaneInfo, "cwd" | "sessionPath">[],
	worktrees: ReadonlyMap<string, string>,
	since: number,
	index: SessionIndex = new SessionIndex(),
): Map<string, string[]> {
	const files = new Map<string, string[]>();
	for (const [branch, worktree] of worktrees) {
		const pane = panes.find((p) => p.cwd !== undefined && p.sessionPath !== undefined && samePath(p.cwd, worktree));
		if (!pane?.sessionPath) continue;
		files.set(branch, index.sessionsSince(index.moved(pane.sessionPath), since));
	}
	return files;
}

/**
 * The fixture: one shared interface file with each odd pair's `makeItem<k>()`, a module and a user per pair, a test per
 * odd pair that calls `show<k>(makeItem<k>())` (it passes with either side of the pair alone and throws with both:
 * the semantic break, 0576), and a package file.
 */
export function fixtureFiles(maxThreads: number): Record<string, string> {
	const pairs = Math.ceil(maxThreads / 2);
	const files: Record<string, string> = {
		"package.json": `${JSON.stringify({ name: "acme-parallel", private: true, type: "module" }, null, 2)}\n`,
		"src/shared.ts": Array.from({ length: pairs }, (_, k) =>
			[
				`export interface Item${k} {\n\tid: string;\n\tlabel: string;\n}\n`,
				...(k % 2 === 1
					? [`\nexport function makeItem${k}(): Item${k} {\n\treturn { id: "${k}", label: "item ${k}" };\n}\n`]
					: []),
			].join(""),
		).join("\n"),
	};
	for (let k = 0; k < pairs; k++) {
		files[`src/mod${k}.ts`] = `export function base${k}(value: number): number {\n\treturn value;\n}\n`;
		files[`src/use${k}.ts`] =
			`import type { Item${k} } from "./shared.ts";\n\nexport function show${k}(item: Item${k}): string {\n\treturn item.label;\n}\n`;
		if (k % 2 === 1)
			files[`test/pair${k}.test.ts`] = [
				'import { expect, test } from "bun:test";',
				`import { makeItem${k} } from "../src/shared.ts";`,
				`import { show${k} } from "../src/use${k}.ts";`,
				"",
				`test("show${k} shows a made item", () => {`,
				`\texpect(() => show${k}(makeItem${k}())).not.toThrow();`,
				"});",
				"",
			].join("\n");
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
	/** The pair merged cleanly and the merged tree failed `bun test` (0576). */
	semanticFailure: boolean;
}

export interface RunScore {
	threads: number;
	wrongThreadEdits: number;
	mainCheckoutChanges: number;
	pairs: PairResult[];
	flaggedBeforeMerge: number;
	conflictsAtMerge: number;
	semanticFailures: number;
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
	semanticFailures: ReadonlyMap<number, boolean> = new Map(),
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
		return {
			pair,
			flagged,
			conflict: conflicts.get(pair) ?? false,
			semanticFailure: semanticFailures.get(pair) ?? false,
		};
	});
	return {
		threads: work.length,
		wrongThreadEdits: wrong,
		mainCheckoutChanges: mainChanges,
		pairs,
		flaggedBeforeMerge: pairs.filter((p) => p.flagged).length,
		conflictsAtMerge: pairs.filter((p) => p.conflict).length,
		semanticFailures: pairs.filter((p) => p.semanticFailure).length,
		foundOnlyAtMerge: pairs.filter((p) => (p.conflict || p.semanticFailure) && !p.flagged).length,
	};
}

type Git = (cwd: string, args: string[]) => { status: number | null; stdout: string };

/**
 * Whether two branches that merge cleanly break the fixture's tests together (0576): the merged tree as a commit,
 * checked out in a temp worktree, and `bun test` run there; false when they conflict or the tree has no tests.
 */
export function mergedTestsFail(git: Git, repo: string, a: string, b: string, bun = process.execPath): boolean {
	const merged = git(repo, ["merge-tree", "--write-tree", a, b]);
	if (merged.status !== 0) return false;
	const tree = merged.stdout.split("\n")[0]?.trim() ?? "";
	const commit = git(repo, [
		"-c",
		"user.name=Bench",
		"-c",
		"user.email=bench@example.org",
		"commit-tree",
		tree,
		"-p",
		a,
		"-p",
		b,
		"-m",
		"h8 merged pair",
	]).stdout.trim();
	const dir = join(mkdtempSync(join(tmpdir(), "h8-merge-")), "tree");
	git(repo, ["worktree", "add", "--detach", dir, commit]);
	try {
		if (!existsSync(join(dir, "test"))) return false;
		const run = spawnSync(bun, ["test"], { cwd: dir, encoding: "utf8", windowsHide: true, timeout: 120_000 });
		return run.status !== 0;
	} finally {
		git(repo, ["worktree", "remove", "--force", dir]);
		removeTree(join(dir, ".."));
	}
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
					`${counts.join(", ")} threads × ${runs} run(s) in ${project ?? "<--project>"} via jocasta thread new (no Allow; model calls; he approves each plan: VM only)`,
					...assignments(Math.min(4, Math.max(...counts)), 0).map((a) => `thread ${a.thread}: ${a.brief}`),
					`read jocasta thread list up to ${timeoutMs / 60_000} min until all are idle or done with a commit on their branch`,
					"measure wrong-thread edits, overlaps flagged before merge, merge-tree conflicts, semantic failures (a clean merge whose tree fails bun test), conflicts found only at merge",
					...(layer
						? [
								`K11 (0543): the layer ${layer} in the sandbox (the service started ${layer === "off" ? "with" : "without"} JOCASTA_LAYER=off; /health checked); cost per finished thread from every omp session of each thread since the run started at the pinned price table; peak memory of the service's, herdr's and the surfaces' trees every ${MEMORY_SAMPLE_MS / 1000} s`,
								layer === "on"
									? "shared check hits and index reuse from the service's logs in its data dir"
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
		const git: Git = (cwd, gitArgs) => {
			const { code, stdout } = runProgramSync(["git", ...gitArgs], { cwd });
			return { status: code, stdout };
		};
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
		const jocasta = openSandbox();
		const { sandbox } = jocasta;
		claimRows(out, "h8-parallel");
		if (layer) await requireServiceSwitch("layer", layer);
		const prices = layer ? loadPriceTable() : undefined;
		const started = conditions(["omp", "herdr", "git", "jocasta"]);
		const ahead = (branch: string) => Number(git(repo, ["rev-list", "--count", `main..${branch}`]).stdout.trim()) || 0;
		const scores: (RunScore & { run: number } & Partial<K11Measures>)[] = [];
		for (const n of counts) {
			for (let run = 0; run < runs; run++) {
				const work = assignments(n, run);
				const runStartedAt = Date.now();
				const countedBefore = layer === "on" ? readLayerCounters(sandbox.dataDir) : null;
				let peakWorkingSet: number | null = null;
				let peakPrivate: number | null = null;
				let sampling = layer !== undefined;
				const sampler = (async () => {
					while (sampling) {
						try {
							const processes = snapshotProcesses();
							const { all } = jocastaProcesses(await jocasta.processes(processes), processes);
							const memory = treeMemory(all);
							peakWorkingSet = Math.max(peakWorkingSet ?? 0, memory.workingSet);
							if (memory.privateBytes !== null) peakPrivate = Math.max(peakPrivate ?? 0, memory.privateBytes);
						} catch (error) {
							// One missed sample lowers the peak's resolution, not the run.
							console.error(`memory sample failed: ${(error as Error).message}`);
						}
						await Bun.sleep(MEMORY_SAMPLE_MS);
					}
				})();
				await Promise.all(work.map((assignment) => startThread(project, assignment.title, assignment.brief)));
				const deadline = Date.now() + timeoutMs;
				let lines: ThreadLine[] = [];
				for (;;) {
					lines = (await listThreads(project)).filter((line) => work.some((a) => a.title === line.title));
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
				const semantic = new Map<number, boolean>();
				for (const pair of new Set(work.map((a) => a.pair))) {
					const [a, b] = work.filter((x) => x.pair === pair).map((x) => lines.find((l) => l.title === x.title)?.branch);
					if (!a || !b) continue;
					const conflict = git(repo, ["merge-tree", "--write-tree", a, b]).status === 1;
					conflicts.set(pair, conflict);
					semantic.set(pair, !conflict && mergedTestsFail(git, repo, a, b));
				}
				const mainChanges = git(repo, ["status", "--porcelain"])
					.stdout.split("\n")
					.filter((l) => l.trim() !== "").length;
				const result: RunScore & { run: number } & Partial<K11Measures> = {
					...score(work, lines, changed, conflicts, mainChanges, semantic),
					run,
				};
				sampling = false;
				await sampler;
				if (layer && prices) {
					const finishedThreads = lines.filter((line) => finished(line, ahead(line.branch))).length;
					const panes = await (await jocasta.herdr()).panes();
					const ours = new Map([...worktrees].filter(([branch]) => lines.some((line) => line.branch === branch)));
					const pricing = benchPricing(prices);
					const files = [...threadSessionFiles(panes, ours, runStartedAt).values()];
					const ledger = trialLedger(
						files.flatMap((sessionFiles) => threadSessionRecords(sessionFiles, pricing)),
						pricing,
					);
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
					`${n} threads, run ${run}: ${result.wrongThreadEdits} wrong-thread edits, ${result.flaggedBeforeMerge} flagged, ${result.conflictsAtMerge} conflicts, ${result.semanticFailures} semantic failures`,
				);
				if (run < runs - 1 || n !== counts.at(-1)) {
					// Stopping a thread and removing its worktree ask him first (0484); the bench leaves closing to him.
					await waitForEnter("close this run's h8 threads, then press Enter");
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
						| "semanticFailures"
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
					semanticFailures: sum("semanticFailures"),
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
