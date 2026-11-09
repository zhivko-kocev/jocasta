// A5 (0482): his case study, 10 real tasks end to end through the boss. This tool records and counts; the tasks are
// his.
//
//   bun evals/a5.ts record --out <dir> [--dry-run]
//     appends the companion's feed (toasts and every other broadcast) to <dir>/companion-feed.jsonl until Ctrl+C.
//   bun evals/a5.ts report --feed <companion-feed.jsonl> --boss <boss session.jsonl> --threads <session.jsonl,…>
//                          [--cap <minutes>] [--out <dir>] [--dry-run]
//     interventions per thread, attention minutes (each urgent toast to his next response) and the batch time.
//
// His responses: his user messages in the boss session (not the service's `[event]` lines) and in thread sessions
// (not each thread's brief, the review prompt or `/handoff`, which the service types), and plan approvals (leaving
// plan mode after a plan was ready). A boss relay of his words into a thread (`thread_send`) counts in both sessions.
import { readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "../bench/lib/args.ts";
import { type FeedEntry, parseFeed, recordFeed, toastsOf } from "../bench/lib/companion-feed.ts";
import { appendRow, printPlan, writeJson } from "../bench/lib/output.ts";
import { planApprovalTimes, sessionStart, type UserMessage, userMessages } from "../bench/lib/sessions.ts";
import { type Summary, summarize } from "../bench/lib/stats.ts";

/**
 * Lines the service itself sends into panes: `[event]` lines into the boss (`service.ts` `event`/`alert`), the review
 * prompt (`hunk.ts` `reviewPrompt`) and the handoff (`service.ts` `HANDOFF_PROMPT`) into threads.
 */
const SERVICE_PREFIXES = ["[event]", "Check this task's work.", "/handoff"];

const byService = (message: UserMessage) =>
	SERVICE_PREFIXES.some((prefix) => message.text.trimStart().startsWith(prefix));

export interface ThreadInput {
	name: string;
	session: string;
}

export interface A5Input {
	feed: readonly FeedEntry[];
	boss: string;
	threads: readonly ThreadInput[];
	/** A toast unanswered this long counts as unanswered, not as attention. */
	capMs: number;
}

export interface A5Report {
	threads: { name: string; interventions: number; messages: number; approvals: number }[];
	bossMessages: number;
	interventions: number;
	urgentToasts: number;
	answered: number;
	attentionMinutes: number;
	/** Per answered toast, minutes to his response. */
	responseMinutes: Summary;
	/** First thread start to the last `done` toast, minutes; null without both. */
	batchMinutes: number | null;
}

export function a5Report(input: A5Input): A5Report {
	const bossMessages = userMessages(input.boss).filter((message) => !byService(message));
	const threads = input.threads.map((thread) => {
		// The first user message is the brief `thread_start` typed.
		const messages = userMessages(thread.session)
			.slice(1)
			.filter((message) => !byService(message));
		const approvals = planApprovalTimes(thread.session);
		return { name: thread.name, messages, approvals, start: sessionStart(thread.session) };
	});
	const responses = [
		...bossMessages.map((message) => message.at),
		...threads.flatMap((thread) => [...thread.messages.map((message) => message.at), ...thread.approvals]),
	]
		.filter(Number.isFinite)
		.sort((a, b) => a - b);
	const urgent = toastsOf(input.feed, true);
	const waits: number[] = [];
	for (const toast of urgent) {
		const response = responses.find((at) => at >= toast.at);
		if (response !== undefined && response - toast.at <= input.capMs) waits.push(response - toast.at);
	}
	const starts = threads.map((thread) => thread.start).filter(Number.isFinite);
	const done = toastsOf(input.feed)
		.filter((toast) => toast.message.reason === "done")
		.map((toast) => toast.at);
	const minutes = (ms: number) => ms / 60_000;
	return {
		threads: threads.map((thread) => ({
			name: thread.name,
			interventions: thread.messages.length + thread.approvals.length,
			messages: thread.messages.length,
			approvals: thread.approvals.length,
		})),
		bossMessages: bossMessages.length,
		interventions:
			bossMessages.length + threads.reduce((sum, thread) => sum + thread.messages.length + thread.approvals.length, 0),
		urgentToasts: urgent.length,
		answered: waits.length,
		attentionMinutes: minutes(waits.reduce((sum, wait) => sum + wait, 0)),
		responseMinutes: summarize(waits.map(minutes)),
		batchMinutes: starts.length > 0 && done.length > 0 ? minutes(Math.max(...done) - Math.min(...starts)) : null,
	};
}

if (import.meta.main) {
	await runScript(async () => {
		const [mode, ...rest] = process.argv.slice(2);
		if (mode === "record") {
			const args = parseBenchArgs(rest, { defaultRuns: 1 });
			const file = args.out === undefined ? "<out>/companion-feed.jsonl" : join(args.out, "companion-feed.jsonl");
			if (args.dryRun) {
				printPlan(
					"A5 record",
					[
						"connect to the companion that <data>/frontier.json names, as the capsule view (his window stays)",
						`append every broadcast with its arrival time to ${file} until Ctrl+C`,
					],
					args.out,
				);
				return;
			}
			const recorder = await recordFeed(file);
			console.log(`recording the companion's feed into ${file}; Ctrl+C stops`);
			process.on("SIGINT", () => recorder.stop());
			await recorder.closed;
			return;
		}
		if (mode !== "report") throw new BenchUsageError("usage: bun evals/a5.ts record|report …");
		const args = parseBenchArgs(rest, { defaultRuns: 1, valued: ["--feed", "--boss", "--threads", "--cap"] });
		const feedPath = flag(args, "feed");
		const bossPath = flag(args, "boss");
		const threadPaths = listFlag(args, "threads", []);
		const cap = Number(flag(args, "cap", "60"));
		if (!Number.isFinite(cap) || cap <= 0) throw new BenchUsageError("--cap needs minutes above 0");
		if (args.dryRun) {
			printPlan(
				"A5 report",
				[
					`toasts from ${feedPath ?? "<--feed>"}; his boss messages from ${bossPath ?? "<--boss>"}`,
					`${threadPaths.length} thread sessions: interventions = his messages after the brief + plan approvals`,
					`attention = Σ (urgent toast → his next response), responses later than ${cap} min count as unanswered`,
					"batch time = first thread start → last done toast",
				],
				args.out,
			);
			return;
		}
		if (feedPath === undefined || bossPath === undefined || threadPaths.length === 0)
			throw new BenchUsageError("report needs --feed, --boss and --threads");
		const report = a5Report({
			feed: parseFeed(readFileSync(feedPath, "utf8")),
			boss: readFileSync(bossPath, "utf8"),
			threads: threadPaths.map((path) => ({ name: basename(path), session: readFileSync(path, "utf8") })),
			capMs: cap * 60_000,
		});
		console.log(JSON.stringify(report, null, 2));
		if (args.out !== undefined) {
			for (const thread of report.threads) appendRow(args.out, "a5", thread);
			console.log(`wrote ${writeJson(args.out, "a5", { benchmark: "A5", report })}`);
		}
	});
}
