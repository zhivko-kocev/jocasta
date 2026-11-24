// The one extension every Jocasta pane's omp loads (0675, 0684): it speaks JSON lines to the service's bridge pipe
// (`JOCASTA_BRIDGE`). Tools come from the service's `welcome`; state goes up as events; prompts and control come down.
// This file and the omp argv in `pane_launch.rs` are the only omp-specific code in Jocasta. No pipe name: it does nothing.
import net from "node:net";

const RETRY_MS = 1000;
const WELCOME_WAIT_MS = 3000;
const ACK_WAIT_MS = 2000;
const UNREACHABLE = "Jocasta's service isn't reachable; try again in a moment.";

interface Tool {
	name: string;
	label?: string;
	description: string;
	parameters: object;
}
interface Msg {
	t?: string;
	id?: string;
	ok?: boolean;
	error?: unknown;
	text?: unknown;
	context?: unknown;
	tools?: Tool[];
	deliver?: string;
	instructions?: unknown;
	[field: string]: unknown;
}
type Json = Record<string, unknown>;
interface Task {
	status?: string;
}
interface Details {
	phases?: { tasks?: Task[] }[];
}
interface Ev {
	toolName?: string;
	result?: { details?: Details };
	details?: Details;
	stop_hook_active?: boolean;
	signal?: AbortSignal;
}
interface Ctx {
	abort?(): void;
	shutdown?(): void;
	isIdle?(): boolean;
	getContextUsage?(): { tokens?: number; percent?: number } | undefined;
	getAsyncJobSnapshot?(): unknown;
	compact?(options: object): unknown;
	openPlanReview?(): unknown;
	sessionManager?: { getSessionFile?(): string | null };
	version?: string;
}
interface Pi {
	version?: string;
	registerTool(tool: object): void;
	sendUserMessage(text: string, options?: object): unknown;
	on(name: string, handler: (event: Ev, context: Ctx) => unknown): void;
}

// biome-ignore lint/complexity/noExcessiveLinesPerFunction: one closure holds the connection and its handlers
export default async function bridge(pi: Pi): Promise<void> {
	const address = process.env.JOCASTA_BRIDGE;
	if (!address) return;
	const key = process.env.JOCASTA_BOSS_KEY || process.env.JOCASTA_PANE_KEY || "";
	let socket: net.Socket | undefined;
	let up = false;
	let ctx: Ctx | undefined;
	let sequence = 0;
	const waiting = new Map<string, (message: Msg) => void>();
	const registered = new Set<string>();
	let welcomed: (() => void) | undefined;
	let stopped = false;

	const send = (message: Json): boolean => {
		if (!up || !socket) return false;
		socket.write(`${JSON.stringify(message)}\n`);
		return true;
	};
	const request = (message: Json, signal?: AbortSignal): Promise<Msg> =>
		new Promise<Msg>((resolve, reject) => {
			const id = String(++sequence);
			if (!send({ ...message, id })) return reject(new Error(UNREACHABLE));
			waiting.set(id, resolve);
			signal?.addEventListener(
				"abort",
				() => {
					waiting.delete(id);
					send({ t: "cancel", id });
					reject(new Error("aborted"));
				},
				{ once: true },
			);
		});

	const registerTool = (tool: Tool): void => {
		if (registered.has(tool.name)) return;
		registered.add(tool.name);
		pi.registerTool({
			name: tool.name,
			label: tool.label ?? tool.name,
			description: tool.description,
			parameters: tool.parameters,
			async execute(_id: string, args: Json, signal?: AbortSignal) {
				const answer = await request({ t: "call", tool: tool.name, args }, signal);
				if (answer.ok === false) throw new Error(String(answer.error ?? "failed"));
				return { content: [{ type: "text", text: String(answer.text ?? "") }], details: {} };
			},
		});
	};

	const answerService = async (message: Msg): Promise<void> => {
		switch (message.t) {
			case "welcome":
				for (const tool of message.tools ?? []) registerTool(tool);
				welcomed?.();
				return;
			case "result":
			case "stop_result": {
				const done = waiting.get(String(message.id));
				if (done) {
					waiting.delete(String(message.id));
					done(message);
				}
				return;
			}
			case "prompt":
				return deliver(message);
			case "compact":
				return compact(message);
			case "abort":
				return void ctx?.abort?.();
			case "shutdown":
				return void ctx?.shutdown?.();
			default:
				return;
		}
	};

	const admitted = async (id: string, work: () => unknown): Promise<void> => {
		try {
			const started = Promise.resolve(work());
			await Promise.race([started, new Promise((resolve) => setTimeout(resolve, ACK_WAIT_MS))]);
			send({ t: "ack", id });
			started.catch(() => {});
		} catch (error) {
			send({ t: "nack", id, error: String((error as Error)?.message ?? error) });
		}
	};
	const deliver = async (message: Msg): Promise<void> => {
		const id = message.id ?? "";
		const text = String(message.text ?? "");
		if (text === "/plan-review" && typeof ctx?.openPlanReview === "function") {
			return admitted(id, () => ctx?.openPlanReview?.());
		}
		const busy = ctx && typeof ctx.isIdle === "function" ? !ctx.isIdle() : false;
		// omp's sendUserMessage takes steer, followUp or aside; `nextTurn` waits for the turn to end, as followUp does
		// (no options would steer into the running turn).
		const as = message.deliver === "steer" ? "steer" : "followUp";
		const options = busy ? { deliverAs: as } : undefined;
		return admitted(id, () => pi.sendUserMessage(text, options));
	};
	const compact = async (message: Msg): Promise<void> => {
		const id = message.id ?? "";
		if (typeof ctx?.compact !== "function") return void send({ t: "nack", id, error: "omp can't compact" });
		return admitted(id, () => ctx?.compact?.({ mode: "handoff", internalGuidance: message.instructions }));
	};

	const connect = (): void => {
		if (stopped) return;
		const next = net.connect(address);
		let buffer = "";
		socket = next;
		next.setEncoding("utf8");
		next.on("connect", () => {
			up = true;
			const caps = typeof pi.sendUserMessage === "function" ? ["prompt"] : [];
			const sessionFile = ctx?.sessionManager?.getSessionFile?.() ?? null;
			send({
				t: "hello",
				v: 1,
				key,
				pid: process.pid,
				session: sessionFile,
				harness: "omp",
				harness_version: pi.version ?? ctx?.version ?? "unknown",
				caps,
			});
			if (ctx) sendCaps();
		});
		next.on("data", (chunk: string) => {
			buffer += chunk;
			for (let end = buffer.indexOf("\n"); end >= 0; end = buffer.indexOf("\n")) {
				const line = buffer.slice(0, end);
				buffer = buffer.slice(end + 1);
				try {
					void answerService(JSON.parse(line));
				} catch {
					// not JSON: ignore
				}
			}
		});
		const lost = (): void => {
			if (socket !== next) return;
			up = false;
			for (const [, done] of waiting) done({ ok: false, error: UNREACHABLE });
			waiting.clear();
			if (!stopped) setTimeout(connect, RETRY_MS).unref?.();
		};
		next.on("close", lost);
		next.on("error", () => {});
	};

	// The session file omp runs now: the hello goes out before omp's `session_start`, so it is null there, and a handoff or
	// a new session changes it later; the service keeps the latest to bring a sleeping pane back to it (0676).
	const session = (): string | null => {
		try {
			return ctx?.sessionManager?.getSessionFile?.() ?? null;
		} catch {
			return null;
		}
	};
	const sendCaps = (): void => {
		const caps: string[] = [];
		if (typeof ctx?.abort === "function") caps.push("abort");
		if (typeof ctx?.shutdown === "function") caps.push("shutdown");
		if (typeof ctx?.getContextUsage === "function") caps.push("usage");
		if (typeof ctx?.getAsyncJobSnapshot === "function") caps.push("jobs");
		if (typeof ctx?.compact === "function") caps.push("compact");
		send({ t: "event", name: "caps", caps, session: session() });
	};
	const event = (name: string, fields: Json = {}): void => void send({ t: "event", name, ...fields });
	const jobs = (): number => {
		try {
			const snapshot = ctx?.getAsyncJobSnapshot?.() as { running?: unknown[] } | unknown[] | undefined;
			const list = Array.isArray(snapshot) ? snapshot : snapshot?.running;
			return Array.isArray(list) ? list.length : 0;
		} catch {
			return 0;
		}
	};

	connect();
	await new Promise<void>((resolve) => {
		welcomed = resolve;
		setTimeout(resolve, WELCOME_WAIT_MS).unref?.();
	});

	pi.on("session_start", (_event: Ev, context: Ctx) => {
		ctx = context;
		sendCaps();
	});
	pi.on("agent_start", (_event: Ev, context: Ctx) => {
		ctx = context;
		event("agent_start");
	});
	pi.on("agent_end", (_event: Ev, context: Ctx) => {
		ctx = context;
		event("agent_end", { idle: Boolean(ctx?.isIdle?.()), jobs: jobs(), session: session() });
	});
	pi.on("turn_end", (_event: Ev, context: Ctx) => {
		ctx = context;
		const usage = ctx?.getContextUsage?.();
		if (usage) event("usage", { tokens: usage.tokens ?? 0, percent: usage.percent ?? 0 });
	});
	pi.on("tool_execution_start", (e: Ev) => {
		if (e.toolName === "ask") event("input", { open: true, kind: "ask" });
	});
	pi.on("tool_execution_end", (e: Ev) => {
		if (e.toolName === "ask") event("input", { open: false, kind: "ask" });
		if (e.toolName === "todo") {
			const phases = e.result?.details?.phases ?? e.details?.phases ?? [];
			const tasks = (phases ?? []).flatMap((phase) => phase.tasks ?? []);
			const done = tasks.filter((task) => task.status === "completed").length;
			event("todo", { done, total: tasks.length });
		}
	});
	pi.on("tool_approval_requested", () => event("input", { open: true, kind: "approval" }));
	pi.on("tool_approval_resolved", () => event("input", { open: false, kind: "approval" }));
	pi.on("plan_review_requested", () => event("input", { open: true, kind: "plan" }));
	pi.on("plan_review_resolved", () => event("input", { open: false, kind: "plan" }));
	pi.on("session_stop", async (e: Ev, context: Ctx) => {
		ctx = context;
		if (e.stop_hook_active || e.signal?.aborted || !up) return undefined;
		try {
			const answer = await request({ t: "stop" }, e.signal);
			const text = String(answer.context ?? "");
			return text ? { continue: true, additionalContext: text } : undefined;
		} catch {
			return undefined;
		}
	});
	pi.on("session_shutdown", () => {
		event("shutdown");
		stopped = true;
	});
}
