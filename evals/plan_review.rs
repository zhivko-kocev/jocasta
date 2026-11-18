//! Answering omp's plan review for A6 and its follow-up arms (0567). omp 18.20.2 offers "Approve and execute" and
//! "Approve and keep context" only in its interactive plan review (`modes/interactive-mode.ts:7771`
//! `handlePlanApproval`; "execute" clears the session before the plan-approved prompt, `#approvePlan`'s
//! `preserveContext: false`, `:6872`). Print mode has only `--plan-yolo`, which approves inside the planning session,
//! and ACP's approval keeps the context too, so neither is "execute". The interactive review is answerable without
//! keys: it emits `plan_review_requested` to extensions, and `ctx.resolvePlanReview(reviewId, "execute" | "keep")`
//! answers it "as if the operator picked" (`extensibility/extensions/types.ts:825-838`). So such a trial's first turn
//! runs omp interactive in a terminal (a bench herdr pane: interactive mode needs a TTY on stdin, `main.ts`), starting
//! in plan mode (`plan.defaultOnStartup`), with a per-trial extension (this file's [`plan_review_extension`]) that
//! answers the review with the arm's choice, logs what it saw, and shuts omp down once the approved plan's execution
//! has run: after the approval, the next run that started (an `agent_start`, or a user message, as the synthetic
//! plan-approved prompt is) and has an assistant reply, at an `agent_end` with omp idle and nothing queued. A planning
//! turn that ends without proposing a plan shuts down too; the trial is graded as it stands.

use jocasta::shared::jsonl::json_lines;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The review's answer: "Approve and execute" or "Approve and keep context".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanReviewChoice {
	Execute,
	Keep,
}

impl PlanReviewChoice {
	/// The choice as a variant file and the extension spell it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Execute => "execute",
			Self::Keep => "keep",
		}
	}
}

/// A trial's plan review as the extension logged it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanReviewSummary {
	/// omp opened its plan review.
	pub requested: bool,
	/// The choice the extension gave and whether omp took it (`resolvePlanReview` returned true).
	pub choice: PlanReviewChoice,
	pub answered: bool,
	/// The role the approval asked the work to run on (0608); absent when the review's own tier stays.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub role: Option<String>,
	/// The choice omp reported when the review closed.
	pub resolved: Option<String>,
	/// Why the extension shut omp down; `None` when omp ended otherwise (a crash, the trial's limit).
	pub shutdown: Option<String>,
}

/// The extension's behavior after its constants, as plain JavaScript omp loads with `-e`.
const EXTENSION_BODY: &str = r#"const log = (line) => appendFileSync(LOG, JSON.stringify({ at: Date.now(), ...line }) + "\n");
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
		const answered = ctx.resolvePlanReview(event.reviewId, CHOICE, ROLE === null ? undefined : { role: ROLE });
		log({ event: "answered", choice: CHOICE, ...(ROLE === null ? {} : { role: ROLE }), answered });
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
"#;

/// `value` as a JavaScript literal (`JSON.stringify`).
fn literal(value: &impl Serialize) -> String {
	serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

/// The extension module's source for one trial: answers with `choice` (and `role`, the execution tier of the review's
/// slider, 0608: `resolvePlanReview(reviewId, choice, { role })`) and appends one JSON line per event to `log_file`.
/// Plain JavaScript, so omp loads it with `-e` from the trial root without a build.
pub fn plan_review_extension(choice: PlanReviewChoice, log_file: &str, role: Option<&str>) -> String {
	format!(
		"// Written by Jocasta's task benchmark (evals/plan_review.rs) for one trial: answers omp's plan review.\nimport {{ appendFileSync }} from \"node:fs\";\nconst CHOICE = {};\nconst ROLE = {};\nconst LOG = {};\n{EXTENSION_BODY}",
		literal(&choice.as_str()),
		literal(&role),
		literal(&log_file),
	)
}

/// The text of the first event named `name`, read as `key`.
fn first_text(events: &[Value], name: &str, key: &str) -> Option<String> {
	events
		.iter()
		.find(|event| event.get("event").and_then(Value::as_str) == Some(name))
		.and_then(|event| event.get(key))
		.and_then(Value::as_str)
		.map(str::to_owned)
}

/// The extension's log as a summary; `choice` and `role` are what the trial asked for.
pub fn read_plan_review_log(text: &str, choice: PlanReviewChoice, role: Option<&str>) -> PlanReviewSummary {
	let events: Vec<Value> =
		json_lines::<Value>(text).into_iter().filter(|line| line.get("event").is_some_and(Value::is_string)).collect();
	let named = |name: &str| events.iter().any(|e| e.get("event").and_then(Value::as_str) == Some(name));
	PlanReviewSummary {
		requested: named("requested"),
		choice,
		answered: events.iter().any(|e| {
			e.get("event").and_then(Value::as_str) == Some("answered") && e.get("answered") == Some(&Value::Bool(true))
		}),
		role: role.map(str::to_owned),
		resolved: first_text(&events, "resolved", "choice"),
		shutdown: first_text(&events, "shutdown", "reason"),
	}
}
