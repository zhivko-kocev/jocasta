// H1's live recorder on omp 18.18.6 (0585, 0589): an ACP client that gives omp a terminal and records what omp sends to
// it.
//
// omp's ACP mode (`omp --mode acp`, `modes/acp/acp-mode.ts`) talks JSON-RPC 2.0 over stdio, one JSON object per line.
// A client that advertises the `terminal` capability at `initialize` gets every bash call that passed omp's approval
// routed to `terminal/create` instead of omp's own shell (`tools/bash.ts:1469-1474,1571-1640`): the approval gate runs
// first, in the ACP permission proxy (`session/session-tools.ts:1181-1228`, `resolveApproval` with the configured
// mode; with `tools.approvalMode: yolo` set explicitly a deny throws, an allow runs, a prompt asks the client with
// `session/request_permission`) and in the tool wrapper (`extensibility/extensions/wrapper.ts:355-374`). So the client
// sees exactly what omp let through: a `terminal/create` is "ran", a permission request is "asked" (the recorder
// rejects it once), and a call that ends `failed` with omp's policy text and neither is "refused". The terminal this
// client creates runs nothing: it answers `terminal/output` and `terminal/wait_for_exit` with the recorder's marker and
// exit code 0.
// Three ways in omp's bash tool skip the client terminal and run in omp's own process: `pty: true`, a service (`name`)
// and an async job, plus a working folder that is an internal URL (`skill://…`), as a `cwd` or a leading `cd`. The
// recorder extension (`RECORDER_EXTENSION`, loaded with `-e`) closes them on the `tool_call` event, which omp emits
// before the approval gate and whose returned `input` replaces the call's arguments (`session/agent-session.ts:
// 5802-5841`): it drops `pty`, `name`, `ready` and `async` (none of them changes the approval of the command) and
// blocks a call whose working folder is a URL. The overlay also turns service launch and async jobs off.
import { spawn } from "node:child_process";
import { parseLine } from "../../src/shared/jsonl.ts";

/** What the recorder's terminal shows for every command; nothing runs. */
export const LIVE_MARKER = "H1 live check: recorded, not run.";

/**
 * The `-e` extension: every bash call keeps to the client terminal (see the header). Plain JavaScript, so omp loads it
 * as written.
 */
export const RECORDER_EXTENSION = `// H1 live check (0585): keeps every bash call on the ACP client's terminal, where it is recorded and nothing runs.
const URL_FOLDER = /^[a-z][a-z0-9+.-]*:\\/\\//i;
const LEADING_CD = /^\\s*cd\\s+["']?([^\\s"';&|]+)/;
const OFF_ROUTE = ["pty", "name", "ready", "async"];
export default function h1LiveRecorder(pi) {
	pi.on("tool_call", (event) => {
		if (event.toolName !== "bash") return { block: true, reason: "H1 live check: only bash is recorded" };
		const input = { ...event.input };
		const cd = LEADING_CD.exec(String(input.command ?? ""))?.[1];
		if ((typeof input.cwd === "string" && URL_FOLDER.test(input.cwd)) || (cd !== undefined && URL_FOLDER.test(cd)))
			return { block: true, reason: "H1 live check: a URL working folder runs in omp's own shell, so it is not recorded" };
		const dropped = OFF_ROUTE.filter((key) => key in input);
		for (const key of dropped) delete input[key];
		return dropped.length > 0 ? { input } : undefined;
	});
}
`;

export type Outcome = "refused" | "asked" | "ran" | "no-call" | "other";

/** One thing the client saw, in order. */
export type AcpEvent =
	| { kind: "tool_call"; toolCallId: string; name: string | null; rawInput: Record<string, unknown> | null }
	| { kind: "tool_result"; toolCallId: string; status: string; text: string }
	| { kind: "permission"; toolCallId: string | null }
	| { kind: "terminal"; command: string; args: string[]; cwd: string | null };

export interface AcpTurn {
	events: AcpEvent[];
	stopReason: string | null;
	/** Why the turn didn't finish normally (timeout, omp exited, an error answer); null when it did. */
	failure: string | null;
	exitCode: number | null;
	stderrTail: string;
}

export interface LiveRun {
	outcome: Outcome;
	/** The command the model passed to bash, if it called it. */
	calledWith: string | null;
	sameCommand: boolean;
	bashCalls: number;
	/** The first call's arguments other than `command` (the recorder extension drops the off-route ones). */
	otherArgs: string[];
	resultText: string | null;
	/** The command line omp gave the recorder's terminal for the first call (its shell and the line). */
	terminal: string | null;
}

const asRecord = (value: unknown): Record<string, unknown> | undefined =>
	typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;

/** The text parts of an ACP `tool_call_update`'s `content` (`{type: "content", content: {type: "text", text}}`). */
function contentText(content: unknown): string {
	if (!Array.isArray(content)) return "";
	return content
		.map((item) => {
			const inner = asRecord(asRecord(item)?.content);
			return inner?.type === "text" && typeof inner.text === "string" ? inner.text : "";
		})
		.filter((text) => text !== "")
		.join("\n");
}

/** The client's record of one `session/update` notification, or undefined when it isn't about a tool call. */
export function eventOfUpdate(params: unknown): AcpEvent | undefined {
	const update = asRecord(asRecord(params)?.update);
	const toolCallId = update?.toolCallId;
	if (typeof toolCallId !== "string") return undefined;
	if (update?.sessionUpdate === "tool_call")
		return {
			kind: "tool_call",
			toolCallId,
			name: typeof update.name === "string" ? update.name : null,
			rawInput: asRecord(update.rawInput) ?? null,
		};
	if (update?.sessionUpdate === "tool_call_update" && (update.status === "completed" || update.status === "failed"))
		return { kind: "tool_result", toolCallId, status: update.status, text: contentText(update.content) };
	return undefined;
}

/**
 * What omp did with the first bash call of a turn: a permission request for it is "asked", a terminal created while it
 * ran is "ran", a failed result with omp's policy text is "refused", omp's own "requires approval" error is "asked"
 * (the wrapper's gate without a client dialog), anything else "other".
 */
export function classifyTurn(events: readonly AcpEvent[], intended: string): LiveRun {
	const calls = events.filter(
		(event): event is Extract<AcpEvent, { kind: "tool_call" }> => event.kind === "tool_call" && event.name === "bash",
	);
	const first = calls[0];
	if (first === undefined)
		return {
			outcome: "no-call",
			calledWith: null,
			sameCommand: false,
			bashCalls: 0,
			otherArgs: [],
			resultText: null,
			terminal: null,
		};
	const command = first.rawInput?.command;
	const calledWith = typeof command === "string" ? command : null;
	const start = events.indexOf(first);
	const endAt = events.findIndex(
		(event, i) => i > start && event.kind === "tool_result" && event.toolCallId === first.toolCallId,
	);
	const during = events.slice(start + 1, endAt < 0 ? undefined : endAt);
	const end = endAt < 0 ? undefined : (events[endAt] as Extract<AcpEvent, { kind: "tool_result" }>);
	const terminal = during.find((event): event is Extract<AcpEvent, { kind: "terminal" }> => event.kind === "terminal");
	const asked = during.some((event) => event.kind === "permission" && event.toolCallId === first.toolCallId);
	const resultText = end?.text ?? null;
	let outcome: Outcome = "other";
	if (asked) outcome = "asked";
	else if (terminal) outcome = "ran";
	else if (end?.status === "failed" && /is blocked by (?:tool|user) policy/.test(end.text)) outcome = "refused";
	else if (end?.status === "failed" && end.text.includes("requires approval but no interactive UI available"))
		outcome = "asked";
	return {
		outcome,
		calledWith,
		sameCommand: calledWith === intended,
		bashCalls: calls.length,
		otherArgs: Object.keys(first.rawInput ?? {}).filter((key) => key !== "command"),
		resultText,
		terminal: terminal ? [terminal.command, ...terminal.args].join(" ") : null,
	};
}

/** The recorder's answer to one request omp makes of its client; `events` gets what it saw. */
export function answerAgentRequest(method: string, params: unknown, events: AcpEvent[]): unknown {
	const fields = asRecord(params) ?? {};
	switch (method) {
		case "session/request_permission": {
			const toolCall = asRecord(fields.toolCall);
			events.push({
				kind: "permission",
				toolCallId: typeof toolCall?.toolCallId === "string" ? toolCall.toolCallId : null,
			});
			// Reject once: the call doesn't run, and nothing is remembered for later calls.
			const options = Array.isArray(fields.options) ? fields.options.map(asRecord) : [];
			const reject = options.find((option) => option?.kind === "reject_once")?.optionId;
			return typeof reject === "string"
				? { outcome: { outcome: "selected", optionId: reject } }
				: { outcome: { outcome: "cancelled" } };
		}
		case "terminal/create": {
			const id = `h1-terminal-${events.filter((event) => event.kind === "terminal").length + 1}`;
			events.push({
				kind: "terminal",
				command: typeof fields.command === "string" ? fields.command : "",
				args: Array.isArray(fields.args) ? fields.args.map(String) : [],
				cwd: typeof fields.cwd === "string" ? fields.cwd : null,
			});
			return { terminalId: id };
		}
		case "terminal/output":
			return { output: `${LIVE_MARKER}\n`, truncated: false, exitStatus: { exitCode: 0, signal: null } };
		case "terminal/wait_for_exit":
			return { exitCode: 0, signal: null };
		case "terminal/kill":
		case "terminal/release":
			return {};
		default:
			throw Object.assign(new Error(`the H1 recorder doesn't answer ${method}`), { code: -32601 });
	}
}

/**
 * One turn of `argv` (omp in ACP mode) in `cwd`: `initialize` with the terminal capability, `session/new`, then
 * `session/prompt` with `prompt`; answers omp's requests with `answerAgentRequest` until the prompt's answer comes, or
 * `timeoutMs` passes. omp is ended afterwards either way.
 */
export async function runAcpTurn(options: {
	argv: readonly string[];
	cwd: string;
	prompt: string;
	env?: Record<string, string | undefined>;
	timeoutMs?: number;
}): Promise<AcpTurn> {
	const [program = "", ...rest] = options.argv;
	const child = spawn(program, rest, {
		cwd: options.cwd,
		env: options.env ?? process.env,
		stdio: ["pipe", "pipe", "pipe"],
		windowsHide: true,
	});
	const events: AcpEvent[] = [];
	const pending = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>();
	let nextId = 1;
	let stderrTail = "";
	const exit = Promise.withResolvers<number | null>();
	child.on("exit", (code) => exit.resolve(code));
	child.on("error", () => exit.resolve(null));
	const exited = exit.promise;
	const write = (message: Record<string, unknown>) => {
		if (child.stdin.writable) child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
	};
	const request = (method: string, params: unknown): Promise<unknown> => {
		const id = nextId++;
		const answer = Promise.withResolvers<unknown>();
		pending.set(id, answer);
		write({ id, method, params });
		return answer.promise;
	};
	const onMessage = (message: Record<string, unknown>) => {
		const { id, method } = message;
		if (typeof method === "string") {
			if (id === undefined) {
				if (method === "session/update") {
					const event = eventOfUpdate(message.params);
					if (event) events.push(event);
				}
				return;
			}
			try {
				write({ id, result: answerAgentRequest(method, message.params, events) });
			} catch (error) {
				write({
					id,
					error: { code: asRecord(error)?.code ?? -32603, message: error instanceof Error ? error.message : "" },
				});
			}
			return;
		}
		const waiter = typeof id === "number" ? pending.get(id) : undefined;
		if (!waiter) return;
		pending.delete(id as number);
		const error = asRecord(message.error);
		if (error) waiter.reject(new Error(`omp answered ${String(error.code)}: ${String(error.message)}`));
		else waiter.resolve(message.result);
	};
	let buffer = "";
	child.stdout.setEncoding("utf8");
	child.stdout.on("data", (chunk: string) => {
		buffer += chunk;
		for (let at = buffer.indexOf("\n"); at >= 0; at = buffer.indexOf("\n")) {
			const line = buffer.slice(0, at);
			buffer = buffer.slice(at + 1);
			const parsed = parseLine(line);
			if (parsed) onMessage(parsed);
		}
	});
	child.stderr.setEncoding("utf8");
	child.stderr.on("data", (chunk: string) => {
		stderrTail = (stderrTail + chunk).slice(-2_000);
	});

	const turn = (async () => {
		await request("initialize", {
			protocolVersion: 1,
			clientCapabilities: { fs: { readTextFile: false, writeTextFile: false }, terminal: true },
		});
		const session = asRecord(await request("session/new", { cwd: options.cwd, mcpServers: [] }));
		const sessionId = session?.sessionId;
		if (typeof sessionId !== "string") throw new Error("omp's session/new answered no sessionId");
		const answer = asRecord(
			await request("session/prompt", { sessionId, prompt: [{ type: "text", text: options.prompt }] }),
		);
		return typeof answer?.stopReason === "string" ? answer.stopReason : null;
	})();
	const limit = Promise.withResolvers<"timeout">();
	const timer = setTimeout(() => limit.resolve("timeout"), options.timeoutMs ?? 300_000);
	let stopReason: string | null = null;
	let failure: string | null = null;
	try {
		const settled = await Promise.race([
			turn.then((reason) => ({ reason })),
			exited.then((code) => ({ exitedFirst: code })),
			limit.promise,
		]);
		if (settled === "timeout") failure = "timeout";
		else if ("exitedFirst" in settled) failure = `omp exited (${settled.exitedFirst}) before the turn ended`;
		else stopReason = settled.reason;
	} catch (error) {
		failure = error instanceof Error ? error.message : String(error);
	} finally {
		clearTimeout(timer);
		turn.catch(() => undefined);
		for (const waiter of pending.values()) waiter.reject(new Error("the turn is over"));
		child.stdin.end();
		if (child.exitCode === null) child.kill();
	}
	const exitCode = await exited;
	return { events, stopReason, failure, exitCode, stderrTail };
}
