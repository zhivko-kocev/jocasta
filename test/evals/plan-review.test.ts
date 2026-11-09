import { expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { planReviewExtension, readPlanReviewLog } from "../../evals/plan-review.ts";
import { type EvalCase, PLAN_REVIEW_OVERLAY, runTrial } from "../../evals/runner.ts";
import { envDelta, herdrTerminal, type TerminalRunner, TRIAL_WORKSPACE } from "../../evals/terminal.ts";
import { loadVariants } from "../../evals/variants.ts";
import { useTempDirs } from "../shared/fixture.ts";

const tempDir = useTempDirs("jocasta-review-");

type Handler = (event: Record<string, unknown>, ctx: unknown) => void;

/** Loads a generated extension with a stand-in for omp's extension API; `fire` sends it an event. */
async function loadExtension(file: string) {
	const handlers = new Map<string, Handler[]>();
	const calls: { resolved: [string, string][]; shutdowns: number } = { resolved: [], shutdowns: 0 };
	let idle = true;
	const ctx = {
		resolvePlanReview: (id: string, choice: string) => {
			calls.resolved.push([id, choice]);
			return true;
		},
		isIdle: () => idle,
		hasPendingMessages: () => false,
		shutdown: () => {
			calls.shutdowns++;
		},
	};
	const module = (await import(file)) as { default: (pi: unknown) => void };
	module.default({
		on: (event: string, handler: Handler) => handlers.set(event, [...(handlers.get(event) ?? []), handler]),
	});
	const fire = (event: string, payload: Record<string, unknown> = {}) => {
		for (const handler of handlers.get(event) ?? []) handler(payload, ctx);
	};
	return { fire, calls, setIdle: (value: boolean) => (idle = value) };
}

/** omp's events around an approval: the planning run, the review, then the execution run the approval starts. */
function approveAndExecute(fire: (event: string, payload?: Record<string, unknown>) => void, choice: string) {
	fire("agent_start");
	fire("message_end", { message: { role: "assistant" } });
	fire("plan_review_requested", { reviewId: "r1" });
	fire("plan_review_resolved", { reviewId: "r1", choice });
	// The planning run ends after the approval, before the execution run: no shutdown yet.
	fire("agent_end");
	fire("message_end", { message: { role: "user" } });
	fire("agent_start");
	fire("message_end", { message: { role: "assistant" } });
	fire("agent_end");
}

test("the extension answers the review with the arm's choice and shuts omp down after the execution, not before", async () => {
	const dir = tempDir();
	const log = join(dir, "plan-review.jsonl");
	const file = join(dir, "plan-review.js");
	writeFileSync(file, planReviewExtension("execute", log));
	const { fire, calls } = await loadExtension(file);
	fire("agent_start");
	fire("plan_review_requested", { reviewId: "r1" });
	expect(calls.resolved).toEqual([["r1", "execute"]]);
	fire("plan_review_resolved", { reviewId: "r1", choice: "execute" });
	fire("agent_end");
	expect(calls.shutdowns).toBe(0);
	fire("message_end", { message: { role: "user" } });
	fire("agent_start");
	fire("message_end", { message: { role: "assistant" } });
	fire("agent_end");
	expect(calls.shutdowns).toBe(1);
	expect(readPlanReviewLog(readFileSync(log, "utf8"), "execute")).toEqual({
		requested: true,
		choice: "execute",
		answered: true,
		resolved: "execute",
		shutdown: "the approved plan's execution ended",
	});
});

test("keep-context answers keep; a planning turn that ends without a proposal shuts down too; busy omp waits", async () => {
	const dir = tempDir();
	const keepFile = join(dir, "keep.js");
	writeFileSync(keepFile, planReviewExtension("keep", join(dir, "keep.jsonl")));
	const keep = await loadExtension(keepFile);
	approveAndExecute(keep.fire, "keep");
	expect(keep.calls.resolved).toEqual([["r1", "keep"]]);
	expect(keep.calls.shutdowns).toBe(1);

	const noPlanFile = join(dir, "none.js");
	writeFileSync(noPlanFile, planReviewExtension("execute", join(dir, "none.jsonl")));
	const none = await loadExtension(noPlanFile);
	none.setIdle(false);
	none.fire("agent_end");
	expect(none.calls.shutdowns).toBe(0);
	none.setIdle(true);
	none.fire("agent_end");
	expect(none.calls.shutdowns).toBe(1);
	expect(readPlanReviewLog(readFileSync(join(dir, "none.jsonl"), "utf8"), "execute")).toMatchObject({
		requested: false,
		answered: false,
		shutdown: "turn ended without a plan proposal",
	});
	expect(readPlanReviewLog("", "keep")).toEqual({
		requested: false,
		choice: "keep",
		answered: false,
		resolved: null,
		shutdown: null,
	});
});

test("a pane's environment carries what the trial changed, and empties what it dropped", () => {
	expect(envDelta({ A: "1", B: "changed", D: "new" }, { A: "1", B: "2", C: "3" })).toEqual({
		B: "changed",
		D: "new",
		C: "",
	});
});

test("a dropped herdr read never ends the turn: the pane's process is watched locally until it exits (EVL-6, EVL-17)", async () => {
	const work = tempDir();
	const calls: string[] = [];
	let reads = 0;
	let child: Bun.Subprocess | undefined;
	const fake = {
		async ensureWorkspace(label: string, cwd: string) {
			calls.push(`workspace ${label} in ${cwd}`);
			return "w1";
		},
		async createTab(_workspace: string, tab: { command: string[]; cwd: string }) {
			child = Bun.spawn(tab.command, { cwd: tab.cwd, stdio: ["ignore", "ignore", "ignore"], windowsHide: true });
			return { tabId: "t1", paneId: "p1" };
		},
		async paneRoot() {
			reads++;
			// herdr 0.9.5 sometimes closes the connection without answering (0517).
			if (reads === 1) throw new Error("herdr closed the connection before answering pane.process_info");
			return child?.pid;
		},
		close() {
			calls.push("close");
		},
	};
	const terminal = herdrTerminal("bench-pty-test", {
		workspaceCwd: work,
		connect: async () => fake,
		closeTab: async (_session, tabId) => calls.push(`tab.close ${tabId}`),
	});
	const ended = await terminal(process.execPath, ["-e", "await Bun.sleep(1500)"], work, {}, 60_000);
	expect(ended.timedOut).toBe(false);
	// The turn lasted until the process exited, past the dropped read.
	expect(ended.wallMs).toBeGreaterThanOrEqual(1_400);
	expect(reads).toBe(2);
	expect(calls).toEqual([`workspace ${TRIAL_WORKSPACE} in ${work}`, "tab.close t1", "close"]);
	expect(() => herdrTerminal("jocasta")).toThrow("the trials' terminal session can't be jocasta");
}, 30_000);

test("a plan-review trial: the first turn in the terminal with the extension, plan mode on, the review turns in print mode", async () => {
	const root = tempDir();
	const fixture = join(root, "fixture");
	mkdirSync(fixture);
	writeFileSync(join(fixture, "lib.txt"), "x\n");
	const variants = join(root, "variants");
	mkdirSync(variants);
	writeFileSync(
		join(variants, "review.yaml"),
		'model: anthropic/claude-opus-5-5\nplan_review: execute\nfollowups: ["Check it"]\nsettings:\n  providers:\n    cacheWarming: streaming\n',
	);
	const evalCase: EvalCase = { id: "c1", dir: root, prompt: "Fix it", tags: [], checks: [], hiddenChecks: [], fixture };
	const out = join(root, "out");
	mkdirSync(join(out, "transcripts"), { recursive: true });
	const seen: { args: string[]; overlay: Record<string, unknown> }[] = [];
	const terminal: TerminalRunner = async (_command, args) => {
		const after = (flag: string) => args[args.indexOf(flag) + 1] as string;
		seen.push({ args, overlay: JSON.parse(readFileSync(after("--config"), "utf8")) });
		// omp interactive with the extension: its events, and the session file it writes.
		const { fire } = await loadExtension(after("-e"));
		approveAndExecute(fire, "execute");
		const sessions = after("--session-dir");
		mkdirSync(sessions, { recursive: true });
		const entries = [
			{ type: "session", id: "s1", timestamp: "2026-11-09T10:00:00.000Z" },
			{
				type: "message",
				timestamp: "2026-11-09T10:00:01.000Z",
				message: {
					role: "assistant",
					model: "claude-opus-5-5",
					content: [{ type: "toolCall", id: "c", name: "write", arguments: { path: "lib.txt" } }],
					usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { total: 0.01 } },
				},
			},
		];
		writeFileSync(join(sessions, "s1.jsonl"), `${entries.map((e) => JSON.stringify(e)).join("\n")}\n`);
		return { timedOut: false, wallMs: 5 };
	};
	const row = await runTrial(evalCase, 0, "omp/stub", {
		outDir: out,
		launcher: [process.execPath, join(import.meta.dir, "fixtures", "omp.ts")],
		variant: loadVariants(variants, ["review"]),
		runInTerminal: terminal,
		workDir: root,
	});
	const [first] = seen;
	expect(first?.args).not.toContain("-p");
	expect(first?.args.at(-1)).toBe("Fix it");
	expect(dirname(first?.args[first.args.indexOf("-e") + 1] ?? "")).toBe(
		dirname(first?.args[first.args.indexOf("--config") + 1] ?? ""),
	);
	expect(first?.overlay).toMatchObject({
		...PLAN_REVIEW_OVERLAY,
		providers: { cacheWarming: "streaming" },
		memory: { backend: "off" },
	});
	expect(row.planReview).toEqual({
		requested: true,
		choice: "execute",
		answered: true,
		resolved: "execute",
		shutdown: "the approved plan's execution ended",
	});
	expect(row.grade["omp exited 0"]).toBe(1);
	expect(row.prompts).toBe(2);
	// The first turn's usage from its session file, plus the review turn the fake omp appended to it (-c, --session-dir).
	expect(row.turns).toBe(2);
	expect(row.usage.input).toBe(1010);
	expect(row.findings).toEqual({ total: 0, blocking: 0 });
	expect(readFileSync(join(out, "transcripts", "c1-0.jsonl"), "utf8")).toContain('"id":"s1"');
}, 60_000);
