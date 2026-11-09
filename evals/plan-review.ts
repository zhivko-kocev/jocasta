import { jsonLines } from "../src/shared/jsonl.ts";

/**
 * Answering omp's plan review for A6 and its follow-up arms (0567). omp 18.18.2 offers "Approve and execute" and
 * "Approve and keep context" only in its interactive plan review (`modes/interactive-mode.ts`, `#handlePlanProposal`;
 * "execute" clears the session before the plan-approved prompt, `#approvePlan`'s `preserveContext: false`). Print mode
 * has only `--plan-yolo`, which approves inside the planning session, and ACP's approval keeps the context too, so
 * neither is "execute". The interactive review is answerable without keys: it emits `plan_review_requested` to
 * extensions, and `ctx.resolvePlanReview(reviewId, "execute" | "keep")` answers it "as if the operator picked"
 * (`extensibility/extensions/types.ts`). So such a trial's first turn runs omp interactive in a terminal (a bench herdr
 * pane: interactive mode needs a TTY on stdin, `main.ts`), starting in plan mode (`plan.defaultOnStartup`), with a
 * per-trial extension (this file's {@link planReviewExtension}) that answers the review with the arm's choice, logs
 * what it saw, and shuts omp down once the approved plan's execution has run: after the approval, the next run that
 * started (an `agent_start`, or a user message, as the synthetic plan-approved prompt is) and has an assistant reply,
 * at an `agent_end` with omp idle and nothing queued. A planning turn that ends without proposing a plan shuts down
 * too; the trial is graded as it stands.
 */

export type PlanReviewChoice = "execute" | "keep";

/** The extension's log lines: what happened, with epoch ms. */
export interface PlanReviewEvent {
	at: number;
	event: "requested" | "answered" | "resolved" | "shutdown";
	choice?: string;
	answered?: boolean;
	reason?: string;
}

/** A trial's plan review as the extension logged it. */
export interface PlanReviewSummary {
	/** omp opened its plan review. */
	requested: boolean;
	/** The choice the extension gave and whether omp took it (`resolvePlanReview` returned true). */
	choice: PlanReviewChoice;
	answered: boolean;
	/** The choice omp reported when the review closed. */
	resolved: string | null;
	/** Why the extension shut omp down; null when omp ended otherwise (a crash, the trial's limit). */
	shutdown: string | null;
}

/**
 * The extension module's source for one trial: answers with `choice` and appends {@link PlanReviewEvent} lines to
 * `logFile`. Plain JavaScript, so omp loads it with `-e` from the trial root without a build.
 */
export function planReviewExtension(choice: PlanReviewChoice, logFile: string): string {
	return `// Written by Jocasta's task benchmark (evals/plan-review.ts) for one trial: answers omp's plan review.
import { appendFileSync } from "node:fs";
const CHOICE = ${JSON.stringify(choice)};
const LOG = ${JSON.stringify(logFile)};
const log = (line) => appendFileSync(LOG, JSON.stringify({ at: Date.now(), ...line }) + "\\n");
export default function planReview(pi) {
	let requested = false;
	let approved = false;
	let executionStarted = false;
	let repliedSinceStart = false;
	let done = false;
	const stop = (ctx, reason) => {
		if (done) return;
		done = true;
		log({ event: "shutdown", reason });
		ctx.shutdown();
	};
	pi.on("plan_review_requested", (event, ctx) => {
		requested = true;
		log({ event: "requested" });
		const answered = ctx.resolvePlanReview(event.reviewId, CHOICE);
		log({ event: "answered", choice: CHOICE, answered });
	});
	pi.on("plan_review_resolved", (event) => {
		log({ event: "resolved", choice: event.choice });
		if (event.choice === "execute" || event.choice === "keep" || event.choice === "compact") approved = true;
	});
	pi.on("agent_start", () => {
		if (approved && !executionStarted) executionStarted = true;
	});
	pi.on("message_end", (event) => {
		const role = event.message?.role;
		if (approved && !executionStarted && role === "user") executionStarted = true;
		else if (executionStarted && role === "assistant") repliedSinceStart = true;
	});
	pi.on("agent_end", (_event, ctx) => {
		if (!ctx.isIdle() || ctx.hasPendingMessages()) return;
		if (!requested) stop(ctx, "turn ended without a plan proposal");
		else if (executionStarted && repliedSinceStart) stop(ctx, "the approved plan's execution ended");
	});
}
`;
}

/** The extension's log as a summary; `choice` is what the trial asked for. */
export function readPlanReviewLog(text: string, choice: PlanReviewChoice): PlanReviewSummary {
	const events = jsonLines<PlanReviewEvent>(text).filter((line) => typeof line.event === "string");
	return {
		requested: events.some((e) => e.event === "requested"),
		choice,
		answered: events.some((e) => e.event === "answered" && e.answered === true),
		resolved: events.find((e) => e.event === "resolved")?.choice ?? null,
		shutdown: events.find((e) => e.event === "shutdown")?.reason ?? null,
	};
}
