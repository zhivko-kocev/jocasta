// H2 (0482, 0552, 0576): the time from "needs you" to his notification, in a sandbox Jocasta.
//
//   bun bench/h2-toast.ts --out <dir> [--runs 30] [--minutes 120] [--condition host|herdr] [--start <project>]
//                         [--dry-run]
//
// Jocasta raises its notifications through herdr's `notification.show`, which herdr hands to Windows' notification
// service through the foreground client and answers `shown` (or `disabled`, `rate_limited`, `no_foreground_client`,
// `busy`). While it runs, H2 records herdr's `pane.agent_status_changed` pushes for every pane (t1, `herdr.ts`
// `watch`; a reference figure, since the service's own path is the condition's) and reads the service's notification
// log, `<data>/notifications.jsonl`, one line per `notification.show` call written when herdr's reply comes back
// (`at`, `paneId`, `reason`, `title`, `body`, `result`). Each pane's omp session file (herdr reports its path) gives
// t0: an `ask` tool call's message once complete (`completedAt`, not the request's start, so the model's generation
// isn't in the figures) or a plan proposal. t3 is the moment herdr answered `shown` for the pane's `blocked`
// notification: the notification reached Windows; how long Windows then takes to draw the banner isn't measured.
// Conditions (0576): `host` (the default), the service reading each thread's state from omp's collab host, polled
// every 1 s (0547); `herdr`, the sandbox service started with `JOCASTA_HOSTS=off`, so it ignores omp's host registry
// and falls back to herdr's events and its tick. H2 reads the service's switch from `/health` and refuses a mismatch.
// Trials come from threads he starts (the boss's and the today pane left out), or with `--start <project>` from
// threads H2 starts as `jocasta thread new` does and reads as `jocasta thread list` does (no Allow, 0544, 0576; each
// brief makes omp ask one question, so a model runs: VM only): then only those threads' panes are trials (a listed
// thread's pane is the one in its branch's worktree), and the next thread starts once the last one's
// notification came, or after 5 minutes without one, which the summary counts as a miss.
// Writes h2-toast.jsonl (one row per trial), h2-notifications.jsonl (the log's lines of this run) and h2-toast.json
// (median, p95 and CIs of t1−t0 and t3−t0, and the count of each herdr result).
import { existsSync, readFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { BOSS_WORKSPACE, isTodayPane } from "../src/frontier/server/boss.ts";
import { type AgentState, connectHerdr, type PaneInfo } from "../src/frontier/server/herdr.ts";
import { jsonLines } from "../src/shared/jsonl.ts";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "./lib/args.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";
import { requireSandbox, requireServiceSwitch } from "./lib/sandbox.ts";
import { askTimes, planReadyTimes } from "./lib/sessions.ts";
import { summarize } from "./lib/stats.ts";
import { listThreads, startThread, type ThreadLine } from "./lib/threads.ts";

export const CONDITIONS = ["host", "herdr"] as const;
export type Condition = (typeof CONDITIONS)[number];

export interface HerdrEvent {
	at: number;
	paneId: string;
	state: AgentState;
}

/** One line of the service's `<data>/notifications.jsonl`: a `notification.show` call and herdr's reply. */
export interface NotificationEntry {
	/** Epoch ms when herdr's reply came back. */
	at: number;
	paneId: string | null;
	/** The toast's reason as the service names it: `blocked`, `done`, `failed` or `event`. */
	reason: string;
	title: string;
	body?: string;
	/** herdr's reason (`shown`, `disabled`, `rate_limited`, `no_foreground_client`, `busy`) or `error`. */
	result: string;
}

/** The service's notification log as entries; other lines (partial, malformed) are skipped. */
export function parseNotifications(text: string): NotificationEntry[] {
	return jsonLines<Partial<NotificationEntry>>(text).filter(
		(line): line is NotificationEntry =>
			typeof line.at === "number" && typeof line.result === "string" && typeof line.reason === "string",
	);
}

export const NOTIFICATIONS_FILE = "notifications.jsonl";

/** How long a started thread has to bring its notification before the next one starts (BEN-23). */
export const TRIAL_LIMIT_MS = 5 * 60_000;

export interface H2Trial {
	paneId: string;
	kind: "ask" | "plan";
	t0: number;
	/** ms after t0; null when not seen. */
	herdrMs: number | null;
	/** t3 − t0: herdr answered `shown` for the pane's `blocked` notification; null when it didn't. */
	notificationMs: number | null;
	/** herdr's answer to the pane's first `blocked` notification after t0; null when the service raised none. */
	notificationResult: string | null;
}

/**
 * Pairs each "needs you" moment (t0) in a pane's session with the first herdr state change to blocked/idle/done and
 * the pane's `blocked` notifications, each at or after t0 and before the pane's next t0: t3 is the first one herdr
 * answered `shown`.
 */
export function correlate(
	sessions: ReadonlyMap<string, string>,
	events: readonly HerdrEvent[],
	notifications: readonly NotificationEntry[],
): H2Trial[] {
	const blocked = notifications.filter((entry) => entry.reason === "blocked");
	const trials: H2Trial[] = [];
	for (const [paneId, text] of sessions) {
		const moments = [
			...askTimes(text).map((t0) => ({ t0, kind: "ask" as const })),
			...planReadyTimes(text).map((t0) => ({ t0, kind: "plan" as const })),
		]
			.filter((moment) => Number.isFinite(moment.t0))
			.sort((a, b) => a.t0 - b.t0);
		moments.forEach(({ t0, kind }, i) => {
			const next = moments[i + 1]?.t0 ?? Number.POSITIVE_INFINITY;
			const within = (at: number) => at >= t0 && at < next;
			const herdr = events.find(
				(event) =>
					event.paneId === paneId && within(event.at) && event.state !== "working" && event.state !== "unknown",
			);
			const ours = blocked.filter((entry) => entry.paneId === paneId && within(entry.at));
			const shown = ours.find((entry) => entry.result === "shown");
			trials.push({
				paneId,
				kind,
				t0,
				herdrMs: herdr ? herdr.at - t0 : null,
				notificationMs: shown ? shown.at - t0 : null,
				notificationResult: ours[0]?.result ?? null,
			});
		});
	}
	return trials.sort((a, b) => a.t0 - b.t0);
}

/**
 * The pane of the thread on `branch` (`feature/<slug>`): the one working in a folder `<slug>` inside a `<project>.wt`
 * folder, where the service puts a thread's worktree (`threadPlace`).
 */
export function paneOfBranch(panes: readonly Pick<PaneInfo, "paneId" | "cwd">[], branch: string): string | undefined {
	const slug = branch.replace(/^feature\//, "");
	return panes.find((pane) => {
		if (!pane.cwd) return false;
		const cwd = pane.cwd.replace(/[\\/]+$/, "");
		return basename(cwd) === slug && basename(dirname(cwd)).endsWith(".wt");
	})?.paneId;
}

/**
 * Gives each of the bench's threads without a pane yet its pane once `jocasta thread list` shows a thread on its
 * branch (0576): the pane working in that branch's worktree (the list names no pane).
 */
export function findListedPanes(
	ours: { branch: string; paneId?: string }[],
	listed: readonly Pick<ThreadLine, "branch">[],
	panes: readonly Pick<PaneInfo, "paneId" | "cwd">[],
): void {
	const branches = new Set(listed.map((line) => line.branch));
	for (const thread of ours) if (branches.has(thread.branch)) thread.paneId ??= paneOfBranch(panes, thread.branch);
}

/** The brief `--start` gives each thread: one question with omp's ask tool, then nothing else. */
const ASK_BRIEF =
	"Benchmark H2: use the ask tool once to ask me whether the greeting should say hello or hi, then stop and change nothing.";

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 30,
			valued: ["--minutes", "--condition", "--start"],
		});
		const minutes = Number(flag(args, "minutes", "120"));
		const condition = flag(args, "condition", "host") as Condition;
		if (!CONDITIONS.includes(condition)) throw new BenchUsageError("--condition is host or herdr");
		if (!Number.isFinite(minutes) || minutes <= 0) throw new BenchUsageError("--minutes needs a number above 0");
		const project = flag(args, "start");
		if (args.dryRun) {
			printPlan(
				"H2 toast latency",
				[
					`condition ${condition === "host" ? "host (omp's collab host, polled every 1 s)" : "herdr (service started with JOCASTA_HOSTS=off: herdr's events and tick)"}; collect ${args.runs} trials or stop after ${minutes} min`,
					"refuse a run whose condition doesn't match the sandbox service's /health",
					"record herdr pane.agent_status_changed pushes (t1, a reference figure)",
					`read the service's <data>/${NOTIFICATIONS_FILE}: t3 = herdr answered \`shown\` to the pane's blocked notification.show`,
					"t0 from each pane's omp session file: an ask tool call's completed message or a plan proposal",
					project
						? `start each trial as \`jocasta thread new ${project}\` does (no Allow; model call: VM only), the next after the last one's notification or ${TRIAL_LIMIT_MS / 60_000} min: "${ASK_BRIEF}"`
						: "trials come from threads started by hand (the boss's and the today pane left out)",
					"report median, p95 and bootstrap 95% CIs of t1−t0 and t3−t0, and how often herdr answered each result",
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const sandbox = requireSandbox();
		claimRows(out, "h2-toast", "h2-notifications");
		await requireServiceSwitch("hosts", condition === "herdr" ? "off" : "on");
		const started = conditions(["omp", "herdr", "jocasta"]);
		const startedAt = Date.now();
		const logFile = join(sandbox.dataDir, NOTIFICATIONS_FILE);
		const readLog = () =>
			existsSync(logFile)
				? parseNotifications(readFileSync(logFile, "utf8")).filter((entry) => entry.at >= startedAt)
				: [];
		const herdr = await connectHerdr({ session: sandbox.herdrSession });
		// The service restarts herdr, not this recorder.
		herdr.keepDown();
		const events: HerdrEvent[] = [];
		const sessionPaths = new Map<string, string>();
		// With --start, the bench's own threads by branch, their pane once `jocasta thread list` shows them, and when each
		// started.
		const ours: { branch: string; startedAt: number; paneId?: string }[] = [];
		const refreshPanes = async () => {
			const { workspaces, panes } = await herdr.snapshot();
			const boss = workspaces.find((w) => w.label === BOSS_WORKSPACE)?.id;
			if (project && ours.some((thread) => thread.paneId === undefined))
				findListedPanes(ours, await listThreads(project), panes);
			const trialPanes = project
				? panes.filter((pane) => ours.some((thread) => thread.paneId === pane.paneId))
				: panes.filter((pane) => pane.workspaceId !== boss && !isTodayPane(pane));
			for (const pane of trialPanes) if (pane.sessionPath) sessionPaths.set(pane.paneId, pane.sessionPath);
		};
		const readSessions = () =>
			new Map(
				[...sessionPaths]
					.filter(([, path]) => existsSync(path))
					.map(([pane, path]) => [pane, readFileSync(path, "utf8")]),
			);
		await refreshPanes();
		const stopWatch = herdr.watch(
			() => [...sessionPaths.keys()],
			(event) => events.push({ at: Date.now(), paneId: event.paneId, state: event.state }),
		);
		const deadline = Date.now() + minutes * 60_000;
		let trials: H2Trial[] = [];
		const answeredIn = (paneId: string | undefined) =>
			paneId !== undefined && trials.some((t) => t.paneId === paneId && t.notificationResult !== null);
		try {
			while (Date.now() < deadline) {
				const last = ours.at(-1);
				if (
					project &&
					ours.length < args.runs &&
					(last === undefined || answeredIn(last.paneId) || Date.now() - last.startedAt >= TRIAL_LIMIT_MS)
				) {
					const at = Date.now();
					ours.push({ branch: await startThread(project, `h2 trial ${ours.length + 1}`, ASK_BRIEF), startedAt: at });
				}
				await Bun.sleep(2_000);
				await refreshPanes();
				trials = correlate(readSessions(), events, readLog());
				if (trials.filter((trial) => trial.notificationResult !== null).length >= args.runs) break;
			}
		} finally {
			stopWatch();
			herdr.close();
		}
		const notifications = readLog();
		for (const entry of notifications) appendRow(out, "h2-notifications", entry);
		trials = correlate(readSessions(), events, notifications).slice(0, args.runs);
		for (const trial of trials) appendRow(out, "h2-toast", { condition, ...trial });
		const seen = (key: "herdrMs" | "notificationMs") =>
			summarize(trials.map((trial) => trial[key]).filter((ms): ms is number => ms !== null));
		const results: Record<string, number> = {};
		for (const trial of trials) {
			const result = trial.notificationResult ?? "none";
			results[result] = (results[result] ?? 0) + 1;
		}
		const file = writeJson(out, "h2-toast", {
			benchmark: "H2",
			condition,
			endPoint: "t3 = herdr answered `shown` to the service's notification.show for the pane's blocked toast (0552)",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			trials: trials.length,
			herdrMs: seen("herdrMs"),
			notificationMs: seen("notificationMs"),
			notificationResults: results,
			...(project
				? {
						started: ours.length,
						missed: ours.filter((thread) => !answeredIn(thread.paneId)).map((thread) => thread.branch),
					}
				: {}),
		});
		console.log(`${trials.length} trials; wrote ${file}`);
	});
}
