import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { sameToken } from "../../shared/fsx.ts";
import { appendJsonl } from "../../shared/jsonl.ts";
import { ompModelCost } from "../../shared/ledger.ts";
import { LineRpc } from "../../shared/line-rpc.ts";
import { log } from "../../shared/log.ts";
import { LLM_PATH } from "../../shared/memory/graph/callback.ts";
import { dataDir } from "../../shared/paths.ts";
import { checkSettings, readSettings, type Settings } from "../../shared/settings.ts";

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
	/** The omp provider the call was sent to, when its selector named one (`provider/id`); null for an omp role. */
	provider: string | null;
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
	/** Test seam; `<dataDir>/graph-usage.jsonl` by default. */
	usageFile?: string;
}

/** One answered Graphiti model call in `<dataDir>/graph-usage.jsonl` (0582), read into "$ today". */
export interface GraphUsageLine {
	at: string;
	model: string;
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite: number;
	/** Dollars at omp's models.db prices; 0 for a model it doesn't price. */
	cost: number;
}

/** Appends the call's usage, priced from omp's models.db (0582); a failed write is logged and the call still answers. */
function recordUsage(file: string, reply: Completion): void {
	const { input, output, cacheRead, cacheWrite } = reply.usage;
	// The reply may name the model as `provider/id` or as the id alone; models.db rows hold the id.
	const prefix = reply.provider === null ? undefined : `${reply.provider}/`;
	const id = prefix && reply.model.startsWith(prefix) ? reply.model.slice(prefix.length) : reply.model;
	const price = ompModelCost(reply.provider, id);
	const cost = price
		? (input * price.input + output * price.output + cacheRead * price.cacheRead + cacheWrite * price.cacheWrite) / 1e6
		: 0;
	const line: GraphUsageLine = {
		at: new Date().toISOString(),
		model: reply.model,
		input,
		output,
		cacheRead,
		cacheWrite,
		cost,
	};
	try {
		appendJsonl(file, line);
	} catch (cause) {
		log(
			"warn",
			"llm",
			`couldn't record Graphiti's model usage in ${file}: ${cause instanceof Error ? cause.message : String(cause)}`,
		);
	}
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
			recordUsage(options.usageFile ?? join(dataDir(), "graph-usage.jsonl"), reply);
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

/**
 * `graph.main` / `graph.small` in `jocasta.yaml`: an omp `provider/id`, or `undefined` for omp's role. A bad value of
 * either throws `jocasta.yaml`'s line and turns Graphiti's model calls off (0584); a `graph` that isn't a mapping
 * uses omp's roles, as its settings problem says.
 */
function configuredModel(settings: Record<string, unknown>, name: GraphModel): string | undefined {
	const graph = (settings as Settings).graph;
	if (graph === null || typeof graph !== "object" || Array.isArray(graph)) return undefined;
	checkSettings(settings, "graph.main");
	checkSettings(settings, "graph.small");
	return graph[name];
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
		// `@role` selectors name no provider; `provider/id` ones do (configuredModel).
		provider: selector.startsWith("@") ? null : selector.slice(0, selector.indexOf("/")),
		text: choice?.message?.content ?? "",
		stopReason: choice?.finish_reason === "length" ? "length" : "stop",
		usage: { input: prompt - cached, output: reply.usage?.completion_tokens ?? 0, cacheRead: cached, cacheWrite: 0 },
	};
}

/** Starts one `omp auth-gateway stdio` child: requests by id, answered in any order. */
function startGateway(command: string[], onExit: () => void): LineRpc<GatewayLine> {
	const gateway = new LineRpc<GatewayLine>([...command, "auth-gateway", "stdio"], {
		// Out of any project, so no project `.omp` settings or extensions load (as `omp -p` ran before, omp-adaptation §1).
		cwd: tmpdir(),
		exitError: (code, stderr) => {
			const tail = stderr.trim();
			return new Error(`omp auth-gateway stdio exited ${code}${tail ? `: ${tail}` : ""}`);
		},
		onOther: (line) => {
			if (line.ready) log("info", "llm", `omp auth-gateway stdio ready (omp ${line.version ?? "unknown"})`);
		},
		onExit,
	});
	// The service's stdin pipe closing ends the child (the gateway serves until stdin ends), so it never pins the process.
	gateway.unref();
	return gateway;
}

interface OmpCompleterOptions {
	/** Test seam; `["omp"]` by default. `auth-gateway stdio` is appended. */
	command?: string[];
	/** Test seam; `readSettings()` by default. */
	settings?: () => Record<string, unknown>;
	/** Test seam: how long the gateway child may sit without calls before it is closed; 15 minutes by default. */
	idleMs?: number;
}

interface OmpCompleter extends Completer {
	/** Ends the gateway child, if one runs (its stdin closes and it exits once its answers are written). */
	close(): void;
}

/** The gateway child closes after this long without calls, as Graphiti's containers stop after 15 idle minutes (0073, 0340). */
const GATEWAY_IDLE_MS = 15 * 60_000;

/**
 * His omp as Graphiti's model (0390, 0463): `graph.main` / `graph.small` from `jocasta.yaml`, else omp's `@default` /
 * `@smol` roles with their `retry.fallbackChains`, through one `omp auth-gateway stdio` child started on the first
 * call, closed 15 minutes after the last call ends, and started again on the next call after it exits or closes.
 */
export function ompCompleter(options: OmpCompleterOptions = {}): OmpCompleter {
	const command = options.command ?? ["omp"];
	const settings = options.settings ?? readSettings;
	const idleMs = options.idleMs ?? GATEWAY_IDLE_MS;
	let gateway: LineRpc<GatewayLine> | undefined;
	let calls = 0;
	let idle: Timer | undefined;
	const close = () => {
		clearTimeout(idle);
		idle = undefined;
		gateway?.close();
		gateway = undefined;
	};
	return {
		async complete(name, request) {
			const selector = configuredModel(settings(), name) ?? `@${OMP_ROLE[name]}`;
			clearTimeout(idle);
			idle = undefined;
			if (!gateway) {
				const started: LineRpc<GatewayLine> = startGateway(command, () => {
					if (gateway === started) gateway = undefined;
				});
				gateway = started;
			}
			const messages = [
				...(request.system === undefined ? [] : [{ role: "system", content: request.system }]),
				...request.messages.map((content) => ({ role: "user", content })),
			];
			calls++;
			try {
				const body = {
					model: selector,
					messages,
					...(request.maxTokens === undefined ? {} : { max_tokens: request.maxTokens }),
				};
				const line = await gateway.request((id) => ({ id, path: CHAT_PATH, body }), {
					signal: request.signal,
					abortMessage: "model call aborted",
				});
				return completion(selector, line.status ?? 500, line.body);
			} finally {
				calls--;
				if (calls === 0) {
					// Unref'd: the idle clock never keeps the service alive.
					idle = setTimeout(close, idleMs);
					idle.unref();
				}
			}
		},
		close,
	};
}

const defaultCompleter = ompCompleter();
