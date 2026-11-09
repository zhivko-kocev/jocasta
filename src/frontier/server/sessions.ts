/**
 * Reading omp's session files (F1 guide §5): `~/.omp/agent/sessions/<cwd dir>/<stem>.jsonl`; line 1 is a `title`
 * entry, the `session` header with `cwd` follows; assistant messages are `{type: "message", message: {role:
 * "assistant", content, model, usage: {cost: {total}}, timestamp}}`, written once when finished. Subagent and advisor
 * transcripts sit in the session's artifacts dir (`<stem>/<agentId>.jsonl`, `<stem>/__advisor.jsonl`, nested deeper
 * for a subagent's own): they are not sessions, but their cost is the session's (CR-04; `omp stats` counts them the
 * same way, `@oh-my-pi/omp-stats` `parser.ts` `classifyAgentType`, `listSessionFiles`).
 *
 * omp 18.12.6's entry kinds (0464): `model_usage` entries (side calls: memory extraction, cache warming, advisor,
 * judges; `session-entries.ts` `ModelUsageEntry`) count toward spend like assistant turns (0452); an assistant turn
 * marked `retryRecovery` (an attempt a retry superseded, `turn-recovery.ts`) still costs, but never gives the last
 * line, the question or the test result; `memory_recall` entries are ignored; a session moved to a sibling file (its
 * header's `parentSession` names the old one, `session-manager.ts` `#moveOffSessionFile`) is followed there
 * (`SessionIndex.moved`), and the entries it copied from the old file aren't counted twice.
 *
 * A session is a tree (0509): each entry names its `parentId`, and a rewind (`session-manager.ts` `branch`,
 * `branchWithSummary`) leaves the abandoned turns in the file. Cost, spend and message times count every entry in file
 * order, as abandoned turns were billed. The state (model, last line, question, plan, test result, bash runs, handoff
 * time) follows the active branch as omp replays it: each entry's `activeLeafId`, else its id, is the leaf
 * (`session-entries.ts` `sessionEntryLeafId`; an unknown one falls back to the newest entry selecting itself,
 * `lastSelfSelectedLeafId`), and the branch is the walk back from the leaf by `parentId` (`getBranch`). An entry
 * extending the leaf applies at once, an off-branch entry (the leaf unchanged) changes nothing, and any other leaf move
 * replays the branch from its root; each entry's parent and state effect are kept for that. Entries without an id or a
 * `parentId` apply in file order.
 *
 * Files are read incrementally (CR-03): each keeps its byte offset and running stats, and only appended lines are
 * parsed (`omp-session.ts` `SessionTail`). A file checked under a second ago isn't looked at again, and an active
 * session's artifacts dir is re-listed only when the session changed or every 30 s (PM-6, PM-17).
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, join, relative, sep } from "node:path";
import { dayStart, localDay } from "../../shared/day.ts";
import { asObject } from "../../shared/fsx.ts";
import { jsonLines } from "../../shared/jsonl.ts";
import { forgetStaleSessions } from "../../shared/ledger.ts";
import {
	readHeader,
	type SessionHeader,
	SessionTail,
	SessionWalker,
	type TailVisitor,
	transcriptsOf,
} from "../../shared/omp-session.ts";
import { ompAgentDir } from "../../shared/paths.ts";

/** omp's sessions dir: `<omp agent dir>/sessions` (guide §5, `docs/config-usage.md:80`). */
function ompSessionsDir(): string {
	return join(ompAgentDir(), "sessions");
}

/** One session file with its header. */
type SessionFile = SessionHeader & { mtimeMs: number };

/**
 * One bash call of a session and how it ended (0453): exit 0, a failure, or still running in the background; `exit` its
 * exit code (omp's `details.exitCode` on a failed call), null when the result doesn't say.
 */
export interface BashRun {
	command: string;
	/** When the call was made, epoch ms. */
	at: number;
	outcome: "ok" | "fail" | "background";
	exit: number | null;
}

/** Bash runs a session's stats keep: the latest, enough for one review turn. */
const BASH_RUNS_KEPT = 200;

/** What the service reads from one session: for the sidebar, the rail, the HUD and the worklog. */
export interface SessionStats {
	/** The last assistant message's `provider/model`. */
	model: string | null;
	/** Summed `usage.cost.total` of every assistant message, USD, its subagents' and advisors' included. */
	cost: number;
	/** Each assistant message's cost by local day (`YYYY-MM-DD`), its subagents' and advisors' included. */
	costByDay: Map<string, number>;
	/** The last assistant message's text, first line. */
	lastLine: string | null;
	/** The question of an `ask` call in the last assistant message: what a blocked omp waits on. */
	question: string | null;
	/** Every message's time, epoch ms, in file order. */
	messageTimes: number[];
	/**
	 * The title of a plan waiting for his approval in omp's Plan Review, or null. omp records the proposal as a
	 * successful `write` to `xd://propose` (a `toolResult` whose `details.xdev` is `{tool: "propose", mode: "execute",
	 * inner: {title, planFilePath}}`; `event-controller.ts:2196-2235`), then aborts the turn silently (an empty
	 * assistant message). Approving leaves plan mode (`mode_change` to `none`, `interactive-mode.ts:5146`); refining
	 * sends a user message; either, or any later assistant output, ends the wait.
	 */
	planAwaiting: string | null;
	/** Whether the session is in plan mode: its last `mode_change` was to `plan` (omp writes one on entering it). */
	planMode: boolean;
	/** The file of the last plan proposed in the session (`inner.planFilePath`), kept after approval: the reviewer reads it (0453). */
	planFile: string | null;
	/** How often he sent a waiting plan back (Plan Review's "Refine plan" sends his message while it waits, 0471). */
	planSentBack: number;
	/**
	 * The last `TESTS: pass|fail|none — <command>` line of an assistant message (the review turn's last line, 0425,
	 * 0453), the command it names, and that message's time; null when none.
	 */
	tests: { result: Exclude<TestsResult, "unknown">; at: number; command: string | null } | null;
	/** The latest bash calls with their outcome (0453), oldest first. */
	bashRuns: BashRun[];
	/** Bash calls waiting for their result, by tool call id. */
	pendingBash: Map<string, { command: string; at: number }>;
	/** The time of the last `/handoff` compaction entry (`method: "handoff"`), epoch ms; null when none. */
	handoffAt: number | null;
	/** The context the last assistant message left (0174): its input, cached and output tokens; null before one. */
	context: number | null;
}

/**
 * A review's test result (0425, 0453): the review turn's test command ran and exited 0 (`pass`), it failed or the
 * turn said it failed (`fail`), the project has none (`none`); `unknown` when no run backs the turn's claim
 * ("tests unverified").
 */
export type TestsResult = "pass" | "fail" | "none" | "unknown";

/** The line `reviewPrompt` asks the review turn to end with: `TESTS: pass — <command>`. */
const TESTS_LINE = /^[\s*_`>-]*TESTS:\s*(pass|fail|none)\b[`*_]*(?:[ \t]*[—–:-]+[ \t]*([^\n]+))?/gim;

/** A session written to this recently may still have subagents or an advisor writing: its artifacts dir is re-listed. */
const ACTIVE_MS = 10 * 60_000;

/** How often an unchanged active session's artifacts dir is listed again. */
const RELIST_MS = 30_000;

/** A file checked this recently is answered from what was read (PM-6). */
const CHECK_MS = 1_000;

/** The ids of every entry in `file` (the session a moved one copied its transcript from). */
function entryIds(file: string): Set<string> {
	const ids = new Set<string>();
	const collect: TailVisitor = {
		entry: ({ id }) => {
			if (typeof id === "string") ids.add(id);
		},
		reset: () => ids.clear(),
	};
	new SessionTail(file, collect).advance();
	return ids;
}

/** The plan title of an `xd://propose` dispatch result, or undefined for any other tool result. */
function proposedPlan(message: Record<string, unknown>): string | undefined {
	if (message.toolName !== "write" || message.isError === true) return undefined;
	const xdev = asObject(asObject(message.details)?.xdev);
	if (xdev?.tool !== "propose" || xdev.mode !== "execute") return undefined;
	const title = asObject(xdev.inner)?.title ?? asObject(xdev.args)?.title;
	return typeof title === "string" && title.trim() ? title.trim() : "plan";
}

const emptyStats = (): SessionStats => ({
	model: null,
	cost: 0,
	costByDay: new Map(),
	lastLine: null,
	question: null,
	messageTimes: [],
	planAwaiting: null,
	planMode: false,
	planFile: null,
	planSentBack: 0,
	tests: null,
	bashRuns: [],
	pendingBash: new Map(),
	handoffAt: null,
	context: null,
});

/** A plan proposal's file (`inner.planFilePath`), or undefined. */
function proposedPlanFile(message: Record<string, unknown>): string | undefined {
	const file = asObject(asObject(asObject(message.details)?.xdev)?.inner)?.planFilePath;
	return typeof file === "string" && file.trim() ? file.trim() : undefined;
}

/** A time from a message's own `timestamp`, else the entry's ISO time; NaN when neither. */
function entryTime(entry: Record<string, unknown>, message?: Record<string, unknown>): number {
	// omp stats' rule (`coerceEntryTimestamp`): a stored zero means "no time"; the entry's ISO time is next.
	if (typeof message?.timestamp === "number" && message.timestamp > 0) return message.timestamp;
	return typeof entry.timestamp === "string" ? Date.parse(entry.timestamp) : Number.NaN;
}

/** A model call's `usage.cost.total`, or undefined when it has none. */
function costOf(usage: unknown): number | undefined {
	const cost = asObject(asObject(usage)?.cost)?.total;
	return typeof cost === "number" && Number.isFinite(cost) ? cost : undefined;
}

/** The tokens a model call's context holds once answered (`usage.totalTokens`, else its parts summed); undefined when none. */
function contextOf(usage: unknown): number | undefined {
	const tokens = asObject(usage);
	if (!tokens) return undefined;
	if (typeof tokens.totalTokens === "number" && tokens.totalTokens > 0) return tokens.totalTokens;
	let sum = 0;
	for (const key of ["input", "output", "cacheRead", "cacheWrite"]) {
		const value = tokens[key];
		if (typeof value === "number" && Number.isFinite(value)) sum += value;
	}
	return sum > 0 ? sum : undefined;
}

/**
 * Counts one model call's cost (when it has one): into the stats' total and its day unless the entry is a copy of
 * one the old file holds, and into the day's ledger always (the ledger knows a copy).
 */
function addCost(
	stats: SessionStats,
	entry: Record<string, unknown>,
	usage: unknown,
	at: number,
	copied: boolean,
	ledger?: SpendLedger,
): void {
	const cost = costOf(usage);
	if (cost === undefined) return;
	ledger?.add(entry, usage, at, cost);
	if (copied) return;
	stats.cost += cost;
	if (Number.isFinite(at)) {
		const day = localDay(at);
		stats.costByDay.set(day, (stats.costByDay.get(day) ?? 0) + cost);
	}
}

/**
 * Every model call's cost by local day, each counted once wherever it was written (0433, 0452, 0464). A call is known
 * by its entry's ISO `timestamp` and its usage (tokens and cost), which its copies keep (a moved session's sibling, a
 * file rewritten through a rename, a moved session's artifacts) and so does omp's re-journal of a turn it discards
 * durably (an off-branch `model_usage` under a new id with the turn's timestamp, SRV-B-2). So a copy never counts
 * twice, and a call stays counted after the file holding it was rewritten without it, moved or deleted: the day's
 * spend never drops within the day. The day is the message's own time. Calls before `since` aren't kept.
 */
class SpendLedger {
	private since = Number.NEGATIVE_INFINITY;
	private readonly days = new Map<string, { keys: Set<string>; total: number }>();

	add(entry: Record<string, unknown>, usage: unknown, at: number, cost: number): void {
		if (!Number.isFinite(at) || at < this.since) return;
		const u = asObject(usage);
		const tokens = [u?.input, u?.output, u?.cacheRead, u?.cacheWrite].map((n) => (typeof n === "number" ? n : 0));
		// An entry without an ISO time (older transcripts): its id, if any, and its time stand for it.
		const stamp =
			typeof entry.timestamp === "string" ? entry.timestamp : `${typeof entry.id === "string" ? entry.id : ""}|${at}`;
		const key = `${stamp}|${tokens.join("|")}|${cost}`;
		const day = localDay(at);
		let spent = this.days.get(day);
		if (!spent) {
			spent = { keys: new Set(), total: 0 };
			this.days.set(day, spent);
		}
		if (spent.keys.has(key)) return;
		spent.keys.add(key);
		spent.total += cost;
	}

	total(day: string): number {
		return this.days.get(day)?.total ?? 0;
	}

	/** Drops the days before `since`'s and keeps no call older than it from now on. */
	forgetBefore(since: number): void {
		this.since = since;
		const first = localDay(since);
		for (const day of this.days.keys()) {
			if (day < first) this.days.delete(day);
		}
	}
}

/** The test result and command of a `TESTS:` line in `text`, the last one; undefined when there is none. */
function testsLine(text: string): { result: Exclude<TestsResult, "unknown">; command: string | null } | undefined {
	const match = [...text.matchAll(TESTS_LINE)].at(-1);
	const result = match?.[1]?.toLowerCase();
	if (result !== "pass" && result !== "fail" && result !== "none") return undefined;
	const command = match?.[2]?.replace(/^[\s`*_]+|[\s`*_]+$/g, "");
	return { result, command: command ? detached(command) : null };
}

/**
 * A copy of `part` that holds no reference to the text it was cut from (PERF-4): in JavaScriptCore a substring keeps
 * its whole source alive, and the state keeps a first line for every assistant message of a followed session.
 */
function detached(part: string): string {
	return Buffer.from(part, "utf8").toString("utf8");
}

/** A command with its whitespace runs made one space, so a named command matches the one that ran. */
const squash = (command: string) => command.replace(/\s+/g, " ").trim();

/**
 * Whether a run's exit code is the named command's own (0453): nothing after the command's last occurrence may take
 * its place, as a pipe does (omp's shell runs without pipefail: `bun test | tail` exits as `tail` does), or `||`, `;`,
 * a background `&` or a next line. Quoted text, redirections (`2>&1`, `> log`), `&&` and commands before it (`cd x &&`)
 * leave it the command's.
 */
function ownExit(command: string, named: string): boolean {
	const flat = squash(command.replace(/\\\r?\n/g, " ").replace(/\r?\n/g, " ; "));
	const at = flat.lastIndexOf(named);
	if (at < 0) return false;
	const rest = flat
		.slice(at + named.length)
		.replace(/"(?:[^"\\]|\\.)*"|'[^']*'/g, " ")
		.replace(/\d*>>?&(?:\d+|-)/g, " ")
		.replace(/&>>?/g, " ")
		.replace(/&&/g, " ");
	return !/[|;&]/.test(rest);
}

/**
 * What a review turn's tests showed (0453): its `TESTS:` line is believed only when the command it names ran in the
 * session at or after `since` and that run's exit code agrees. `pass` needs the named command's last run to exit 0
 * with the exit code its own (`ownExit`); a failed run, or the turn saying `fail`, is `fail`; `none` is the project
 * having no tests; no line, no command, no such run, one still running in the background, or one whose exit code a
 * later command gave is `unknown` (unverified).
 */
export function checkedTests(stats: Pick<SessionStats, "tests" | "bashRuns">, since: number): TestsResult {
	const reported = stats.tests && stats.tests.at >= since ? stats.tests : null;
	if (!reported) return "unknown";
	if (reported.result === "none") return "none";
	if (!reported.command) return reported.result === "fail" ? "fail" : "unknown";
	const named = squash(reported.command);
	const run = stats.bashRuns.findLast((r) => r.at >= since && squash(r.command).includes(named));
	if (!run) return reported.result === "fail" ? "fail" : "unknown";
	if (run.outcome === "fail" || reported.result === "fail") return "fail";
	return run.outcome === "ok" && ownExit(run.command, named) ? "pass" : "unknown";
}

/**
 * Each check's exit code in the review turn (0453, 0540): the last run at or after `since` of a bash call holding the
 * command, its exit code when that code is the command's own (`ownExit`: not piped or followed by another), else null,
 * as for a check that didn't run or still runs in the background.
 */
export function checkExits(
	stats: Pick<SessionStats, "bashRuns">,
	commands: readonly string[],
	since: number,
): { command: string; exit: number | null }[] {
	return commands.map((command) => {
		const named = squash(command);
		const run = stats.bashRuns.findLast((r) => r.at >= since && squash(r.command).includes(named));
		const exit = run && run.outcome !== "background" && ownExit(run.command, named) ? run.exit : null;
		return { command, exit };
	});
}

/** What one entry does to the active branch's state (0509): only what the state needs, no message texts. */
interface Effect {
	/** A `mode_change`'s mode. */
	mode?: string;
	/** A `/handoff` compaction's time. */
	handoffAt?: number;
	/** An `xd://propose` result: the plan's title and file. */
	plan?: { title: string; file: string | undefined };
	/** A bash call's result: its tool call id, outcome and exit code. */
	result?: { callId: string; outcome: BashRun["outcome"]; exit: number | null };
	/** A user message; `synthetic` when omp wrote it. */
	user?: { synthetic: boolean };
	/** An assistant message. */
	assistant?: {
		/** It has content: it ends a plan's wait. */
		said: boolean;
		model: string | undefined;
		/** A superseded retry (0464): the model only. */
		retry: boolean;
		firstLine: string | undefined;
		tests: SessionStats["tests"] | undefined;
		question: string | null;
		bash: { id: string; command: string; at: number }[];
		context: number | undefined;
	};
}

/** An entry's state effect, or undefined when it has none. */
function effectOf(entry: Record<string, unknown>): Effect | undefined {
	if (entry.type === "mode_change") return { mode: String(entry.mode) };
	if (entry.type === "compaction" && entry.method === "handoff") {
		const at = typeof entry.timestamp === "string" ? Date.parse(entry.timestamp) : Number.NaN;
		return Number.isFinite(at) ? { handoffAt: at } : undefined;
	}
	if (entry.type !== "message") return undefined;
	const message = asObject(entry.message);
	if (!message) return undefined;
	if (message.role === "toolResult") {
		const effect: Effect = {};
		const title = proposedPlan(message);
		if (title) effect.plan = { title, file: proposedPlanFile(message) };
		if (typeof message.toolCallId === "string") {
			const details = asObject(message.details);
			const outcome = asObject(details?.async) ? "background" : message.isError === true ? "fail" : "ok";
			const code = details?.exitCode;
			const exit = outcome === "ok" ? 0 : outcome === "fail" && typeof code === "number" ? code : null;
			effect.result = { callId: message.toolCallId, outcome, exit };
		}
		return effect.plan || effect.result ? effect : undefined;
	}
	if (message.role === "user") return { user: { synthetic: message.synthetic === true } };
	if (message.role !== "assistant") return undefined;
	const at = entryTime(entry, message);
	const model =
		typeof message.model !== "string"
			? undefined
			: typeof message.provider === "string"
				? `${message.provider}/${message.model}`
				: message.model;
	const said = Array.isArray(message.content) && message.content.length > 0;
	// An attempt a retry superseded: billed, but not what the thread said or asked (0464).
	if (message.retryRecovery) {
		return {
			assistant: {
				said,
				model,
				retry: true,
				firstLine: undefined,
				tests: undefined,
				question: null,
				bash: [],
				context: undefined,
			},
		};
	}
	let text = "";
	let question: string | null = null;
	const bash: { id: string; command: string; at: number }[] = [];
	for (const value of Array.isArray(message.content) ? message.content : []) {
		const part = asObject(value);
		if (part?.type === "text" && typeof part.text === "string") text += part.text;
		if (part?.type === "toolCall" && part.name === "ask") {
			const first = asObject((asObject(part.arguments)?.questions as unknown[] | undefined)?.[0]);
			if (typeof first?.question === "string") question = first.question;
		}
		if (part?.type === "toolCall" && part.name === "bash" && typeof part.id === "string") {
			const command = asObject(part.arguments)?.command;
			if (typeof command === "string") bash.push({ id: part.id, command, at: Number.isFinite(at) ? at : 0 });
		}
	}
	const reported = testsLine(text);
	const firstLine = text.trim().split("\n")[0]?.trim();
	return {
		assistant: {
			said,
			model,
			retry: false,
			firstLine: firstLine ? detached(firstLine) : undefined,
			tests: reported ? { ...reported, at: Number.isFinite(at) ? at : 0 } : undefined,
			question,
			bash,
			context: contextOf(message.usage),
		},
	};
}

/** Applies one entry's effect to the state fields of `stats`. */
function applyEffect(stats: SessionStats, effect: Effect): void {
	if (effect.mode !== undefined) {
		stats.planMode = effect.mode === "plan";
		if (effect.mode !== "plan") stats.planAwaiting = null;
	}
	if (effect.handoffAt !== undefined) stats.handoffAt = effect.handoffAt;
	if (effect.plan) {
		stats.planAwaiting = effect.plan.title;
		stats.planFile = effect.plan.file ?? stats.planFile;
	}
	if (effect.result) {
		const call = stats.pendingBash.get(effect.result.callId);
		if (call) {
			stats.pendingBash.delete(effect.result.callId);
			stats.bashRuns.push({ ...call, outcome: effect.result.outcome, exit: effect.result.exit });
			if (stats.bashRuns.length > BASH_RUNS_KEPT) stats.bashRuns.splice(0, stats.bashRuns.length - BASH_RUNS_KEPT);
		}
	}
	if (effect.user) {
		if (stats.planAwaiting && !effect.user.synthetic) stats.planSentBack++;
		stats.planAwaiting = null;
	}
	const said = effect.assistant;
	if (!said) return;
	if (said.said) stats.planAwaiting = null;
	if (said.model !== undefined) stats.model = said.model;
	if (said.retry) return;
	for (const { id, command, at } of said.bash) stats.pendingBash.set(id, { command, at });
	if (said.firstLine) stats.lastLine = said.firstLine;
	if (said.tests) stats.tests = said.tests;
	if (said.context !== undefined) stats.context = said.context;
	stats.question = said.question;
}

/** The state fields of `stats` set back to a session's start. */
function resetState(stats: SessionStats): void {
	stats.model = null;
	stats.lastLine = null;
	stats.question = null;
	stats.planAwaiting = null;
	stats.planMode = false;
	stats.planFile = null;
	stats.planSentBack = 0;
	stats.tests = null;
	stats.bashRuns = [];
	stats.pendingBash = new Map();
	stats.handoffAt = null;
	stats.context = null;
}

/** omp's session tree as far as the state needs it (0509): each entry's parent and effect, and the active leaf. */
interface Branches {
	nodes: Map<string, { parent: string | null; effect: Effect | undefined }>;
	/** The active leaf the state was built for; null before the first entry or after a leaf reset. */
	leaf: string | null;
	/** The newest entry that selected itself: the leaf when one names an unknown entry. */
	lastSelf: string | null;
}

const newBranches = (): Branches => ({ nodes: new Map(), leaf: null, lastSelf: null });

/** The state fields of `stats` rebuilt for the branch from `branches.leaf` back to its root. */
function replayBranch(stats: SessionStats, branches: Branches): void {
	const effects: Effect[] = [];
	const visited = new Set<string>();
	for (let id = branches.leaf; id !== null && !visited.has(id); ) {
		visited.add(id);
		const node = branches.nodes.get(id);
		if (!node) break;
		if (node.effect) effects.push(node.effect);
		id = node.parent;
	}
	resetState(stats);
	for (let i = effects.length - 1; i >= 0; i--) applyEffect(stats, effects[i] as Effect);
}

/**
 * Folds one session entry into `stats`; `seen` holds entry ids already counted in the file this one moved from;
 * `ledger` counts the day's spend across files. Cost and times count in file order; the state follows the active
 * branch (`branches`), an entry without an id or a `parentId` applying in file order. `costOnly` (a nested transcript,
 * whose cost alone the session uses, SRV-B-11) counts the cost and keeps no times, branches or state.
 */
function addEntry(
	stats: SessionStats,
	branches: Branches,
	entry: Record<string, unknown>,
	seen?: Set<string>,
	ledger?: SpendLedger,
	costOnly = false,
): void {
	// A copy of an entry the old file holds: its cost and time are counted there.
	const copied = seen !== undefined && typeof entry.id === "string" && seen.has(entry.id);
	if (entry.type === "model_usage") addCost(stats, entry, entry.usage, entryTime(entry), copied, ledger);
	const message = entry.type === "message" ? asObject(entry.message) : undefined;
	if (message) {
		const at = entryTime(entry, message);
		if (Number.isFinite(at) && !copied && !costOnly) stats.messageTimes.push(at);
		if (message.role === "assistant") addCost(stats, entry, message.usage, at, copied, ledger);
	}
	if (costOnly) return;
	const effect = effectOf(entry);
	if (typeof entry.id !== "string" || !("parentId" in entry)) {
		if (effect) applyEffect(stats, effect);
		return;
	}
	const { id } = entry;
	const parent = typeof entry.parentId === "string" ? entry.parentId : null;
	branches.nodes.set(id, { parent, effect });
	// omp's replay (`session-entries.ts` `sessionEntryLeafId`, `lastSelfSelectedLeafId`).
	const selected = entry.activeLeafId === null || typeof entry.activeLeafId === "string" ? entry.activeLeafId : id;
	if (selected === id) branches.lastSelf = id;
	const leaf = selected === null || branches.nodes.has(selected) ? selected : branches.lastSelf;
	if (leaf === branches.leaf) return; // off the branch: the state stands
	const grows = leaf === id && parent === branches.leaf;
	branches.leaf = leaf;
	if (grows) {
		if (effect) applyEffect(stats, effect);
	} else {
		replayBranch(stats, branches);
	}
}

/**
 * Stats of a session file's text, read whole. Tests use it as the oracle the incremental reader (`SessionIndex.read`)
 * must match.
 */
export function readStats(text: string): SessionStats {
	const stats = emptyStats();
	const branches = newBranches();
	for (const entry of jsonLines(text)) addEntry(stats, branches, entry);
	return stats;
}

/** One transcript followed incrementally (`SessionTail`, CR-03) and the stats read of it so far. */
class Tail implements TailVisitor {
	readonly reader: SessionTail;
	stats = emptyStats();
	/** The session tree read so far, which the state follows (0509). */
	branches = newBranches();
	/** For a session moved to this file: the ids of the old file's entries, whose cost and times count there. */
	seen?: Set<string>;

	constructor(
		file: string,
		private readonly ledger: SpendLedger,
		/** A nested transcript: only its cost is read (`addEntry`'s `costOnly`). */
		private readonly costOnly = false,
	) {
		this.reader = new SessionTail(file, this);
	}

	entry(entry: Record<string, unknown>): void {
		addEntry(this.stats, this.branches, entry, this.seen, this.ledger, this.costOnly);
	}

	/** A rewritten file (omp's rewrite through a rename, possibly larger) is read again from its start. */
	reset(): void {
		this.stats = emptyStats();
		this.branches = newBranches();
	}
}

/** A session's own transcript, the nested ones found in its artifacts dir, and their stats combined. */
interface Followed {
	main: Tail;
	nested: Map<string, Tail>;
	/** The main transcript's `mtimeMs:size` when its artifacts dir was last listed. */
	listedAt: string;
	/** When the artifacts dir was last listed, by the index's clock. */
	listedTime: number;
	/** When `read` last looked at the files, by the index's clock. */
	checkedAt: number;
	/** When a nested transcript last grew, by the index's clock: its folder is re-listed while it works (SRV-B-19). */
	nestedGrewAt: number;
	combined: SessionStats;
}

/** Bytes parsed before `warm` lets the event loop run. */
const WARM_STEP = 1 << 20;

/** A session's stats with its nested transcripts' cost added. */
function combine(followed: Followed): SessionStats {
	const main = followed.main.stats;
	if (followed.nested.size === 0) return main;
	const costByDay = new Map(main.costByDay);
	let cost = main.cost;
	for (const { stats } of followed.nested.values()) {
		cost += stats.cost;
		for (const [day, value] of stats.costByDay) costByDay.set(day, (costByDay.get(day) ?? 0) + value);
	}
	return { ...main, cost, costByDay };
}

/**
 * Session files under `dir` modified at or after `sinceMs`, with their headers, and each session's stats read
 * incrementally. Headers are cached by path, as a session's cwd never changes. At the first call of a new local day,
 * message times before yesterday are dropped, and sessions not written since yesterday's midnight and files that are
 * gone are forgotten (PM-17, PERF-4). A day before yesterday is read through a throwaway index (`timesOn`, SRV-B-4).
 */
export class SessionIndex {
	private readonly headers = new Map<string, SessionHeader | null>();
	private readonly followed = new Map<string, Followed>();
	/** The local day of the last call. */
	private day: string | undefined;
	/** The spend of today and yesterday, across every file read (0464): `spentOn`. */
	private readonly ledger = new SpendLedger();
	/** A day before yesterday asked for (the worklog on request): read whole by its own index, dropped at rollover. */
	private past: { day: string; index: SessionIndex } | undefined;
	/** The session files under `dir`, cwd dirs unchanged since the last list answered from it (SRV-B-6). */
	private readonly walker: SessionWalker;

	constructor(
		readonly dir: string = ompSessionsDir(),
		private readonly now: () => number = Date.now,
	) {
		this.walker = new SessionWalker(dir);
	}

	list(sinceMs = 0): SessionFile[] {
		this.rollOver();
		const found: SessionFile[] = [];
		for (const file of this.walker.files()) {
			try {
				const mtimeMs = statSync(file).mtimeMs;
				if (mtimeMs < sinceMs) continue;
				let header = this.headers.get(file);
				if (header === undefined) {
					header = readHeader(file) ?? null;
					// No header yet: read again next time, once omp wrote it.
					if (header) this.headers.set(file, header);
				}
				if (header) found.push({ ...header, mtimeMs });
			} catch {
				// removed since the listing
			}
		}
		return found;
	}

	/**
	 * What model calls cost on `day` (local, `YYYY-MM-DD`; today or yesterday) in the files read so far, subagents and
	 * advisors included, each call once however many files hold it; a call stays counted after its file changed or went.
	 */
	spentOn(day: string): number {
		this.rollOver();
		return this.ledger.total(day);
	}

	/**
	 * A session's stats, its subagents' and advisors' cost included; only lines appended since the last call are
	 * parsed. A missing file has empty stats. A file checked within a second is answered from what was read, unless
	 * `fresh`: a state change is judged on what the file holds now.
	 */
	read(file: string, fresh = false): SessionStats {
		const now = this.now();
		this.rollOver(now);
		let followed = this.followed.get(file);
		if (!followed) {
			if (!existsSync(file)) return emptyStats();
			followed = this.newFollowed(file);
		}
		if (!fresh && now - followed.checkedAt < CHECK_MS) return followed.combined;
		followed.checkedAt = now;
		let changed = followed.main.reader.advance();
		// The known subagent and advisor transcripts are followed on every read (one stat each), so a long `task` call
		// that leaves the session untouched still shows its spend as it goes (SRV-B-19).
		for (const tail of followed.nested.values()) {
			if (tail.reader.advance()) {
				changed = true;
				followed.nestedGrewAt = now;
			}
		}
		const main = followed.main.reader;
		// The artifacts dir is listed again when the session changed, or every `RELIST_MS` while it or a transcript in
		// it is active (a subagent or the advisor may start after the session's last line).
		const key = `${main.mtimeMs}:${main.size}`;
		const active = now - main.mtimeMs < ACTIVE_MS || now - followed.nestedGrewAt < ACTIVE_MS;
		if (key !== followed.listedAt || (active && now - followed.listedTime >= RELIST_MS)) {
			followed.listedAt = key;
			followed.listedTime = now;
			for (const nested of transcriptsOf(file)) {
				if (followed.nested.has(nested)) continue;
				if (this.nestedTail(file, followed, nested).reader.advance()) {
					changed = true;
					followed.nestedGrewAt = now;
				}
			}
		}
		if (changed) followed.combined = combine(followed);
		return followed.combined;
	}

	/** Whether `file` was read already, so a `read` of it parses only what was appended. */
	follows(file: string): boolean {
		return this.followed.has(file);
	}

	/**
	 * The message times of session `file` that a worklog of `day` needs (SRV-B-4). Today's and yesterday's come from
	 * the followed session; a day before yesterday's, whose times this index dropped at rollover, from a throwaway
	 * index kept for that day (`prepare` reads it in steps first).
	 */
	timesOn(file: string, day: string): readonly number[] {
		return this.indexFor(day).read(file).messageTimes;
	}

	/**
	 * Reads the sessions that may hold `day`'s messages (written since its start) in steps, letting the event loop run,
	 * so `timesOn` for that day then only parses what was appended. A day before yesterday goes to its throwaway index.
	 */
	prepare(day: string, stopped: () => boolean = () => false): Promise<void> {
		const index = this.indexFor(day);
		return index.warm(
			index.list(dayStart(day)).map((s) => s.file),
			stopped,
		);
	}

	/** This index for today and yesterday; for an older day, a fresh one (its times kept whole), one day at a time. */
	private indexFor(day: string): SessionIndex {
		this.rollOver();
		if (day >= localDay(dayStart(this.day as string, -1))) return this;
		if (this.past?.day !== day) this.past = { day, index: new SessionIndex(this.dir, this.now) };
		return this.past.index;
	}

	/**
	 * Reads `files` (sessions with their nested transcripts) the way `read` does, about 1 MB at a time with the event
	 * loop let run between steps, so a later `read` only parses what was appended. Stops early when `stopped` says so.
	 */
	async warm(files: string[], stopped: () => boolean = () => false): Promise<void> {
		this.rollOver();
		for (const file of new Set(files)) {
			if (stopped()) return;
			if (!existsSync(file)) continue;
			const followed = this.followed.get(file) ?? this.newFollowed(file);
			await followed.main.reader.advanceInSteps(WARM_STEP, stopped);
			for (const nested of transcriptsOf(file)) {
				await this.nestedTail(file, followed, nested).reader.advanceInSteps(WARM_STEP, stopped);
			}
			followed.listedAt = `${followed.main.reader.mtimeMs}:${followed.main.reader.size}`;
			followed.listedTime = this.now();
			followed.combined = combine(followed);
		}
	}

	/**
	 * Where a session went on (0464): when omp moved it to a sibling file (another process wrote the old one) or
	 * started one from it, the sibling's header names it as `parentSession` (by id; a fork names the file); the newest
	 * such sibling is followed, and on from there. `file` itself when nothing moved.
	 */
	moved(file: string): string {
		let current = file;
		for (let hops = 0; hops < 8; hops++) {
			const children = this.children(dirname(current));
			const id = this.header(current)?.id;
			const next = (id ? children.get(id) : undefined) ?? children.get(current);
			if (!next || next === current) break;
			current = next;
		}
		return current;
	}

	/**
	 * A thread's session files (0538, 0546): `file` and the others in its folder (omp keeps one per working folder) whose
	 * session began at or after `since` (epoch ms), oldest first. omp starts a new session on "Approve and execute"
	 * (`/clear`, no `parentSession`), so following `moved` alone loses the earlier ones.
	 */
	sessionsSince(file: string, since: number): string[] {
		const dir = dirname(file);
		const own = join(dir, basename(file));
		let names: string[] = [];
		try {
			names = readdirSync(dir).filter((name) => name.endsWith(".jsonl"));
		} catch {
			// gone
		}
		const found = new Map<string, number>([[own, this.header(own)?.started ?? Number.POSITIVE_INFINITY]]);
		for (const name of names) {
			const path = join(dir, name);
			const started = this.header(path)?.started;
			if (started !== undefined && started >= since && !found.has(path)) found.set(path, started);
		}
		return [...found].sort((a, b) => a[1] - b[1]).map(([path]) => path);
	}

	/** A file's header, cached once it has one. */
	private header(file: string): SessionHeader | undefined {
		let header = this.headers.get(file);
		if (header === undefined) {
			try {
				header = readHeader(file) ?? null;
			} catch {
				return undefined; // gone
			}
			if (header) this.headers.set(file, header);
		}
		return header ?? undefined;
	}

	/** Per cwd dir (re-read when the dir changed): each `parentSession` id → its newest child file. */
	private readonly childrenByDir = new Map<string, { mtimeMs: number; byParent: Map<string, string> }>();

	private children(dir: string): Map<string, string> {
		const mtimeMs = statSync(dir, { throwIfNoEntry: false })?.mtimeMs ?? -1;
		const known = this.childrenByDir.get(dir);
		if (known?.mtimeMs === mtimeMs) return known.byParent;
		const newest = new Map<string, { file: string; mtimeMs: number }>();
		let names: string[] = [];
		try {
			names = readdirSync(dir).filter((name) => name.endsWith(".jsonl"));
		} catch {
			// gone
		}
		for (const name of names) {
			const file = join(dir, name);
			const parent = this.header(file)?.parentSession;
			if (!parent) continue;
			const at = statSync(file, { throwIfNoEntry: false })?.mtimeMs ?? 0;
			if ((newest.get(parent)?.mtimeMs ?? -1) < at) newest.set(parent, { file, mtimeMs: at });
		}
		const byParent = new Map([...newest].map(([parent, { file }]) => [parent, file]));
		this.childrenByDir.set(dir, { mtimeMs, byParent });
		return byParent;
	}

	/** The file of session `id` in `dir`, or undefined. */
	private sessionFile(dir: string, id: string): string | undefined {
		let names: string[] = [];
		try {
			names = readdirSync(dir).filter((name) => name.endsWith(".jsonl"));
		} catch {
			return undefined;
		}
		// omp names a session `<time>_<id>.jsonl`; a header is read only when no name says so.
		const named = names.find((name) => basename(name, ".jsonl").endsWith(`_${id}`));
		if (named) return join(dir, named);
		return names.map((name) => join(dir, name)).find((file) => this.header(file)?.id === id);
	}

	/**
	 * A followed session, new. One moved from or started off another (`parentSession`) skips the entries it holds that
	 * are copies of the old file's, so their cost and times aren't counted twice (0452, 0464).
	 */
	private newFollowed(file: string): Followed {
		const main = new Tail(file, this.ledger);
		const parent = this.parentFile(file);
		if (parent) main.seen = entryIds(parent);
		const followed: Followed = {
			main,
			nested: new Map(),
			listedAt: "",
			listedTime: 0,
			checkedAt: Number.NEGATIVE_INFINITY,
			nestedGrewAt: Number.NEGATIVE_INFINITY,
			combined: emptyStats(),
		};
		this.followed.set(file, followed);
		return followed;
	}

	/** The file `file`'s session moved from or forked off: `parentSession` is an id, or (a fork) the file itself. */
	private parentFile(file: string): string | undefined {
		const parent = this.header(file)?.parentSession;
		if (!parent) return undefined;
		if (parent.endsWith(".jsonl")) return existsSync(parent) ? parent : undefined;
		return this.sessionFile(dirname(file), parent);
	}

	/** A nested transcript's tail; one omp copied along with a moved session's artifacts skips the copied entries. */
	private nestedTail(file: string, followed: Followed, nested: string): Tail {
		let tail = followed.nested.get(nested);
		if (!tail) {
			tail = new Tail(nested, this.ledger, true);
			const parent = followed.main.seen ? this.parentFile(file) : undefined;
			if (parent) {
				const twin = join(parent.replace(/\.jsonl$/, ""), relative(file.replace(/\.jsonl$/, ""), nested));
				if (existsSync(twin)) tail.seen = entryIds(twin);
			}
			followed.nested.set(nested, tail);
		}
		return tail;
	}

	/**
	 * At the first call of a new local day: message times and spent calls before yesterday's midnight are dropped;
	 * sessions not written since then (no transcript of theirs either) and files that no longer exist are forgotten, so
	 * the index holds today's and yesterday's sessions only (PERF-4, SRV-B-11). A forgotten session read again later is
	 * read from its start; its calls are in the ledger already, so none counts twice. An older day's index goes too.
	 */
	private rollOver(now = this.now()): void {
		const day = localDay(now);
		if (this.day === day) return;
		const first = this.day === undefined;
		this.day = day;
		const since = dayStart(day, -1);
		this.ledger.forgetBefore(since);
		this.past = undefined;
		if (first) return;
		for (const [file, followed] of this.followed) {
			const tails = [followed.main, ...followed.nested.values()];
			if (!existsSync(file) || tails.every((tail) => tail.reader.mtimeMs < since)) {
				this.followed.delete(file);
				continue;
			}
			for (const { stats } of tails) {
				stats.messageTimes = stats.messageTimes.filter((at) => at >= since);
			}
			followed.combined = combine(followed);
		}
		for (const file of this.headers.keys()) {
			if (!existsSync(file)) this.headers.delete(file);
		}
		// The thread ledger's parsed files go the same way, except those of a session still followed (SRV-B-11).
		const kept = [...this.followed.keys()].map((file) => file.replace(/\.jsonl$/, ""));
		forgetStaleSessions(since, (file) => kept.some((stem) => file === `${stem}.jsonl` || file.startsWith(stem + sep)));
	}
}
