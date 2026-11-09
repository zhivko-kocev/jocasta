import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
import { sameToken } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import { readSettings } from "../../shared/settings.ts";

/** The endpoint's base path; an OpenAI client posts to `<base>/chat/completions`. */
export const LLM_PATH = "/llm/v1";

/** Graphiti's two model names (0341). */
export type GraphModel = "main" | "small";

/** One model call: the system text, the user turns in order, and a token cap. */
export interface CompletionRequest {
	system?: string;
	messages: string[];
	maxTokens?: number;
	signal?: AbortSignal;
}

interface Completion {
	/** The id of the model that answered. */
	model: string;
	text: string;
	stopReason: "stop" | "length";
	usage: { input: number; output: number; cacheRead: number; cacheWrite: number };
}

/** Where the endpoint's calls go (0390); `complete` throws when no model answers. */
export interface Completer {
	complete(model: GraphModel, request: CompletionRequest): Promise<Completion>;
}

interface LlmEndpoint {
	/** The bearer token a caller must send. */
	token: string;
	/** Answers a request under `LLM_PATH`; `undefined` for any other path. */
	handle(request: Request): Promise<Response | undefined>;
}

interface LlmEndpointOptions {
	token?: string;
	/** Test seam; his omp (`ompCompleter()`) by default. */
	completer?: Completer;
}

/** The chat completion request as Graphiti 0.30.2's `OpenAIGenericClient._generate_response` sends it. */
interface ChatRequest {
	model?: string;
	messages?: { role?: string; content?: unknown }[];
	max_tokens?: number;
	max_completion_tokens?: number;
	stream?: boolean;
	response_format?: { type?: string; json_schema?: { name?: string; schema?: unknown } };
}

function error(status: number, message: string, type = "invalid_request_error"): Response {
	return Response.json({ error: { message, type } }, { status });
}

function textOf(content: unknown): string | undefined {
	if (typeof content === "string") return content;
	if (!Array.isArray(content)) return undefined;
	const parts: string[] = [];
	for (const part of content as { type?: string; text?: unknown }[]) {
		if (part?.type !== "text" || typeof part.text !== "string") return undefined;
		parts.push(part.text);
	}
	return parts.join("");
}

/** The instruction appended to the system prompt when JSON is asked for; omp's gateway drops `response_format`. */
function jsonInstruction(format: ChatRequest["response_format"]): string | undefined {
	if (format?.type === "json_schema") {
		return `Reply with only one JSON object that matches this JSON Schema, with no other text and no code fence:\n${JSON.stringify(format.json_schema?.schema ?? {})}`;
	}
	if (format?.type === "json_object") return "Reply with only one JSON object, with no other text and no code fence.";
	return undefined;
}

/** The JSON object in a reply that wrapped it in a fence or prose; the text unchanged when it holds none. */
export function extractJson(text: string): string {
	const trimmed = text.trim();
	const fenced = /^```[\w-]*\s*\n?([\s\S]*?)\n?```$/.exec(trimmed)?.[1]?.trim() ?? trimmed;
	try {
		JSON.parse(fenced);
		return fenced;
	} catch {
		const start = fenced.indexOf("{");
		const end = fenced.lastIndexOf("}");
		return start >= 0 && end > start ? fenced.slice(start, end + 1) : fenced;
	}
}

/**
 * The OpenAI-compatible endpoint Graphiti's model calls come back to (0334): `POST <LLM_PATH>/chat/completions`,
 * non-streaming, with a bearer token. The request's `model` name (`main` or `small`, 0341) goes to the completer, by
 * default `omp auth-gateway stdio` on his logins and roles (0463). The service's server (`server.ts`) serves it beside
 * the page, on its port, for Graphiti's container, which calls back through `host.docker.internal` (`graph/callback.ts`).
 */
export function createLlmEndpoint(options: LlmEndpointOptions = {}): LlmEndpoint {
	const token = options.token ?? randomBytes(32).toString("hex");
	const completer = options.completer ?? defaultCompleter;
	return {
		token,
		async handle(request) {
			const path = new URL(request.url).pathname;
			if (path !== LLM_PATH && !path.startsWith(`${LLM_PATH}/`)) return undefined;
			const auth = request.headers.get("authorization") ?? "";
			if (!sameToken(auth.startsWith("Bearer ") ? auth.slice(7) : "", token)) {
				return error(401, "invalid token", "authentication_error");
			}
			if (request.method !== "POST" || path !== `${LLM_PATH}/chat/completions`) return error(404, "not found");
			let body: ChatRequest;
			try {
				body = (await request.json()) as ChatRequest;
			} catch {
				return error(400, "body is not JSON");
			}
			if (body.stream) return error(400, "streaming is not supported");
			const system: string[] = [];
			const messages: string[] = [];
			for (const message of body.messages ?? []) {
				const text = textOf(message.content);
				if (text === undefined) return error(400, "message content must be text");
				if (message.role === "system" || message.role === "developer") system.push(text);
				else if (message.role === "user") messages.push(text);
				else return error(400, `unsupported message role: ${String(message.role)}`);
			}
			if (messages.length === 0) return error(400, "no user message");
			const name = body.model ?? "";
			if (name !== "main" && name !== "small") {
				return error(404, `no model for ${JSON.stringify(name)}`, "model_not_found");
			}
			const json = jsonInstruction(body.response_format);
			if (json) system.push(json);
			const maxTokens = body.max_completion_tokens ?? body.max_tokens;
			let reply: Completion;
			try {
				reply = await completer.complete(name, {
					...(system.length > 0 ? { system: system.join("\n\n") } : {}),
					messages,
					...(maxTokens === undefined ? {} : { maxTokens }),
					signal: request.signal,
				});
			} catch (cause) {
				const message = cause instanceof Error ? cause.message : String(cause);
				log("warn", "llm", `Graphiti's ${name} model call failed: ${message}`);
				return error(502, message, "upstream_error");
			}
			const { input, output, cacheRead, cacheWrite } = reply.usage;
			return Response.json({
				id: `chatcmpl-${randomBytes(12).toString("hex")}`,
				object: "chat.completion",
				created: Math.floor(Date.now() / 1000),
				model: reply.model,
				choices: [
					{
						index: 0,
						message: { role: "assistant", content: json ? extractJson(reply.text) : reply.text },
						finish_reason: reply.stopReason,
					},
				],
				usage: {
					prompt_tokens: input + cacheRead + cacheWrite,
					completion_tokens: output,
					total_tokens: input + cacheRead + cacheWrite + output,
				},
			});
		},
	};
}

// --- his omp as the completer: `omp auth-gateway stdio` (0390, 0463) ---

/** omp's role behind each Graphiti name when `jocasta.yaml` names no model. */
const OMP_ROLE: Record<GraphModel, string> = { main: "default", small: "smol" };

/** The gateway route the endpoint's calls take. */
const CHAT_PATH = "/v1/chat/completions";

/** The subset of the gateway's OpenAI chat-completion reply this file reads (pi-ai `openai-chat-server.ts`). */
interface GatewayReply {
	model?: string;
	choices?: { message?: { content?: string | null }; finish_reason?: string }[];
	usage?: { prompt_tokens?: number; completion_tokens?: number; prompt_tokens_details?: { cached_tokens?: number } };
	error?: { message?: string };
}

/** One response line of `omp auth-gateway stdio` (pi-ai `auth-gateway/stdio.ts`); the first is `{ready, version}`. */
interface GatewayLine {
	id?: string | number | null;
	status?: number;
	body?: unknown;
	ready?: boolean;
	version?: string;
}

/** `graph.main` / `graph.small` in `jocasta.yaml`: an omp `provider/id`, or `undefined` for omp's role. */
function configuredModel(settings: Record<string, unknown>, name: GraphModel): string | undefined {
	const graph = settings.graph;
	const value = graph && typeof graph === "object" && name in graph ? graph[name as keyof typeof graph] : undefined;
	if (value === undefined) return undefined;
	if (typeof value !== "string" || !/^[^/\s]+\/\S+$/.test(value)) {
		throw new Error(`graph.${name} in jocasta.yaml must be an omp model as provider/id`);
	}
	return value;
}

/** A gateway answer as a completion; a non-200 status throws with the gateway's message. */
function completion(selector: string, status: number, body: unknown): Completion {
	const reply = (body && typeof body === "object" ? body : {}) as GatewayReply;
	if (status !== 200) {
		const message = reply.error?.message ?? (typeof body === "string" && body.trim() ? body.trim() : undefined);
		throw new Error(message ?? `omp auth-gateway answered ${status}`);
	}
	const choice = reply.choices?.[0];
	const prompt = reply.usage?.prompt_tokens ?? 0;
	const cached = reply.usage?.prompt_tokens_details?.cached_tokens ?? 0;
	return {
		model: reply.model ?? selector,
		text: choice?.message?.content ?? "",
		stopReason: choice?.finish_reason === "length" ? "length" : "stop",
		usage: { input: prompt - cached, output: reply.usage?.completion_tokens ?? 0, cacheRead: cached, cacheWrite: 0 },
	};
}

/** One running `omp auth-gateway stdio` child: requests by id, answered in any order. */
interface Gateway {
	send(path: string, body: unknown, signal?: AbortSignal): Promise<GatewayLine>;
	close(): void;
}

function startGateway(command: string[], onExit: () => void): Gateway {
	const child = Bun.spawn([...command, "auth-gateway", "stdio"], {
		// Out of any project, so no project `.omp` settings or extensions load (as `omp -p` ran before, omp-adaptation §1).
		cwd: tmpdir(),
		stdin: "pipe",
		stdout: "pipe",
		stderr: "pipe",
		windowsHide: true,
	});
	const pending = new Map<number, PromiseWithResolvers<GatewayLine>>();
	let nextId = 1;
	let stderrTail = "";
	let gone: Error | undefined;

	const answers = (async () => {
		const decoder = new TextDecoder();
		let buffered = "";
		for await (const chunk of child.stdout) {
			buffered += decoder.decode(chunk, { stream: true });
			let newline = buffered.indexOf("\n");
			while (newline >= 0) {
				const text = buffered.slice(0, newline).trim();
				buffered = buffered.slice(newline + 1);
				newline = buffered.indexOf("\n");
				let line: GatewayLine | null;
				try {
					line = JSON.parse(text) as GatewayLine | null;
				} catch {
					continue;
				}
				if (line?.ready) log("info", "llm", `omp auth-gateway stdio ready (omp ${line.version ?? "unknown"})`);
				else if (typeof line?.id === "number") pending.get(line.id)?.resolve(line);
			}
		}
	})().catch(() => undefined);
	const errors = (async () => {
		const decoder = new TextDecoder();
		for await (const chunk of child.stderr) stderrTail = (stderrTail + decoder.decode(chunk)).slice(-500);
	})().catch(() => undefined);
	// Every line it wrote is read before the calls still waiting fail.
	void Promise.all([child.exited, answers, errors]).then(([code]) => {
		const tail = stderrTail.trim();
		gone = new Error(`omp auth-gateway stdio exited ${code}${tail ? `: ${tail}` : ""}`);
		for (const waiter of pending.values()) waiter.reject(gone);
		pending.clear();
		onExit();
	});
	// The service's stdin pipe closing ends the child (the gateway serves until stdin ends), so it never pins the process.
	child.unref();

	return {
		async send(path, body, signal) {
			if (gone) throw gone;
			if (signal?.aborted) throw new Error("model call aborted");
			const id = nextId++;
			const waiter = Promise.withResolvers<GatewayLine>();
			pending.set(id, waiter);
			// The protocol has no cancel line: an aborted call stops waiting and its late answer is dropped.
			const abort = () => {
				pending.delete(id);
				waiter.reject(new Error("model call aborted"));
			};
			signal?.addEventListener("abort", abort, { once: true });
			try {
				child.stdin.write(`${JSON.stringify({ id, path, body })}\n`);
				void child.stdin.flush();
				return await waiter.promise;
			} finally {
				pending.delete(id);
				signal?.removeEventListener("abort", abort);
			}
		},
		close() {
			try {
				void child.stdin.end();
			} catch {
				// Already closed.
			}
		},
	};
}

interface OmpCompleterOptions {
	/** Test seam; `["omp"]` by default. `auth-gateway stdio` is appended. */
	command?: string[];
	/** Test seam; `readSettings()` by default. */
	settings?: () => Record<string, unknown>;
}

interface OmpCompleter extends Completer {
	/** Ends the gateway child, if one runs (its stdin closes and it exits once its answers are written). */
	close(): void;
}

/**
 * His omp as Graphiti's model (0390, 0463): `graph.main` / `graph.small` from `jocasta.yaml`, else omp's `@default` /
 * `@smol` roles with their `retry.fallbackChains`, through one `omp auth-gateway stdio` child started on the first
 * call and started again on the next call after it exits.
 */
export function ompCompleter(options: OmpCompleterOptions = {}): OmpCompleter {
	const command = options.command ?? ["omp"];
	const settings = options.settings ?? readSettings;
	let gateway: Gateway | undefined;
	return {
		async complete(name, request) {
			const selector = configuredModel(settings(), name) ?? `@${OMP_ROLE[name]}`;
			if (!gateway) {
				const started: Gateway = startGateway(command, () => {
					if (gateway === started) gateway = undefined;
				});
				gateway = started;
			}
			const messages = [
				...(request.system === undefined ? [] : [{ role: "system", content: request.system }]),
				...request.messages.map((content) => ({ role: "user", content })),
			];
			const line = await gateway.send(
				CHAT_PATH,
				{
					model: selector,
					messages,
					...(request.maxTokens === undefined ? {} : { max_tokens: request.maxTokens }),
				},
				request.signal,
			);
			return completion(selector, line.status ?? 500, line.body);
		},
		close() {
			gateway?.close();
			gateway = undefined;
		},
	};
}

const defaultCompleter = ompCompleter();
