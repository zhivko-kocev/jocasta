// H3 (0482): crash and restart recovery, in a sandbox Jocasta with thread panes open.
//
//   bun bench/h3-recovery.ts --out <dir> [--runs 20] [--faults herdr,companion,pane] [--timeout 120]
//                            [--restart "jocasta"] [--dry-run]
//
// Each run snapshots the sandbox herdr session's panes and every pane folder's git state, injects one fault and polls
// until the work is back, then compares:
//   herdr      kills the session's herdr server (the parent of a pane's shell, `pane.process_info`); recovered when
//              every workspace that had an omp pane before has an omp pane again (0522: the service relaunches the
//              panes under new ids). Each row also gives the ms until herdr first listed exactly the old pane ids
//              (`oldPaneIdsMs`, null when no poll saw them).
//   companion  kills the companion that `<data>/frontier.json` names and runs `--restart` (default `jocasta`);
//              recovered when a new companion answers and every workspace that had an omp pane before has one
//              again (0528; the new companion relaunches the boss pane under a new id). Each restart's client ends
//              after its run, root first and never the new companion's tree, so one client is attached at a time.
//   pane       kills the process tree of one omp pane, the one in the folder it killed longest ago (one at work first
//              among equals), waiting when it must so one kill per folder per 10 minutes (0528): each kill then meets
//              0521's first ending, not its restart wait. Recovered when every workspace that had an omp pane before
//              has one again (0522; 0521's restart gives the pane a new id).
// Jocasta closing itself (0530): a run that didn't recover, after which no companion answers and herdr lists no panes,
//              counts not recovered with `stoppedItself`. `--restart` then starts Jocasta again, its client staying
//              attached in place of the terminal's, and the next run starts once a companion answers and every
//              workspace that had an omp pane before has one again (`restartMs`); not within 5 minutes, H3 ends there.
// Lost work: each pane folder's `git status --porcelain` and `git diff` hash before and after; each pane's session file
// still parses (every non-empty line JSON, a `session` entry anywhere, 0523). Logging off and on (fault 4) can't be
// scripted; it is run by hand with the same checks (README).
// Writes h3-recovery.jsonl (one row per run) and h3-recovery.json (rates with CIs and recovery-time summaries).
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { TOKEN_COOKIE } from "../src/frontier/protocol.ts";
import { isTodayPane } from "../src/frontier/server/boss.ts";
import { type FrontierFile, readFrontierFile } from "../src/frontier/server/frontier-file.ts";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { herdrRequest, paneShellPid } from "./lib/herdr.ts";
import { appendRow, conditions, printPlan, writeJson } from "./lib/output.ts";
import { isAlive, killTree, type ProcessInfo, snapshotProcesses, treeOf } from "./lib/proctree.ts";
import { requireSandbox } from "./lib/sandbox.ts";
import { rate, summarize } from "./lib/stats.ts";
import { jsonLines, waitFor } from "./lib/timing.ts";

export const FAULTS = ["herdr", "companion", "pane"] as const;
export type Fault = (typeof FAULTS)[number];

export interface WirePane {
	pane_id: string;
	workspace_id?: string | null;
	cwd?: string | null;
	label?: string | null;
	agent?: string | null;
	agent_status?: string;
	agent_session?: { kind?: string; value?: string } | null;
}

export interface PaneState {
	paneId: string;
	workspaceId: string | null;
	cwd: string | null;
	agent: string | null;
	status: string | null;
	sessionPath: string | null;
}

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
	/** Jocasta had closed itself (0530): the run didn't recover, no companion answered and herdr listed no panes. */
	stoppedItself: boolean;
	/** ms from `--restart` until a companion answered and omp was back everywhere (0530); null when not started again. */
	restartMs: number | null;
	target: string | null;
}

/**
 * herdr's panes as H3 compares them, the plugin's `today` pane left out (0553): it is no thread, and the service opens
 * it again under a new id.
 */
export function paneStates(list: readonly WirePane[]): PaneState[] {
	return list
		.filter((pane) => !isTodayPane({ label: pane.label ?? undefined, agent: pane.agent ?? undefined }))
		.map((pane) => ({
			paneId: pane.pane_id,
			workspaceId: pane.workspace_id ?? null,
			cwd: pane.cwd ?? null,
			agent: pane.agent ?? null,
			status: pane.agent_status ?? null,
			sessionPath: pane.agent_session?.kind === "path" ? (pane.agent_session.value ?? null) : null,
		}));
}

async function panes(session: string): Promise<PaneState[] | null> {
	try {
		return paneStates((await herdrRequest<{ panes: WirePane[] }>(session, "pane.list", {}, 3_000)).panes);
	} catch {
		return null;
	}
}

function folderState(cwd: string): FolderState {
	const run = (args: string[]) => spawnSync("git", args, { cwd, encoding: "utf8" }).stdout ?? "";
	return {
		status: run(["status", "--porcelain"]),
		diffHash: new Bun.CryptoHasher("sha256").update(run(["diff", "HEAD"])).digest("hex"),
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

/** How long a Jocasta that closed itself has to come back before H3 ends (0530). */
const RESTART_BOUND_MS = 5 * 60_000;

/** The companion `frontier` names answers `/health`; a missing file or a companion that hangs doesn't (5 s). */
async function companionAnswers(frontier: FrontierFile | undefined = readFrontierFile()): Promise<boolean> {
	if (!frontier) return false;
	const health = await fetch(`http://127.0.0.1:${frontier.port}/health`, {
		headers: { cookie: `${TOKEN_COOKIE}=${frontier.token}` },
		signal: AbortSignal.timeout(5_000),
	}).catch(() => undefined);
	return health?.ok === true;
}

/**
 * Jocasta closed itself during a run (0530): the run didn't recover, no companion answers, and herdr lists no panes
 * (or doesn't answer). A companion killed by the companion fault leaves herdr's panes, so that run isn't one.
 */
export function closedItself(recovered: boolean, companionUp: boolean, listed: readonly PaneState[] | null): boolean {
	return !recovered && !companionUp && (listed === null || listed.length === 0);
}

/**
 * Starts a Jocasta that closed itself again (0530) with `restart`, whose client stays attached in place of the
 * terminal's; the ms until a companion answers and every workspace that had an omp pane before has one again, or
 * null when that takes longer than `RESTART_BOUND_MS`.
 */
async function startAgain(restart: string, before: readonly PaneState[], session: string): Promise<number | null> {
	const t0 = performance.now();
	const again = spawn(restart, { shell: true, detached: true, stdio: "ignore", windowsHide: true });
	again.unref();
	const back = await waitFor(
		async () => {
			if (!(await companionAnswers())) return false;
			const now = await panes(session);
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

/** A folder as 0521 keys it: case and slashes don't matter on Windows. */
export function folderKey(pane: PaneState, platform: NodeJS.Platform = process.platform): string {
	const path = pane.cwd ?? pane.paneId;
	return platform === "win32" ? path.replaceAll("\\", "/").toLowerCase() : path;
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
			(ready(pane) === ready(best) && pane.status === "working" && best.status !== "working")
		) {
			best = pane;
		}
	}
	return best && { pane: best, waitMs: Math.max(0, ready(best) - now) };
}

/**
 * Ends a restart's process tree root first, so `jocasta` is gone before it can see its herdr client exit and ask to
 * close (0434); never a process in `keep` (the new companion's tree). Returns the pids it ended.
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
			if (!FAULTS.includes(fault)) throw new BenchUsageError(`unknown fault ${fault} (herdr, companion, pane)`);
		const timeoutMs = Number(flag(args, "timeout", "120")) * 1000;
		if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new BenchUsageError("--timeout needs seconds above 0");
		const restart = flag(args, "restart", "jocasta");
		if (args.dryRun) {
			printPlan(
				"H3 crash recovery",
				[
					`${args.runs} runs of each fault: ${faults.join(", ")} (log off/on is by hand)`,
					"before each: herdr's panes, each pane folder's git status and diff hash, each session file",
					"herdr: kill the session's herdr server; recovered when every workspace with an omp pane has one again",
					"herdr: also the time until herdr first lists exactly the old pane ids (a second figure)",
					`companion: kill the companion in frontier.json, run \`${restart}\`; recovered when a new one answers and every workspace with an omp pane has one again; then the restart's client ends`,
					"pane: kill the omp pane in the folder killed longest ago (one at work first), once per folder in 10.5 min; recovered when every workspace with an omp pane has one again",
					`Jocasta closing itself (no companion, no panes after a run that didn't recover): not recovered, \`${restart}\` starts it again and its client stays; the next run once omp is back everywhere, at most 5 min, else H3 ends`,
					"lost work: git status and diff hash unchanged; each session file all JSON with a session entry",
					`poll every 250 ms for up to ${timeoutMs / 1000} s; report recovery rate, time to recovery and lost work`,
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const { herdrSession } = requireSandbox();
		const started = conditions(["herdr", "omp"]);
		const rows: H3Row[] = [];
		// The pane fault's folders: when each one's last run ended (0528).
		const lastEnded = new Map<string, number>();
		for (const fault of faults) {
			for (let run = 0; run < args.runs; run++) {
				if (fault === "pane") {
					const listed = await panes(herdrSession);
					const next = listed ? pickPane(listed, lastEnded, Date.now()) : undefined;
					if (next && next.waitMs > 0) {
						console.log(
							`pane run ${run}: waiting ${Math.ceil(next.waitMs / 1000)} s, one kill per folder per 10 minutes (0528)`,
						);
						await Bun.sleep(next.waitMs);
					}
				}
				const before = await panes(herdrSession);
				if (!before || before.length === 0) throw new Error(`herdr session ${herdrSession} lists no panes`);
				const folders = new Map(
					[
						...new Set(before.map((pane) => pane.cwd).filter((cwd): cwd is string => cwd !== null && existsSync(cwd))),
					].map((cwd) => [cwd, folderState(cwd)]),
				);
				let target: string | null = null;
				let killedFolder: string | null = null;
				let restartPid: number | undefined;
				let recoveredAfter: () => Promise<boolean>;
				let oldPaneIdsMs: () => number | null = () => null;
				const t0 = performance.now();
				if (fault === "herdr") {
					const shell = await paneShellPid(herdrSession, before[0]?.paneId ?? "");
					const server = snapshotProcesses().find((info) => info.pid === shell)?.ppid;
					if (!server) throw new Error("couldn't find herdr's server process (parent of a pane's shell)");
					target = String(server);
					killTree(server);
					const watch = herdrRecovery(before, t0);
					oldPaneIdsMs = watch.oldPaneIdsMs;
					recoveredAfter = async () => watch.check(await panes(herdrSession));
				} else if (fault === "companion") {
					const frontier = readFrontierFile();
					if (!frontier) throw new Error("no companion: <data>/frontier.json is missing");
					target = String(frontier.pid);
					// The companion alone: its panes live in herdr's process tree, not under it.
					process.kill(frontier.pid, "SIGKILL");
					await waitFor(() => !isAlive(frontier.pid), { timeoutMs: 10_000 });
					const restarted = spawn(restart, { shell: true, detached: true, stdio: "ignore", windowsHide: true });
					restarted.unref();
					restartPid = restarted.pid;
					recoveredAfter = async () => {
						const next = readFrontierFile();
						if (!next || next.pid === frontier.pid) return false;
						const now = await panes(herdrSession);
						return (await companionAnswers(next)) && now !== null && ompBack(before, now);
					};
				} else {
					const picked = pickPane(before, lastEnded, Date.now());
					if (!picked) throw new Error("no omp pane to kill");
					const pane = picked.pane;
					target = pane.paneId;
					killedFolder = folderKey(pane);
					const shell = await paneShellPid(herdrSession, pane.paneId);
					if (!shell) throw new Error(`herdr gave no process for pane ${pane.paneId}`);
					const tree = treeOf(shell, snapshotProcesses());
					// The pane's shell stays; omp and what it started go.
					for (const info of tree.slice(1).filter((p) => /omp|bun/i.test(p.name))) killTree(info.pid);
					await Bun.sleep(1_000);
					recoveredAfter = async () => {
						const now = await panes(herdrSession);
						return now !== null && ompBack(before, now);
					};
				}
				const waited = await waitFor(recoveredAfter, { timeoutMs, intervalMs: 250 });
				const recoveryMs = waited === null ? null : Math.round(performance.now() - t0);
				const listed = await panes(herdrSession);
				const after = listed ?? [];
				const checks = {
					samePanes: samePaneIds(before, after),
					foldersIntact: foldersIntact(folders, new Map([...folders.keys()].map((cwd) => [cwd, folderState(cwd)]))),
					sessionsParse: before
						.map((pane) => pane.sessionPath)
						.filter((path): path is string => path !== null && existsSync(path))
						.every((path) => sessionParses(readFileSync(path, "utf8"))),
				};
				// 0530: Jocasta closed itself (0518's heal can take herdr started again for him quitting): started again.
				const stoppedItself = closedItself(waited !== null, await companionAnswers(), listed);
				if (stoppedItself) console.log(`${fault} run ${run}: Jocasta closed itself; starting it again (0530)`);
				const restartMs = stoppedItself ? await startAgain(restart, before, herdrSession) : null;
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
					// One client at a time (0528): the restart's tree goes, root first; the new companion's stays.
					const processes = snapshotProcesses();
					const companion = readFrontierFile();
					const keep = new Set(companion ? treeOf(companion.pid, processes).map((info) => info.pid) : []);
					const ended = endRestart(restartPid, keep, processes);
					console.log(`companion run ${run}: ended the restart's client (${ended.length} processes)`);
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
			byFault,
		});
		console.log(`wrote ${file}`);
	});
}
