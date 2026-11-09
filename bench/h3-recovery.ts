// H3 (0482): crash and restart recovery, in a sandbox Jocasta with thread panes open.
//
//   bun bench/h3-recovery.ts --out <dir> [--runs 20] [--faults herdr,service,pane] [--timeout 120]
//                            [--restart "jocasta"] [--dry-run]
//
// Each run snapshots the sandbox herdr session's panes and every pane folder's git state, finds its target, injects one
// fault and polls until the work is back, then compares. Recovery time runs from the moment the kill returned (the
// lookups come before it):
//   herdr    kills the session's herdr server (the parent of a pane's process, `pane.process_info`); recovered when
//            every workspace that had an omp pane before has an omp pane again (0522: the service relaunches the
//            panes under new ids). Each row also gives the ms until herdr first listed exactly the old pane ids
//            (`oldPaneIdsMs`, null when no poll saw them).
//   service  kills the service that `<data>/jocasta-service.json` names and runs `--restart` (default `jocasta`); recovered
//            when a new service answers and every workspace that had an omp pane before has one again (0528; the new
//            service relaunches the boss pane under a new id). Each restart's client ends after its run, root first
//            and never the new service's tree, so one client is attached at a time. Rows of runs before 0576 named
//            this fault `companion` (h3-recovery.json's `renamed` says so).
//   pane     kills the process tree herdr started for one omp pane (omp itself), the one in the folder it killed
//            longest ago (one at work first among equals), waiting when it must so one kill per folder per 10 minutes
//            (0528): each kill then meets 0521's first ending, not its restart wait. Recovered when omp runs again in
//            that workspace under another pane id (0521's restart gives the pane a new id) and every workspace that
//            had an omp pane before has one again (0522).
// Jocasta closing itself (0530): a run that didn't recover, after which no service answers (one still closing is
//            waited for) and herdr lists no panes, counts not recovered with `stoppedItself`. `--restart` then starts
//            Jocasta again, its client staying attached in place of the terminal's, and the next run starts once a
//            service answers and every workspace that had an omp pane before has one again (`restartMs`); not within 5
//            minutes, H3 ends there.
// Lost work: each pane folder's `git status --porcelain` and `git diff` hash before and after, for the folders in a git
// work tree (the boss's home has no git state to lose); a git that fails there fails the run. Each pane's session file
// still parses (every non-empty line JSON, a `session` entry anywhere, 0523). Logging off and on (fault 4) can't be
// scripted; it is run by hand with the same checks (README).
// Writes h3-recovery.jsonl (one row per run) and h3-recovery.json (rates with CIs and recovery-time summaries).
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { isTodayPane } from "../src/frontier/server/boss.ts";
import type { PaneInfo } from "../src/frontier/server/herdr.ts";
import { git as gitRead } from "../src/shared/git.ts";
import { jsonLines } from "../src/shared/jsonl.ts";
import { pathKey } from "../src/shared/pathkey.ts";
import { killTree, pidAlive } from "../src/shared/proctree.ts";
import { readServiceFile } from "../src/shared/service-file.ts";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { git } from "./lib/git.ts";
import { openSandbox, type SandboxJocasta } from "./lib/jocasta.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";
import { type ProcessInfo, snapshotProcesses, treeOf } from "./lib/proctree.ts";
import { rate, summarize } from "./lib/stats.ts";
import { waitFor } from "./lib/timing.ts";

export const FAULTS = ["herdr", "service", "pane"] as const;
export type Fault = (typeof FAULTS)[number];

/** The faults renamed since earlier runs' rows (0576, 0584): new name → old name. */
export const RENAMED_FAULTS: Partial<Record<Fault, string>> = { service: "companion" };

/** A pane as H3 compares it: the fields of the product client's `PaneInfo` it reads. */
export type PaneState = Pick<PaneInfo, "paneId" | "workspaceId" | "cwd" | "agent" | "agentState" | "sessionPath">;

export interface FolderState {
	status: string;
	diffHash: string;
}

export interface H3Row {
	fault: Fault;
	run: number;
	recovered: boolean;
	recoveryMs: number | null;
	/** Herdr fault only (0522): ms until herdr first listed exactly the old pane ids; null when never seen. */
	oldPaneIdsMs: number | null;
	samePanes: boolean;
	foldersIntact: boolean;
	sessionsParse: boolean;
	/** Jocasta had closed itself (0530): the run didn't recover, no service answered and herdr listed no panes. */
	stoppedItself: boolean;
	/** ms from `--restart` until a service answered and omp was back everywhere (0530); null when not started again. */
	restartMs: number | null;
	target: string | null;
}

/**
 * herdr's panes as H3 compares them, the plugin's `today` pane left out (0553): it is no thread, and the service opens
 * it again under a new id.
 */
export function threadPanes<T extends Pick<PaneInfo, "label" | "agent">>(list: readonly T[]): T[] {
	return list.filter((pane) => !isTodayPane(pane));
}

/** How long one of H3's pane listings may take before the poll counts herdr as not answering. */
const LIST_MS = 3_000;

async function panes(jocasta: SandboxJocasta): Promise<PaneState[] | null> {
	try {
		return threadPanes(await (await jocasta.herdr()).panes());
	} catch {
		return null;
	}
}

/** A folder's git state; a git that fails throws, so a broken check never compares two empty outputs. */
function folderState(cwd: string): FolderState {
	return {
		status: git(cwd, ["status", "--porcelain"]),
		diffHash: new Bun.CryptoHasher("sha256").update(git(cwd, ["diff", "HEAD"])).digest("hex"),
	};
}

/** Same pane ids, as sets. */
export function samePaneIds(before: readonly PaneState[], after: readonly PaneState[]): boolean {
	const a = new Set(before.map((pane) => pane.paneId));
	const b = new Set(after.map((pane) => pane.paneId));
	return a.size === b.size && [...a].every((id) => b.has(id));
}

/** Every workspace that had an omp pane before has an omp pane now, whatever its id (0522). */
export function ompBack(before: readonly PaneState[], after: readonly PaneState[]): boolean {
	const back = new Set(after.filter((pane) => pane.agent === "omp").map((pane) => pane.workspaceId));
	return before.filter((pane) => pane.agent === "omp").every((pane) => back.has(pane.workspaceId));
}

/**
 * The pane fault's own pane is back (0521, 0522): omp runs again in the killed pane's workspace under another pane id
 * (0521's restart gives it a new one), so the dying omp, still listed under the old id, never counts.
 */
export function paneBack(killed: PaneState, now: readonly PaneState[]): boolean {
	return now.some(
		(pane) => pane.workspaceId === killed.workspaceId && pane.agent === "omp" && pane.paneId !== killed.paneId,
	);
}

/** How long a Jocasta that closed itself has to come back before H3 ends (0530). */
const RESTART_BOUND_MS = 5 * 60_000;

/** How long a service that is closing Jocasta gets to finish before H3 judges whether Jocasta closed itself (0530). */
const CLOSING_BOUND_MS = 60_000;

/**
 * Whether the service `jocasta-service.json` names answers as itself (`serviceState`: `up`, or `busy` while its process
 * lives); one that is closing Jocasta answers `/health` until its last step, so it is waited for (`CLOSING_BOUND_MS`)
 * and then doesn't count.
 */
async function serviceAnswers(jocasta: SandboxJocasta): Promise<boolean> {
	let state = await jocasta.health();
	if (state === "closing") {
		await waitFor(
			async () => {
				state = await jocasta.health();
				return state !== "closing";
			},
			{ timeoutMs: CLOSING_BOUND_MS, intervalMs: 500 },
		);
	}
	return state === "up" || state === "busy";
}

/**
 * Jocasta closed itself during a run (0530): the run didn't recover, no service answers, and herdr lists no panes
 * (or doesn't answer). A service killed by the service fault leaves herdr's panes, so that run isn't one.
 */
export function closedItself(recovered: boolean, serviceUp: boolean, listed: readonly PaneState[] | null): boolean {
	return !recovered && !serviceUp && (listed === null || listed.length === 0);
}

/**
 * Starts a Jocasta that closed itself again (0530) with `restart`, whose client stays attached in place of the
 * terminal's; the ms until a service answers and every workspace that had an omp pane before has one again, or
 * null when that takes longer than `RESTART_BOUND_MS`.
 */
async function startAgain(
	restart: string,
	before: readonly PaneState[],
	jocasta: SandboxJocasta,
): Promise<number | null> {
	const t0 = performance.now();
	const again = spawn(restart, { shell: true, detached: true, stdio: "ignore", windowsHide: true });
	again.unref();
	const back = await waitFor(
		async () => {
			if ((await jocasta.health()) !== "up") return false;
			const now = await panes(jocasta);
			return now !== null && ompBack(before, now);
		},
		{ timeoutMs: RESTART_BOUND_MS, intervalMs: 1_000 },
	);
	return back === null ? null : Math.round(performance.now() - t0);
}

/**
 * Between two kills in one folder (0528): 0521 counts endings in a row per folder and starts counting again 10
 * minutes after the last ending that started omp again; 30 s more for the service's refresh.
 */
export const FOLDER_SPACING_MS = 10 * 60_000 + 30_000;

/** A folder as 0521 keys it (`pathKey`): case and slashes don't matter on Windows. */
export function folderKey(pane: PaneState): string {
	return pathKey(pane.cwd ?? pane.paneId);
}

/**
 * The pane fault's pane (0528): the omp pane whose folder's last run ended longest ago (never first), at work first
 * among equals; `waitMs` until that folder is `FOLDER_SPACING_MS` past its last run.
 */
export function pickPane(
	list: readonly PaneState[],
	lastEnded: ReadonlyMap<string, number>,
	now: number,
): { pane: PaneState; waitMs: number } | undefined {
	const ready = (pane: PaneState) => {
		const last = lastEnded.get(folderKey(pane));
		return last === undefined ? Number.NEGATIVE_INFINITY : last + FOLDER_SPACING_MS;
	};
	let best: PaneState | undefined;
	for (const pane of list) {
		if (pane.agent !== "omp") continue;
		if (
			!best ||
			ready(pane) < ready(best) ||
			(ready(pane) === ready(best) && pane.agentState === "working" && best.agentState !== "working")
		) {
			best = pane;
		}
	}
	return best && { pane: best, waitMs: Math.max(0, ready(best) - now) };
}

/**
 * Ends a restart's process tree root first, so `jocasta` is gone before it can see its herdr client exit and ask to
 * close (0434); never a process in `keep` (the new service's tree). Returns the pids it ended.
 */
export function endRestart(
	rootPid: number,
	keep: ReadonlySet<number>,
	processes: readonly ProcessInfo[],
	kill: (pid: number) => void = (pid) => process.kill(pid, "SIGKILL"),
): number[] {
	const ended: number[] = [];
	for (const info of treeOf(rootPid, processes)) {
		if (keep.has(info.pid)) continue;
		try {
			kill(info.pid);
			ended.push(info.pid);
		} catch {
			// Gone already.
		}
	}
	return ended;
}

/**
 * The herdr fault's poll (0522): recovered when omp is back; on the way it notes the ms from `since` until a listing
 * first holds exactly the old pane ids.
 */
export function herdrRecovery(
	before: readonly PaneState[],
	since: number,
	clock: () => number = () => performance.now(),
): { check(now: readonly PaneState[] | null): boolean; oldPaneIdsMs(): number | null } {
	let oldIdsMs: number | null = null;
	return {
		check(now) {
			if (now === null) return false;
			if (oldIdsMs === null && samePaneIds(before, now)) oldIdsMs = Math.round(clock() - since);
			return ompBack(before, now);
		},
		oldPaneIdsMs: () => oldIdsMs,
	};
}

/** Every folder's git state unchanged. */
export function foldersIntact(
	before: ReadonlyMap<string, FolderState>,
	after: ReadonlyMap<string, FolderState>,
): boolean {
	return [...before].every(([cwd, state]) => {
		const now = after.get(cwd);
		return now !== undefined && now.status === state.status && now.diffHash === state.diffHash;
	});
}

/** A session file that still parses (0523): every non-empty line is JSON and a `session` entry is in it, anywhere. */
export function sessionParses(text: string): boolean {
	const lines = text.split(/\r?\n/).filter((line) => line.trim() !== "");
	const entries = jsonLines<{ type?: unknown }>(text);
	return entries.length === lines.length && entries.some((entry) => entry.type === "session");
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 20,
			valued: ["--faults", "--timeout", "--restart"],
		});
		const faults = listFlag(args, "faults", FAULTS) as Fault[];
		for (const fault of faults)
			if (!FAULTS.includes(fault)) throw new BenchUsageError(`unknown fault ${fault} (herdr, service, pane)`);
		const timeoutMs = Number(flag(args, "timeout", "120")) * 1000;
		if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new BenchUsageError("--timeout needs seconds above 0");
		const restart = flag(args, "restart", "jocasta");
		if (args.dryRun) {
			printPlan(
				"H3 crash recovery",
				[
					`${args.runs} runs of each fault: ${faults.join(", ")} (log off/on is by hand)`,
					"before each: herdr's panes, each pane folder's git status and diff hash, each session file",
					"herdr: kill the session's herdr server; recovered when every workspace with an omp pane has one again; timed from the kill",
					"herdr: also the time until herdr first lists exactly the old pane ids (a second figure)",
					`service: kill the service in jocasta-service.json, run \`${restart}\`; recovered when a new one answers and every workspace with an omp pane has one again; then the restart's client ends`,
					"pane: kill the process tree of the omp pane in the folder killed longest ago (one at work first), once per folder in 10.5 min; recovered when omp runs again in its workspace under another pane id and every workspace with an omp pane has one again",
					`Jocasta closing itself (no service, after one closing is waited for, and no panes after a run that didn't recover): not recovered, \`${restart}\` starts it again and its client stays; the next run once omp is back everywhere, at most 5 min, else H3 ends`,
					"lost work: git status and diff hash unchanged; each session file all JSON with a session entry",
					`poll every 250 ms for up to ${timeoutMs / 1000} s; report recovery rate, time to recovery and lost work`,
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const jocasta = openSandbox({ herdrTimeoutMs: LIST_MS });
		const { herdrSession } = jocasta.sandbox;
		claimRows(out, "h3-recovery");
		const started = conditions(["herdr", "omp", "jocasta"]);
		const rows: H3Row[] = [];
		// The pane fault's folders: when each one's last run ended (0528).
		const lastEnded = new Map<string, number>();
		for (const fault of faults) {
			for (let run = 0; run < args.runs; run++) {
				if (fault === "pane") {
					const listed = await panes(jocasta);
					const next = listed ? pickPane(listed, lastEnded, Date.now()) : undefined;
					if (next && next.waitMs > 0) {
						console.log(
							`pane run ${run}: waiting ${Math.ceil(next.waitMs / 1000)} s, one kill per folder per 10 minutes (0528)`,
						);
						await Bun.sleep(next.waitMs);
					}
				}
				const before = await panes(jocasta);
				if (!before || before.length === 0) throw new Error(`herdr session ${herdrSession} lists no panes`);
				const folders = new Map(
					[
						...new Set(
							before
								.map((pane) => pane.cwd)
								.filter(
									(cwd): cwd is string =>
										cwd !== undefined &&
										existsSync(cwd) &&
										gitRead(cwd, ["rev-parse", "--is-inside-work-tree"]) === "true",
								),
						),
					].map((cwd) => [cwd, folderState(cwd)]),
				);
				let target: string | null = null;
				let killedFolder: string | null = null;
				let restartPid: number | undefined;
				let recoveredAfter: () => Promise<boolean>;
				let oldPaneIdsMs: () => number | null = () => null;
				// Each fault's lookups come first; `t0` is when its kill returned (0522's time to recovery).
				let t0: number;
				if (fault === "herdr") {
					const { herdrServer: server } = await jocasta.processes(snapshotProcesses());
					if (!server) throw new Error("couldn't find herdr's server process (parent of a pane's process)");
					target = String(server);
					await killTree(server);
					t0 = performance.now();
					const watch = herdrRecovery(before, t0);
					oldPaneIdsMs = watch.oldPaneIdsMs;
					recoveredAfter = async () => watch.check(await panes(jocasta));
				} else if (fault === "service") {
					const running = readServiceFile();
					if (!running) throw new Error("no service: <data>/jocasta-service.json is missing");
					target = String(running.pid);
					// The service alone: its panes live in herdr's process tree, not under it.
					process.kill(running.pid, "SIGKILL");
					t0 = performance.now();
					await waitFor(() => !pidAlive(running.pid), { timeoutMs: 10_000 });
					const restarted = spawn(restart, { shell: true, detached: true, stdio: "ignore", windowsHide: true });
					restarted.unref();
					restartPid = restarted.pid;
					recoveredAfter = async () => {
						const next = readServiceFile();
						if (!next || next.pid === running.pid) return false;
						const now = await panes(jocasta);
						return (await jocasta.health(next)) === "up" && now !== null && ompBack(before, now);
					};
				} else {
					const picked = pickPane(before, lastEnded, Date.now());
					if (!picked) throw new Error("no omp pane to kill");
					const pane = picked.pane;
					target = pane.paneId;
					killedFolder = folderKey(pane);
					const herdr = await jocasta.herdr();
					const root = await herdr.paneRoot(pane.paneId, 10_000).catch(() => undefined);
					if (!root) throw new Error(`herdr gave no process for pane ${pane.paneId}`);
					// herdr started omp itself in the pane (no shell to keep): its whole tree goes, whatever omp's binary is.
					await killTree(root);
					t0 = performance.now();
					recoveredAfter = async () => {
						const now = await panes(jocasta);
						return now !== null && paneBack(pane, now) && ompBack(before, now);
					};
				}
				const waited = await waitFor(recoveredAfter, { timeoutMs, intervalMs: 250 });
				const recoveryMs = waited === null ? null : Math.round(performance.now() - t0);
				const listed = await panes(jocasta);
				const after = listed ?? [];
				const checks = {
					samePanes: samePaneIds(before, after),
					foldersIntact: foldersIntact(folders, new Map([...folders.keys()].map((cwd) => [cwd, folderState(cwd)]))),
					sessionsParse: before
						.map((pane) => pane.sessionPath)
						.filter((path): path is string => path !== undefined && existsSync(path))
						.every((path) => sessionParses(readFileSync(path, "utf8"))),
				};
				// 0530: Jocasta closed itself (0518's heal can take herdr started again for him quitting): started again.
				const stoppedItself = closedItself(waited !== null, await serviceAnswers(jocasta), listed);
				if (stoppedItself) console.log(`${fault} run ${run}: Jocasta closed itself; starting it again (0530)`);
				const restartMs = stoppedItself ? await startAgain(restart, before, jocasta) : null;
				const row: H3Row = {
					fault,
					run,
					recovered: waited !== null,
					recoveryMs,
					oldPaneIdsMs: oldPaneIdsMs(),
					...checks,
					stoppedItself,
					restartMs,
					target,
				};
				rows.push(row);
				appendRow(out, "h3-recovery", row);
				console.log(
					`${fault} run ${run}: ${row.recovered ? `back in ${recoveryMs} ms` : "not recovered"}${
						stoppedItself
							? `; Jocasta ${restartMs === null ? "not back" : `back in ${restartMs} ms`} after its restart`
							: ""
					}`,
				);
				if (stoppedItself && restartMs === null) {
					throw new Error(
						`Jocasta closed itself in ${fault} run ${run} and omp wasn't back everywhere ${RESTART_BOUND_MS / 60_000} min after its restart`,
					);
				}
				if (killedFolder) lastEnded.set(killedFolder, Date.now());
				if (restartPid !== undefined) {
					// One client at a time (0528): the restart's tree goes, root first; the new service's stays.
					const processes = snapshotProcesses();
					const service = readServiceFile();
					const keep = new Set(service ? treeOf(service.pid, processes).map((info) => info.pid) : []);
					const ended = endRestart(restartPid, keep, processes);
					console.log(`service run ${run}: ended the restart's client (${ended.length} processes)`);
				}
				// Settle before the next fault.
				await Bun.sleep(5_000);
			}
		}
		const byFault = Object.fromEntries(
			faults.map((fault) => {
				const of = rows.filter((row) => row.fault === fault);
				return [
					fault,
					{
						recovered: rate(of.map((row) => row.recovered)),
						samePanes: rate(of.map((row) => row.samePanes)),
						foldersIntact: rate(of.map((row) => row.foldersIntact)),
						sessionsParse: rate(of.map((row) => row.sessionsParse)),
						recoveryMs: summarize(of.map((row) => row.recoveryMs).filter((ms): ms is number => ms !== null)),
						stoppedItself: of.filter((row) => row.stoppedItself).length,
						restartMs: summarize(of.map((row) => row.restartMs).filter((ms): ms is number => ms !== null)),
						...(fault === "herdr"
							? {
									oldPaneIdsMs: summarize(of.map((row) => row.oldPaneIdsMs).filter((ms): ms is number => ms !== null)),
								}
							: {}),
					},
				];
			}),
		);
		const file = writeJson(out, "h3-recovery", {
			benchmark: "H3",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			renamed: RENAMED_FAULTS,
			byFault,
		});
		console.log(`wrote ${file}`);
	});
}
