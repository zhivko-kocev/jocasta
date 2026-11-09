import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { removeTree } from "../src/shared/fsx.ts";
import { jsonLines } from "../src/shared/jsonl.ts";
import type { Harness } from "./variants.ts";

/**
 * The harnesses a trial runs (0386, 0482 E4, 0537), one adapter each (EVL-14): how a turn's arguments are built, which
 * of a variant's omp keys it takes, where its records are and how they're saved, and how its transcript reads (usage,
 * assistant text, the error it ended on). The runner and the task benchmark ask the adapter instead of branching on
 * the harness's name.
 */

/**
 * Where A7's Anthropic API key waits (0537): the only API key of the run, given to opencode's process as
 * `ANTHROPIC_API_KEY` and removed from every other process a trial starts (and an `ANTHROPIC_API_KEY` of the bench's
 * own environment from every trial).
 */
export const A7_API_KEY_ENV = "JOCASTA_A7_ANTHROPIC_API_KEY";

export interface Usage {
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite: number;
	totalTokens: number;
	cost: number;
}

/** What a trial's transcript says about its model, usage, turns and tool calls. */
export interface HarnessResponse {
	model: string | null;
	usage: Usage;
	totalUsage: Usage;
	requests: number;
	turns: number;
	toolCalls: number;
}

/** One turn as the runner builds its arguments (see {@link HarnessAdapter.args}). */
export interface Turn {
	prompt: string;
	followUp: boolean;
	overlay: string;
	profile?: string;
	sessionDir?: string;
	session?: string;
	extension?: string;
}

/** What {@link saveSessions} needs of a finished trial. */
export interface TrialRecords {
	transcript: string;
	sessionDir?: string;
	launcher: readonly string[];
	env: Record<string, string | undefined>;
	cwd: string;
	scrub?: (text: string) => string;
}

/** The variant keys omp alone takes unless a harness lists them in {@link HarnessAdapter.accepts}, in report order. */
export const OMP_KEYS = [
	"plan_first",
	"plan_review",
	"configs",
	"extensions",
	"tools",
	"settings",
	"layer",
	"append_system_prompt",
] as const;

export type OmpKey = (typeof OMP_KEYS)[number];

export interface HarnessAdapter {
	/** The keys of {@link OMP_KEYS} this harness takes too; a variant setting another one for it is refused. */
	accepts: readonly OmpKey[];
	/** One turn's arguments after the launcher, `flags` being the variant's (`variantArgs`). */
	args(flags: string[], turn: Turn): string[];
	/** Writes its session files to the trial's `--session-dir`; can answer omp's interactive plan review (0567). */
	sessionFiles: boolean;
	/** The variable whose value is its Anthropic API key (A7, 0537), given to it as `ANTHROPIC_API_KEY`. */
	apiKeyEnv?: string;
	/** The session a follow-up continues, from the turns' output so far; undefined when it continues the last. */
	session(output: string): string | undefined;
	/** The trial-root folder its records are saved to when the run keeps none: its usage is read from them. */
	recordsDir?: string;
	/** Copies or exports the trial's records into `dir` (emptied first by {@link saveSessions}). */
	saveSessions(dir: string, trial: TrialRecords): void;
	/** The trial's usage: from its transcript, or from its records (`savedSessions`, the session files of a plan review). */
	response(trial: {
		transcript: string;
		savedSessions?: string;
		sessionDir?: string;
		planReview: boolean;
	}): HarnessResponse;
	/** The assistant's text in one turn's transcript, joined by newlines. */
	assistantText(transcript: string): string;
	/** The error its own records say the trial ended on, or "" (EVL-2; see `endSignal` in tasks.ts). */
	endSignal(transcript: string): string;
}

const emptyUsage = (): Usage => ({ input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 });

/** omp's usage shape: Jocasta's, with `cost` an object whose `total` is the dollars. */
type OmpUsage = Omit<Usage, "cost"> & { cost?: { total?: number } };

function addUsage(target: Usage, used: Partial<OmpUsage> | undefined): void {
	if (!used) return;
	target.input += used.input ?? 0;
	target.output += used.output ?? 0;
	target.cacheRead += used.cacheRead ?? 0;
	target.cacheWrite += used.cacheWrite ?? 0;
	target.totalTokens += used.totalTokens ?? 0;
	target.cost += used.cost?.total ?? 0;
}

/** The text blocks of a message's `content`. */
function textBlocks(content: unknown): string[] {
	if (!Array.isArray(content)) return [];
	const texts: string[] = [];
	for (const block of content as { type?: unknown; text?: unknown }[])
		if (block?.type === "text" && typeof block.text === "string") texts.push(block.text);
	return texts;
}

/**
 * From omp's `--mode json` events: the model as the responses name it, the main session's usage summed over its
 * assistant `message_end` events, those turns and their `toolCall` blocks, and `totalUsage`/`requests` with the
 * foreground subagents of each `task` call added from its `tool_execution_end` details (`usage`, `results[].requests`).
 */
export function readResponse(transcript: string): HarnessResponse {
	const usage = emptyUsage();
	const subagentUsage = emptyUsage();
	let subagentRequests = 0;
	let model: string | null = null;
	let turns = 0;
	let toolCalls = 0;
	// A line torn by a kill is skipped (DUP-1): a timed-out trial is graded as such, not thrown as infra.
	for (const event of jsonLines<{
		type?: string;
		toolName?: string;
		result?: { details?: { usage?: OmpUsage; results?: { requests?: number }[] } };
		message?: { role?: string; model?: string; content?: unknown; usage?: OmpUsage };
	}>(transcript)) {
		if (event.type === "tool_execution_end" && event.toolName === "task") {
			const details = event.result?.details;
			addUsage(subagentUsage, details?.usage);
			for (const result of details?.results ?? []) subagentRequests += result.requests ?? 0;
			continue;
		}
		if (event.type !== "message_end" || event.message?.role !== "assistant") continue;
		turns++;
		const { content } = event.message;
		if (Array.isArray(content))
			toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "toolCall").length;
		model = event.message.model ?? model;
		addUsage(usage, event.message.usage);
	}
	const totalUsage = { ...usage };
	for (const key of Object.keys(totalUsage) as (keyof Usage)[]) totalUsage[key] += subagentUsage[key];
	return { model, usage, totalUsage, requests: turns + subagentRequests, turns, toolCalls };
}

/** The `.jsonl` files under omp's session folder, the folder's own (the trial's sessions) first, then subagents'. */
function ompSessionFiles(dir: string): { path: string; main: boolean }[] {
	if (!existsSync(dir)) return [];
	return readdirSync(dir, { recursive: true, encoding: "utf8" })
		.filter((name) => name.endsWith(".jsonl"))
		.map((name) => ({ path: join(dir, name), main: !/[\\/]/.test(name) }))
		.sort((a, b) => Number(b.main) - Number(a.main) || a.path.localeCompare(b.path));
}

/** The trial's own omp session files' text, joined: a plan-review trial's transcript (0567). */
export function ompSessionText(dir: string): string {
	return ompSessionFiles(dir)
		.filter((file) => file.main)
		.map((file) => {
			const text = readFileSync(file.path, "utf8");
			return text === "" || text.endsWith("\n") ? text : `${text}\n`;
		})
		.join("");
}

/**
 * From omp's session files in `dir` (a plan-review trial, whose first turn had a terminal, not JSON): as
 * {@link readResponse} counts the JSON stream, the trial's own assistant messages give the model, `usage`, the turns
 * and their `toolCall` blocks; the subagents' files beside them add to `totalUsage` and `requests`.
 */
export function readOmpSessionsResponse(dir: string): HarnessResponse {
	const usage = emptyUsage();
	const subagentUsage = emptyUsage();
	let subagentRequests = 0;
	let model: string | null = null;
	let turns = 0;
	let toolCalls = 0;
	for (const file of ompSessionFiles(dir)) {
		for (const entry of jsonLines<{
			type?: string;
			message?: { role?: string; model?: string; content?: unknown; usage?: OmpUsage };
		}>(readFileSync(file.path, "utf8"))) {
			if (entry.type !== "message" || entry.message?.role !== "assistant") continue;
			if (!file.main) {
				subagentRequests++;
				addUsage(subagentUsage, entry.message.usage);
				continue;
			}
			turns++;
			const { content } = entry.message;
			if (Array.isArray(content))
				toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "toolCall").length;
			model = entry.message.model ?? model;
			addUsage(usage, entry.message.usage);
		}
	}
	const totalUsage = { ...usage };
	for (const key of Object.keys(totalUsage) as (keyof Usage)[]) totalUsage[key] += subagentUsage[key];
	return { model, usage, totalUsage, requests: turns + subagentRequests, turns, toolCalls };
}

/** Claude Code's usage shape in its stream-json `result` events. */
interface ClaudeUsage {
	input_tokens?: number;
	output_tokens?: number;
	cache_read_input_tokens?: number;
	cache_creation_input_tokens?: number;
}

/**
 * From Claude Code's `-p --output-format stream-json --verbose` events (one `result` event per invocation, with
 * `total_cost_usd` and `usage`; `assistant` events carry `message.model` and `tool_use` blocks): usage summed over the
 * results, turns from `num_turns`, tool calls from the `tool_use` blocks. A run killed at the limit has no `result`
 * (EVL-3): its turns are then its distinct assistant messages, and its usage is the ledger's, from the saved
 * transcript. Claude Code's subagents are inside its own totals, so `totalUsage` equals `usage`.
 */
export function readClaudeResponse(transcript: string): HarnessResponse {
	const usage = emptyUsage();
	let model: string | null = null;
	let turns = 0;
	let results = 0;
	let toolCalls = 0;
	// Claude Code streams one `assistant` event per content block, each with its message's id.
	const messages = new Set<string>();
	for (const event of jsonLines<{
		type?: string;
		uuid?: string;
		model?: string;
		num_turns?: number;
		total_cost_usd?: number;
		usage?: ClaudeUsage;
		message?: { id?: string; model?: string; content?: unknown };
	}>(transcript)) {
		if (event.type === "assistant") {
			messages.add(event.message?.id ?? event.uuid ?? `#${messages.size}`);
			model = event.message?.model ?? model;
			const content = event.message?.content;
			if (Array.isArray(content))
				toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "tool_use").length;
		} else if (event.type === "result") {
			results++;
			const used = event.usage ?? {};
			usage.input += used.input_tokens ?? 0;
			usage.output += used.output_tokens ?? 0;
			usage.cacheRead += used.cache_read_input_tokens ?? 0;
			usage.cacheWrite += used.cache_creation_input_tokens ?? 0;
			usage.cost += event.total_cost_usd ?? 0;
			turns += event.num_turns ?? 0;
		}
	}
	if (results === 0) turns = messages.size;
	usage.totalTokens = usage.input + usage.output + usage.cacheRead + usage.cacheWrite;
	return { model, usage, totalUsage: { ...usage }, requests: turns, turns, toolCalls };
}

/**
 * From opencode's exports in `dir` (`opencode export` JSON, one per session): the model, each `step-finish` part's
 * tokens and cost summed (reasoning counted as output), assistant messages as turns and steps as requests, `tool`
 * parts as tool calls. The cost is opencode's own: on A7's API key, the bill.
 */
export function readOpencodeResponse(dir: string): HarnessResponse {
	const usage = emptyUsage();
	let model: string | null = null;
	let turns = 0;
	let requests = 0;
	let toolCalls = 0;
	const files = existsSync(dir) ? readdirSync(dir).filter((name) => name.endsWith(".json")) : [];
	for (const name of files) {
		let exported: { messages?: { info?: { role?: string; modelID?: string }; parts?: Record<string, unknown>[] }[] };
		try {
			exported = JSON.parse(readFileSync(join(dir, name), "utf8"));
		} catch {
			process.stderr.write(`opencode export ${name} isn't JSON; its usage is left out\n`);
			continue;
		}
		for (const message of exported.messages ?? []) {
			if (message.info?.role !== "assistant") continue;
			turns++;
			model = message.info.modelID ?? model;
			for (const part of message.parts ?? []) {
				if (part.type === "tool") toolCalls++;
				if (part.type !== "step-finish") continue;
				requests++;
				const tokens = (part.tokens ?? {}) as {
					input?: number;
					output?: number;
					reasoning?: number;
					cache?: { read?: number; write?: number };
				};
				usage.input += tokens.input ?? 0;
				usage.output += (tokens.output ?? 0) + (tokens.reasoning ?? 0);
				usage.cacheRead += tokens.cache?.read ?? 0;
				usage.cacheWrite += tokens.cache?.write ?? 0;
				usage.cost += typeof part.cost === "number" ? part.cost : 0;
			}
		}
	}
	usage.totalTokens = usage.input + usage.output + usage.cacheRead + usage.cacheWrite;
	return { model, usage, totalUsage: { ...usage }, requests, turns, toolCalls };
}

/** The opencode sessions a `--format json` transcript names (`sessionID`), in first-seen order. */
export function opencodeSessions(transcript: string): string[] {
	return [...new Set([...transcript.matchAll(/"sessionID"\s*:\s*"(ses_[A-Za-z0-9]+)"/g)].map((m) => m[1] as string))];
}

/** The Claude Code sessions a stream-json transcript names (`session_id`), in first-seen order. */
export function claudeSessions(transcript: string): string[] {
	return [...new Set([...transcript.matchAll(/"session_id"\s*:\s*"([0-9a-f-]{36})"/g)].map((m) => m[1] as string))];
}

/**
 * omp (0386): `[--profile <p>] [-c] <variant flags> [--session-dir <dir>] --config <overlay> -p <prompt> --mode json`;
 * a follow-up continues the trial's session (`-c`). An interactive turn (`extension` set: the plan-review extension,
 * 0567) has no `-p`/`--mode`: `<flags> [--session-dir <dir>] --config <overlay> -e <extension> <prompt>`. Its records
 * are the `--session-dir` folder as it is.
 */
const omp: HarnessAdapter = {
	accepts: OMP_KEYS,
	args(flags, turn) {
		const common = [
			...(turn.profile === undefined ? [] : ["--profile", turn.profile]),
			...(turn.followUp ? ["-c"] : []),
			...flags,
			...(turn.sessionDir === undefined ? [] : ["--session-dir", turn.sessionDir]),
			"--config",
			turn.overlay,
		];
		if (turn.extension !== undefined) return [...common, "-e", turn.extension, turn.prompt];
		return [...common, "-p", turn.prompt, "--mode", "json"];
	},
	sessionFiles: true,
	session: () => undefined,
	saveSessions(dir, trial) {
		if (trial.sessionDir !== undefined && existsSync(trial.sessionDir))
			cpSync(trial.sessionDir, dir, { recursive: true });
	},
	response: (trial) =>
		trial.planReview ? readOmpSessionsResponse(trial.sessionDir as string) : readResponse(trial.transcript),
	assistantText(transcript) {
		const texts: string[] = [];
		for (const event of jsonLines<{ type?: string; message?: { role?: string; content?: unknown } }>(transcript))
			if (event.type === "message_end" && event.message?.role === "assistant")
				texts.push(...textBlocks(event.message.content));
		return texts.join("\n");
	},
	// omp's last assistant message (a `--mode json` `message_end`, or a session file's `message`) when its
	// `stopReason` is `error`, with its `errorMessage`.
	endSignal(transcript) {
		let last: { stopReason?: unknown; errorMessage?: unknown } | undefined;
		for (const event of jsonLines<{
			type?: string;
			message?: { role?: string; stopReason?: unknown; errorMessage?: unknown };
		}>(transcript))
			if ((event.type === "message_end" || event.type === "message") && event.message?.role === "assistant")
				last = event.message;
		if (last?.stopReason !== "error") return "";
		return typeof last.errorMessage === "string" ? last.errorMessage : "error";
	},
};

/**
 * Claude Code (0482 E4): `[-c] <variant flags> -p <prompt> --output-format stream-json --verbose` (stream-json needs
 * `--verbose` in print mode). Its records are `<session>.jsonl` and `<session>/subagents/*.jsonl` from
 * `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*`.
 */
const claude: HarnessAdapter = {
	accepts: ["append_system_prompt"],
	args: (flags, turn) => [
		...(turn.followUp ? ["-c"] : []),
		...flags,
		"-p",
		turn.prompt,
		"--output-format",
		"stream-json",
		"--verbose",
	],
	sessionFiles: false,
	session: () => undefined,
	saveSessions(dir, trial) {
		const projects = join(trial.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude"), "projects");
		const folders = existsSync(projects) ? readdirSync(projects).map((name) => join(projects, name)) : [];
		for (const id of claudeSessions(trial.transcript)) {
			const folder = folders.find((path) => existsSync(join(path, `${id}.jsonl`)));
			if (folder === undefined) {
				process.stderr.write(`no Claude Code transcript for session ${id} under ${projects}\n`);
				continue;
			}
			cpSync(join(folder, `${id}.jsonl`), join(dir, `${id}.jsonl`));
			const subagents = join(folder, id, "subagents");
			if (existsSync(subagents)) cpSync(subagents, join(dir, id, "subagents"), { recursive: true });
		}
	},
	response: (trial) => readClaudeResponse(trial.transcript),
	assistantText(transcript) {
		const texts: string[] = [];
		for (const event of jsonLines<{ type?: string; message?: { content?: unknown } }>(transcript))
			if (event.type === "assistant") texts.push(...textBlocks(event.message?.content));
		return texts.join("\n");
	},
	// Its `result` with `is_error` (its text and `api_error_status`) and a `rate_limit_event` whose status isn't
	// `allowed`.
	endSignal(transcript) {
		const said: string[] = [];
		for (const event of jsonLines<{
			type?: string;
			is_error?: boolean;
			result?: unknown;
			api_error_status?: unknown;
			rate_limit_info?: { status?: unknown };
		}>(transcript)) {
			if (event.type === "result" && event.is_error === true)
				said.push(
					["result error", event.api_error_status, typeof event.result === "string" ? event.result : ""]
						.filter((part) => part !== undefined && part !== null && part !== "")
						.join(" "),
				);
			const status = event.rate_limit_info?.status;
			if (event.type === "rate_limit_event" && status !== "allowed") said.push(`rate limit ${String(status)}`);
		}
		return said.join("\n");
	},
};

/**
 * opencode (A7, 0537): `run [--session <id>] <variant flags> --format json <prompt>`; a follow-up names the first
 * turn's session, since `--continue` takes the machine's last session, which may be another trial's. Its records are
 * `opencode export <session>` of each session into `<session>.json`, through `scrub` (A7's key out, EVL-13), and its
 * usage is read from them.
 */
const opencode: HarnessAdapter = {
	accepts: [],
	args(flags, turn) {
		if (turn.followUp && turn.session === undefined)
			throw new Error("an opencode follow-up needs the first turn's session");
		return [
			"run",
			...(turn.followUp ? ["--session", turn.session as string] : []),
			...flags,
			"--format",
			"json",
			turn.prompt,
		];
	},
	sessionFiles: false,
	apiKeyEnv: A7_API_KEY_ENV,
	session: (output) => opencodeSessions(output)[0],
	recordsDir: "opencode-sessions",
	saveSessions(dir, trial) {
		const [command = "", ...launcherArgs] = trial.launcher;
		for (const id of opencodeSessions(trial.transcript)) {
			const exported = spawnSync(command, [...launcherArgs, "export", id], {
				cwd: trial.cwd,
				env: trial.env,
				encoding: "utf8",
				windowsHide: true,
				maxBuffer: 256 * 1024 * 1024,
				timeout: 120_000,
			});
			if (exported.status !== 0 || !exported.stdout.trimStart().startsWith("{")) {
				process.stderr.write(`opencode export ${id} failed: ${exported.stderr || exported.error?.message}\n`);
				continue;
			}
			writeFileSync(join(dir, `${id}.json`), trial.scrub ? trial.scrub(exported.stdout) : exported.stdout);
		}
	},
	response: (trial) => readOpencodeResponse(trial.savedSessions as string),
	assistantText(transcript) {
		const texts: string[] = [];
		for (const event of jsonLines<{ part?: { type?: unknown; text?: unknown } }>(transcript))
			if (event.part?.type === "text" && typeof event.part.text === "string") texts.push(event.part.text);
		return texts.join("\n");
	},
	// Its `error` events.
	endSignal(transcript) {
		const said: string[] = [];
		for (const event of jsonLines<{ type?: string; error?: unknown }>(transcript))
			if (event.type === "error") said.push(JSON.stringify(event.error ?? event));
		return said.join("\n");
	},
};

export const HARNESSES: Readonly<Record<Harness, HarnessAdapter>> = { omp, claude, opencode };

/**
 * The assistant's text in one turn's transcript, joined by newlines: omp's assistant `message_end` text blocks, Claude
 * Code's `assistant` text blocks, opencode's `text` parts.
 */
export function assistantText(transcript: string, harness: Harness): string {
	return HARNESSES[harness].assistantText(transcript);
}

/**
 * Saves the trial's records into `dir` (emptied first) for the ledger (0538), as the harness keeps them (see each
 * adapter). A session that can't be found or exported is reported on stderr; the ledger then lacks it.
 */
export function saveSessions(harness: Harness, dir: string, trial: TrialRecords): void {
	removeTree(dir);
	mkdirSync(dir, { recursive: true });
	HARNESSES[harness].saveSessions(dir, trial);
}
