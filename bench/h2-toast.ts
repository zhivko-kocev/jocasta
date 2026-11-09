// H2 (0482, 0552): the time from "needs you" to his notification, in a sandbox Jocasta.
//
//   bun bench/h2-toast.ts --out <dir> [--runs 30] [--minutes 120] [--condition events|tick] [--start <project>]
//                         [--dry-run]
//
// There is no companion app (0552): Jocasta raises its notifications through herdr's `notification.show`, which
// herdr hands to Windows' notification service through the foreground client and answers `shown` (or `disabled`,
// `rate_limited`, `no_foreground_client`, `busy`). While it runs, H2 records herdr's `pane.agent_status_changed` pushes
// for every pane (t1, `herdr.ts` `watch`) and reads the service's notification log, `<data>/notifications.jsonl`, one
// line per `notification.show` call written when herdr's reply comes back (`at`, `paneId`, `reason`, `title`, `body`,
// `result`). Each pane's omp session file (herdr reports its path) gives t0: an `ask` tool call or a plan proposal.
// t3 is now the moment herdr answered `shown` for the pane's `blocked` notification: the notification reached Windows.
// The companion's toast broadcast (the old t2) and Jocasta's own toast window (0520's t3) are gone; how long Windows
// then takes to draw the banner isn't measured. Trials come from threads he starts, or with `--start <project>` from
// `thread_start` calls (boss key in `JOCASTA_BOSS_KEY`; each brief makes omp ask one question, so a model runs: VM
// only). `--condition` labels the run: `events` with herdr's stream, `tick` with the stream cut by hand.
// Writes h2-toast.jsonl (one row per trial), h2-notifications.jsonl (the log's lines of this run) and h2-toast.json
// (median, p95 and CIs of t1−t0 and t3−t0, and the count of each herdr result).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type AgentState, connectHerdr } from "../src/frontier/server/herdr.ts";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "./lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "./lib/output.ts";
import { callTool, requireSandbox } from "./lib/sandbox.ts";
import { askTimes, planReadyTimes } from "./lib/sessions.ts";
import { summarize } from "./lib/stats.ts";
import { jsonLines } from "./lib/timing.ts";

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
	/** The toast's reason as the service names it: `blocked`, `done` or `event`. */
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
		const condition = flag(args, "condition", "events");
		if (condition !== "events" && condition !== "tick") throw new BenchUsageError("--condition is events or tick");
		if (!Number.isFinite(minutes) || minutes <= 0) throw new BenchUsageError("--minutes needs a number above 0");
		const project = flag(args, "start");
		if (args.dryRun) {
			printPlan(
				"H2 toast latency",
				[
					`condition ${condition}; collect ${args.runs} trials or stop after ${minutes} min`,
					"record herdr pane.agent_status_changed pushes (t1)",
					`read the service's <data>/${NOTIFICATIONS_FILE}: t3 = herdr answered \`shown\` to the pane's blocked notification.show`,
					"t0 from each pane's omp session file: an ask tool call or a plan proposal",
					project
						? `start each trial with thread_start in ${project} (model call; VM only): "${ASK_BRIEF}"`
						: "trials come from threads started by hand",
					"report median, p95 and bootstrap 95% CIs of t1−t0 and t3−t0, and how often herdr answered each result",
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const sandbox = requireSandbox();
		const bossKey = process.env.JOCASTA_BOSS_KEY;
		if (project && !bossKey) throw new BenchUsageError("--start needs the boss key in JOCASTA_BOSS_KEY");
		const started = conditions(["omp", "herdr"]);
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
		const refreshPanes = async () => {
			for (const pane of await herdr.panes()) if (pane.sessionPath) sessionPaths.set(pane.paneId, pane.sessionPath);
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
		let launched = 0;
		let trials: H2Trial[] = [];
		try {
			while (Date.now() < deadline) {
				const answered = trials.filter((t) => t.notificationResult !== null).length;
				if (project && launched < args.runs && answered >= launched) {
					launched++;
					const reply = await callTool(
						"thread_start",
						{ project, title: `h2 trial ${launched}`, brief: ASK_BRIEF },
						{ bossKey },
					);
					if (reply.isError) throw new Error(`thread_start failed: ${reply.text}`);
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
		});
		console.log(`${trials.length} trials; wrote ${file}`);
	});
}
