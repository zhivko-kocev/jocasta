import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { asObject } from "./fsx.ts";
import { jsonLines } from "./jsonl.ts";
import { transcriptsOf } from "./omp-session.ts";
import { ompAgentDir } from "./paths.ts";

/**
 * The cost ledger (0538, 0546), one implementation for the product and the benchmark: rows computed from saved
 * session records. Per model request: time, model, uncached input, cache read, cache write, output and its cost; per
 * tool call: the tool, an argument hash, the file path, the result's size and whether it failed. Per session: the
 * waste counts and the peak context. What a request costs and the model's window come from a {@link LedgerPricing}:
 * the product's is omp's own (`usage.cost.total`, `models.db`), the benchmark's its pinned price table.
 */

export type LedgerHarness = "omp" | "claude" | "opencode";

export interface RequestRow {
	kind: "request";
	session: string;
	/** `main`, or the subagent file's name. */
	agent: string;
	/** Epoch ms. */
	at: number;
	provider: string | null;
	model: string;
	input: number;
	cacheRead: number;
	cacheWrite: number;
	/** The part of `cacheWrite` written with a 1-hour lifetime. */
	cacheWrite1h: number;
	output: number;
	/** Dollars from the pricing; null when it can't price the request. */
	cost: number | null;
	/** omp's side requests (`model_usage`): `cache-warm`, compaction and the like; absent for the agent's own turns. */
	purpose?: string;
}

/** A request's fields as a reader has them, before it is priced. */
export type RequestFields = Omit<RequestRow, "kind" | "session" | "agent" | "cost">;

export interface ToolRow {
	kind: "tool";
	session: string;
	agent: string;
	at: number;
	tool: string;
	/** The first 16 hex of SHA-256 over the arguments as sorted-key JSON (omp's `i`/`intent` label left out). */
	argHash: string;
	/** The file the call names, the first one when it names several; null when none. */
	path: string | null;
	/** UTF-8 bytes of the result's text; null when the call has no result. */
	resultBytes: number | null;
	error: boolean;
}

export type LedgerRow = RequestRow | ToolRow;

/** A session as read: its rows and the times of its compactions. */
export interface SessionRecord {
	harness: LedgerHarness;
	session: string;
	agent: string;
	/**
	 * The session its header names as the one it moved from, forked off or ran under (omp's `parentSession`: an id
	 * for a move, a `/tan` clone or a seance fork, a file path for a subagent); its file may hold copies of that one's
	 * entries. Absent when the header names none.
	 */
	parentSession?: string;
	/** Epoch ms of the session's header, when it has one; orders a thread's files parent-first. */
	startedAt?: number;
	requests: RequestRow[];
	tools: ToolRow[];
	/** Every file each call touched, by index into `tools`: reads, edits and writes. */
	touched: string[][];
	/** Times of its compactions, Jocasta's own `/handoff` (`method: "handoff"`) left out (0582). */
	compactions: number[];
}

/** The waste counts of 0538. */
export interface Waste {
	/** Reads of a file read before with no edit or write of it in between (a shell command's changes aren't seen). */
	rereads: number;
	/** A non-read call with the same tool and argument hash as an earlier one, with no edit or write in between. */
	repeatedCalls: number;
	/** Results of {@link OVERSIZED_RESULT_BYTES} or more. */
	oversizedResults: number;
	/** Failures of a tool after its first failure in a row (a success of that tool ends the row). */
	repeatedFailures: number;
	/**
	 * Requests writing {@link CACHE_MISS_WRITE_TOKENS} or more to the cache after more than 5 minutes without a request
	 * on the same provider and model (each model has its own cache).
	 */
	cacheMisses: number;
	/** The idle minutes before each cache miss, rounded. */
	cacheMissIdleMinutes: number[];
	/**
	 * omp's compactions (`soft`, `shake`, `remote`, legacy), not Jocasta's own `/handoff` (`method: "handoff"`), which
	 * no context pressure caused (0582).
	 */
	compactions: number;
}

export interface PeakContext {
	/** The largest uncached input + cache read + cache write of the agent's own requests. */
	tokens: number;
	/** The model's window from the pricing; null when unknown. */
	window: number | null;
	percent: number | null;
}

export interface Totals {
	requests: number;
	toolCalls: number;
	input: number;
	cacheRead: number;
	cacheWrite: number;
	output: number;
	/** Dollars of the priced requests. */
	cost: number;
	/** Cache read over all input (uncached + read + write); null without input. */
	cacheReadShare: number | null;
	/** Models the pricing can't price; their requests count in the tokens, not in `cost`. */
	unpricedModels: string[];
}

export interface SessionSummary extends Totals {
	harness: LedgerHarness;
	session: string;
	agent: string;
	peakContext: PeakContext;
	waste: Waste;
}

export interface TrialLedger {
	sessions: SessionSummary[];
	totals: Totals & { peakContext: PeakContext; waste: Waste };
	rows: LedgerRow[];
}

/** What a request costs and how large the model's window is; the seam between the ledger and a price source. */
export interface LedgerPricing {
	/** Dollars of one request; `usage` is the request's usage object as the session file has it. Null when unknown. */
	cost(request: RequestFields, usage: Record<string, unknown>): number | null;
	/** The model's context window in tokens; null when unknown. */
	window(provider: string | null, model: string): number | null;
}

export const OVERSIZED_RESULT_BYTES = 50_000;
const CACHE_MISS_IDLE_MS = 5 * 60_000;
export const CACHE_MISS_WRITE_TOKENS = 10_000;

const READ_TOOLS: Record<string, true> = { read: true, Read: true };
const WRITE_TOOLS: Record<string, true> = {
	edit: true,
	write: true,
	ast_edit: true,
	Edit: true,
	Write: true,
	MultiEdit: true,
	NotebookEdit: true,
	apply_patch: true,
	patch: true,
	multiedit: true,
};

/** omp's read selector at a path's end: `:50`, `:50-80`, `:5-16,960-973`, `:-60`, `:raw`, `:2-4:raw`, `:conflicts`, `:img`. */
const SELECTOR = /:(?:-?\d[\d,+-]*(?::raw)?|raw(?::-?\d[\d,+-]*)?|conflicts|img)$/;

/** A finite number, else 0 (token counts as the session files have them). */
export const tokenCount = (value: unknown): number => (typeof value === "number" && Number.isFinite(value) ? value : 0);

/** Sorted-key JSON, so the same arguments in another order hash alike. */
function canonical(value: unknown): string {
	if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
	if (typeof value === "object" && value !== null)
		return `{${Object.keys(value)
			.sort()
			.map((key) => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`)
			.join(",")}}`;
	return JSON.stringify(value) ?? "null";
}

export function argHash(args: unknown, harness: LedgerHarness): string {
	let hashed = args;
	if (harness === "omp" && typeof args === "object" && args !== null && !Array.isArray(args)) {
		const { i: _i, intent: _intent, ...rest } = args as Record<string, unknown>;
		hashed = rest;
	}
	return createHash("sha256").update(canonical(hashed)).digest("hex").slice(0, 16);
}

/** A path as compared: forward slashes, no leading `./`, omp's read selector (`:50-80`, `:raw`) cut, lowercased. */
export function normalizePath(path: string): string {
	let cut = path.trim().replaceAll("\\", "/").replace(/^\.\//, "");
	const selector = SELECTOR.exec(cut);
	if (selector && selector.index > 1) cut = cut.slice(0, selector.index);
	return cut.toLowerCase();
}

/** The files a call names: `path`/`file_path`/`filePath`/`notebook_path`, omp edit's `[path#TAG]` headers, patch headers. */
export function pathsOf(args: unknown): string[] {
	if (typeof args !== "object" || args === null) return [];
	const record = args as Record<string, unknown>;
	const found: string[] = [];
	for (const key of ["path", "file_path", "filePath", "notebook_path"]) {
		const value = record[key];
		if (typeof value === "string" && value.trim() !== "") found.push(value);
	}
	for (const key of ["input", "patchText", "patch"]) {
		const text = record[key];
		if (typeof text !== "string") continue;
		for (const match of text.matchAll(/^\[([^\]\n#]+)#[0-9A-Fa-f]{4}\]/gm)) found.push(match[1] as string);
		for (const match of text.matchAll(/^\*\*\* (?:Update|Add|Delete) File: (.+)$/gm))
			found.push((match[1] as string).trim());
	}
	return [...new Set(found.map(normalizePath))];
}

/** UTF-8 bytes of `text`. */
export const utf8Bytes = (text: string): number => Buffer.byteLength(text, "utf8");

/** The text of a result's content: a string, or text blocks. */
export function resultText(content: unknown): string {
	if (typeof content === "string") return content;
	if (!Array.isArray(content)) return "";
	return content
		.map((block) => {
			const b = block as { type?: unknown; text?: unknown; content?: unknown };
			if (typeof b?.text === "string") return b.text;
			if (b?.type === "tool_result") return resultText(b.content);
			return "";
		})
		.join("");
}

/** Epoch ms of a number or an ISO string; NaN otherwise. */
export const timeOf = (value: unknown): number =>
	typeof value === "number" ? value : typeof value === "string" ? Date.parse(value) : Number.NaN;

export function newRecord(harness: LedgerHarness, session: string, agent: string): SessionRecord {
	return { harness, session, agent, requests: [], tools: [], touched: [], compactions: [] };
}

/** Adds one tool call to `record`, with the files it touched. */
export function pushTool(
	record: SessionRecord,
	call: { at: number; tool: string; args: unknown; resultBytes: number | null; error: boolean },
): void {
	const touched = pathsOf(call.args);
	record.tools.push({
		kind: "tool",
		session: record.session,
		agent: record.agent,
		at: call.at,
		tool: call.tool,
		argHash: argHash(call.args, record.harness),
		path: touched[0] ?? null,
		resultBytes: call.resultBytes,
		error: call.error,
	});
	record.touched.push(touched);
}

/** One request of `record`, priced. */
export function requestRow(
	record: SessionRecord,
	pricing: LedgerPricing,
	fields: RequestFields,
	usage: Record<string, unknown>,
): RequestRow {
	return {
		kind: "request",
		session: record.session,
		agent: record.agent,
		...fields,
		cost: pricing.cost(fields, usage),
	};
}

/**
 * One omp session file: assistant messages and `model_usage` entries are requests; `toolCall` blocks are calls;
 * compactions other than `method: "handoff"` count (0582). The header's `parentSession` and time are kept for
 * {@link withoutCopies}.
 */
export function readOmpSession(text: string, pricing: LedgerPricing, agent = "main"): SessionRecord {
	const entries = jsonLines(text);
	const header = entries.find((entry) => entry.type === "session");
	const record = newRecord("omp", typeof header?.id === "string" ? header.id : agent, agent);
	if (typeof header?.parentSession === "string") record.parentSession = header.parentSession;
	const startedAt = timeOf(header?.timestamp);
	if (Number.isFinite(startedAt)) record.startedAt = startedAt;
	const results = new Map<string, { bytes: number; error: boolean }>();
	for (const entry of entries) {
		const message = entry.message as Record<string, unknown> | undefined;
		if (entry.type === "message" && message?.role === "toolResult" && typeof message.toolCallId === "string")
			results.set(message.toolCallId, {
				bytes: utf8Bytes(resultText(message.content)),
				error: message.isError === true,
			});
	}
	for (const entry of entries) {
		const at = timeOf(entry.timestamp);
		if (entry.type === "compaction") {
			if (entry.method !== "handoff") record.compactions.push(at);
			continue;
		}
		if (entry.type === "model_usage") {
			const usage = (entry.usage ?? {}) as Record<string, unknown>;
			record.requests.push(
				requestRow(
					record,
					pricing,
					{
						at,
						provider: typeof entry.provider === "string" ? entry.provider : null,
						model: String(entry.model ?? "unknown"),
						input: tokenCount(usage.input),
						cacheRead: tokenCount(usage.cacheRead),
						cacheWrite: tokenCount(usage.cacheWrite),
						cacheWrite1h: tokenCount((usage.cttl as Record<string, unknown> | undefined)?.ephemeral1h),
						output: tokenCount(usage.output),
						purpose: typeof entry.purpose === "string" ? entry.purpose : "side",
					},
					usage,
				),
			);
			continue;
		}
		const message = entry.message as Record<string, unknown> | undefined;
		if (entry.type !== "message" || message?.role !== "assistant") continue;
		const when = Number.isFinite(timeOf(message.timestamp)) ? timeOf(message.timestamp) : at;
		const usage = message.usage as Record<string, unknown> | undefined;
		if (usage)
			record.requests.push(
				requestRow(
					record,
					pricing,
					{
						at: when,
						provider: typeof message.provider === "string" ? message.provider : null,
						model: String(message.model ?? "unknown"),
						input: tokenCount(usage.input),
						cacheRead: tokenCount(usage.cacheRead),
						cacheWrite: tokenCount(usage.cacheWrite),
						cacheWrite1h: tokenCount((usage.cttl as Record<string, unknown> | undefined)?.ephemeral1h),
						output: tokenCount(usage.output),
					},
					usage,
				),
			);
		for (const block of Array.isArray(message.content) ? message.content : []) {
			const b = block as { type?: unknown; id?: unknown; name?: unknown; arguments?: unknown };
			if (b?.type !== "toolCall") continue;
			const result = typeof b.id === "string" ? results.get(b.id) : undefined;
			pushTool(record, {
				at: when,
				tool: String(b.name ?? "unknown"),
				args: b.arguments,
				resultBytes: result?.bytes ?? null,
				error: result?.error ?? false,
			});
		}
	}
	return record;
}

/** The waste counts of one session. */
export function wasteOf(record: SessionRecord): Waste {
	const lastRead = new Map<string, number>();
	const lastWrite = new Map<string, number>();
	let lastAnyWrite = -1;
	const seen = new Map<string, number>();
	const failing = new Map<string, number>();
	let rereads = 0;
	let repeatedCalls = 0;
	let oversizedResults = 0;
	let repeatedFailures = 0;
	record.tools.forEach((call, i) => {
		const touched = record.touched[i] ?? [];
		if (READ_TOOLS[call.tool]) {
			for (const path of touched) {
				const before = lastRead.get(path);
				if (before !== undefined && (lastWrite.get(path) ?? -1) < before) rereads++;
				lastRead.set(path, i);
			}
		} else {
			const key = `${call.tool}\u0000${call.argHash}`;
			const before = seen.get(key);
			if (before !== undefined && lastAnyWrite < before) repeatedCalls++;
			seen.set(key, i);
		}
		if (WRITE_TOOLS[call.tool] && !call.error) {
			for (const path of touched) lastWrite.set(path, i);
			lastAnyWrite = i;
		}
		if ((call.resultBytes ?? 0) >= OVERSIZED_RESULT_BYTES) oversizedResults++;
		if (call.error) {
			const streak = failing.get(call.tool) ?? 0;
			if (streak > 0) repeatedFailures++;
			failing.set(call.tool, streak + 1);
		} else failing.set(call.tool, 0);
	});
	const cacheMissIdleMinutes: number[] = [];
	const previous = new Map<string, number>();
	for (const row of record.requests) {
		const model = `${row.provider ?? ""}/${row.model}`;
		const before = previous.get(model);
		if (before !== undefined && row.at - before > CACHE_MISS_IDLE_MS && row.cacheWrite >= CACHE_MISS_WRITE_TOKENS)
			cacheMissIdleMinutes.push(Math.round((row.at - before) / 60_000));
		if (Number.isFinite(row.at)) previous.set(model, row.at);
	}
	return {
		rereads,
		repeatedCalls,
		oversizedResults,
		repeatedFailures,
		cacheMisses: cacheMissIdleMinutes.length,
		cacheMissIdleMinutes,
		compactions: record.compactions.length,
	};
}

/** The agent's own requests (side requests left out). */
const ownRequests = (record: SessionRecord): RequestRow[] => record.requests.filter((row) => row.purpose === undefined);

/** The agent's own requests' largest context (side requests left out), against the model's window. */
function peakContextOf(record: SessionRecord, pricing: LedgerPricing): PeakContext {
	let peak: RequestRow | undefined;
	let tokens = 0;
	for (const row of ownRequests(record)) {
		const size = row.input + row.cacheRead + row.cacheWrite;
		if (size > tokens || peak === undefined) {
			tokens = size;
			peak = row;
		}
	}
	const window = peak ? pricing.window(peak.provider, peak.model) : null;
	return { tokens, window, percent: window ? Math.round((tokens / window) * 1000) / 10 : null };
}

function totalsOf(requests: readonly RequestRow[], toolCalls: number): Totals {
	const sum = (key: "input" | "cacheRead" | "cacheWrite" | "output") =>
		requests.reduce((total, row) => total + row[key], 0);
	const input = sum("input");
	const cacheRead = sum("cacheRead");
	const cacheWrite = sum("cacheWrite");
	const all = input + cacheRead + cacheWrite;
	return {
		requests: requests.length,
		toolCalls,
		input,
		cacheRead,
		cacheWrite,
		output: sum("output"),
		cost: requests.reduce((total, row) => total + (row.cost ?? 0), 0),
		cacheReadShare: all === 0 ? null : cacheRead / all,
		unpricedModels: [...new Set(requests.filter((row) => row.cost === null).map((row) => row.model))].sort(),
	};
}

export function summarizeSession(record: SessionRecord, pricing: LedgerPricing): SessionSummary {
	return {
		harness: record.harness,
		session: record.session,
		agent: record.agent,
		...totalsOf(record.requests, record.tools.length),
		peakContext: peakContextOf(record, pricing),
		waste: wasteOf(record),
	};
}

/** The waste counts of several sessions, summed. */
function sumWaste(wastes: readonly Waste[]): Waste {
	const waste: Waste = {
		rereads: 0,
		repeatedCalls: 0,
		oversizedResults: 0,
		repeatedFailures: 0,
		cacheMisses: 0,
		cacheMissIdleMinutes: [],
		compactions: 0,
	};
	for (const w of wastes) {
		waste.rereads += w.rereads;
		waste.repeatedCalls += w.repeatedCalls;
		waste.oversizedResults += w.oversizedResults;
		waste.repeatedFailures += w.repeatedFailures;
		waste.cacheMisses += w.cacheMisses;
		waste.cacheMissIdleMinutes.push(...w.cacheMissIdleMinutes);
		waste.compactions += w.compactions;
	}
	return waste;
}

/**
 * `records` with copies left out (0538: each request once), in the same order. A file whose header names a parent
 * (`parentSession`: a moved session's sibling, a `/tan` clone, a seance fork, a subagent, or a subagent transcript
 * omp copied along with a moved session's artifacts) may repeat entries another file holds; its requests, calls and
 * compactions an earlier record already counted are dropped. Records are walked parent-first: those naming no parent,
 * then the others by header time (a fork's, move's or subagent's header is written after its parent's; a copied
 * transcript keeps its original's header, and whichever of the two comes first counts). A copy keeps its request's
 * time, model and usage and its call's time, tool and arguments, which is what is compared; omp's "entries older than
 * the header" isn't a rule, since a move publishes the entry that triggered it only to the new file. A record left
 * whole is returned as is, so running this twice changes nothing.
 */
export function withoutCopies(records: readonly SessionRecord[]): SessionRecord[] {
	const order = records
		.map((record, i) => ({ record, i }))
		.sort((a, b) => {
			const aCopies = a.record.parentSession !== undefined;
			const bCopies = b.record.parentSession !== undefined;
			if (aCopies !== bCopies) return aCopies ? 1 : -1;
			if (!aCopies) return a.i - b.i;
			// No header time sorts last (Infinity - Infinity is NaN, so the input order decides).
			const later = (a.record.startedAt ?? Number.POSITIVE_INFINITY) - (b.record.startedAt ?? Number.POSITIVE_INFINITY);
			return later || a.i - b.i;
		});
	const requests = new Set<string>();
	const tools = new Set<string>();
	const compactions = new Set<number>();
	const requestKey = (r: RequestRow) => `${r.at}|${r.model}|${r.input}|${r.cacheRead}|${r.cacheWrite}|${r.output}`;
	const toolKey = (t: ToolRow) => `${t.at}|${t.tool}|${t.argHash}`;
	const result: SessionRecord[] = [...records];
	for (const { record, i } of order) {
		const copies = record.parentSession !== undefined;
		const keptRequests = copies ? record.requests.filter((r) => !requests.has(requestKey(r))) : record.requests;
		const keptTools = record.tools.flatMap((t, index) => (copies && tools.has(toolKey(t)) ? [] : [index]));
		const keptCompactions = copies ? record.compactions.filter((at) => !compactions.has(at)) : record.compactions;
		for (const r of keptRequests) requests.add(requestKey(r));
		for (const index of keptTools) tools.add(toolKey(record.tools[index] as ToolRow));
		for (const at of keptCompactions) compactions.add(at);
		if (
			keptRequests.length !== record.requests.length ||
			keptTools.length !== record.tools.length ||
			keptCompactions.length !== record.compactions.length
		) {
			result[i] = {
				...record,
				requests: keptRequests,
				tools: keptTools.map((index) => record.tools[index] as ToolRow),
				touched: keptTools.map((index) => record.touched[index] ?? []),
				compactions: keptCompactions,
			};
		}
	}
	return result;
}

/** Every session's summary with copies left out ({@link withoutCopies}), their sums (waste summed, the largest peak) and all rows. */
export function trialLedger(records: readonly SessionRecord[], pricing: LedgerPricing): TrialLedger {
	const counted = withoutCopies(records);
	const sessions = counted.map((record) => summarizeSession(record, pricing));
	const requests = counted.flatMap((record) => record.requests);
	const peak = sessions.reduce<PeakContext>((best, s) => (s.peakContext.tokens > best.tokens ? s.peakContext : best), {
		tokens: 0,
		window: null,
		percent: null,
	});
	return {
		sessions,
		totals: {
			...totalsOf(
				requests,
				sessions.reduce((n, s) => n + s.toolCalls, 0),
			),
			peakContext: peak,
			waste: sumWaste(sessions.map((s) => s.waste)),
		},
		rows: counted.flatMap((record) => [...record.requests, ...record.tools]).sort((a, b) => a.at - b.at),
	};
}

// --- The product's ledger: omp's own prices and windows, one thread's session files. ---

/** omp's prices for a model, in dollars per million tokens. */
export interface ModelCost {
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite: number;
}

interface CachedModels {
	provider: string;
	models: { id: string; contextWindow: number | null; cost: ModelCost | null }[];
}

/** `models.db`'s `model_cache` rows by the db's path, kept while the db's mtime and size stay the same (0546). */
const modelCaches = new Map<string, { mtimeMs: number; size: number; rows: CachedModels[] }>();

/** A model's `cost` as omp's model cache has it; null without finite input and output prices. */
function modelCost(value: unknown): ModelCost | null {
	const cost = asObject(value);
	if (!cost || typeof cost.input !== "number" || !Number.isFinite(cost.input)) return null;
	if (typeof cost.output !== "number" || !Number.isFinite(cost.output)) return null;
	return {
		input: cost.input,
		output: cost.output,
		cacheRead: tokenCount(cost.cacheRead),
		cacheWrite: tokenCount(cost.cacheWrite),
	};
}

/**
 * omp's model cache rows, read again when the db's mtime or size changes (omp refreshes it; a fresh install has none
 * yet). A db that can't be opened or read (missing, locked, not omp's) yields none and isn't kept, so the next call
 * tries again.
 */
function readModelCache(path: string): CachedModels[] {
	let mtimeMs: number;
	let size: number;
	try {
		({ mtimeMs, size } = statSync(path));
	} catch {
		modelCaches.delete(path);
		return [];
	}
	const cached = modelCaches.get(path);
	if (cached && cached.mtimeMs === mtimeMs && cached.size === size) return cached.rows;
	const rows: CachedModels[] = [];
	let db: Database | undefined;
	try {
		db = new Database(path, { readonly: true });
		for (const row of db
			.query<{ provider_id: unknown; models: unknown }, []>("SELECT provider_id, models FROM model_cache")
			.all()) {
			if (typeof row.provider_id !== "string" || typeof row.models !== "string") continue;
			let parsed: unknown;
			try {
				parsed = JSON.parse(row.models);
			} catch {
				continue;
			}
			const models = (Array.isArray(parsed) ? parsed : []).flatMap((model) => {
				const m = asObject(model);
				if (typeof m?.id !== "string") return [];
				const contextWindow = typeof m.contextWindow === "number" && m.contextWindow > 0 ? m.contextWindow : null;
				const cost = modelCost(m.cost);
				return contextWindow === null && cost === null ? [] : [{ id: m.id, contextWindow, cost }];
			});
			rows.push({ provider: row.provider_id, models });
		}
	} catch {
		// Missing, locked or not omp's: none this time.
		return [];
	} finally {
		db?.close();
	}
	modelCaches.set(path, { mtimeMs, size, rows });
	return rows;
}

/**
 * The first model `model` with `field` set in omp's model cache, under `provider` (`provider_id` equal to it or
 * starting with `<provider>:`), or under any provider when it is null.
 */
function cachedModel<K extends "contextWindow" | "cost">(
	provider: string | null,
	model: string,
	field: K,
): NonNullable<CachedModels["models"][number][K]> | null {
	for (const row of readModelCache(join(ompAgentDir(), "models.db"))) {
		if (provider !== null && row.provider !== provider && !row.provider.startsWith(`${provider}:`)) continue;
		const found = row.models.find((m) => m.id === model && m[field] !== null);
		if (found) return found[field] as NonNullable<CachedModels["models"][number][K]>;
	}
	return null;
}

/**
 * omp's price for `model` in dollars per million tokens, from its model cache (`<omp agent dir>/models.db`): under
 * `provider` as {@link ompPricing}'s window matches it, or the first provider listing the model when `provider` is
 * null. Null when the cache has no price for it.
 */
export function ompModelCost(provider: string | null, model: string): ModelCost | null {
	return cachedModel(provider, model, "cost");
}

/**
 * omp's own pricing: a request costs its `usage.cost.total` (null when it has none); the window is the model's
 * `contextWindow` in omp's model cache, `<omp agent dir>/models.db` (`model_cache`, read-only, read again when it
 * changes), under the request's provider (`provider_id` equal to it or starting with `<provider>:`).
 */
export const ompPricing: LedgerPricing = {
	cost(_request, usage) {
		const total = asObject(usage.cost)?.total;
		return typeof total === "number" && Number.isFinite(total) ? total : null;
	},
	window(provider, model) {
		return provider === null ? null : cachedModel(provider, model, "contextWindow");
	},
};

/** One thread's ledger: every omp session of the thread and every subagent and advisor transcript under each. */
export interface ThreadLedger {
	/** Dollars omp reported, all sessions. */
	spend: number;
	requests: number;
	/** Cache read over all input, all sessions; null without input. */
	cacheReadShare: number | null;
	/** The largest of the main sessions' own requests' peaks; null when they made none. */
	peakContext: PeakContext | null;
	/** Summed over all sessions. */
	waste: Waste;
}

interface CachedSession {
	size: number;
	mtimeMs: number;
	record: SessionRecord;
	summary: SessionSummary;
	/** Whether the agent made requests of its own (the peak means something). */
	ownRequests: boolean;
}

/** Each session file's parse, kept while its size and mtime stay the same, until {@link forgetStaleSessions}. */
const sessionCache = new Map<string, CachedSession>();

function cachedSession(path: string, agent: string): CachedSession | undefined {
	let size: number;
	let mtimeMs: number;
	try {
		({ size, mtimeMs } = statSync(path));
	} catch {
		sessionCache.delete(path);
		return undefined;
	}
	const cached = sessionCache.get(path);
	if (cached && cached.size === size && cached.mtimeMs === mtimeMs) return cached;
	let text: string;
	try {
		text = readFileSync(path, "utf8");
	} catch {
		return undefined;
	}
	const record = readOmpSession(text, ompPricing, agent);
	const entry = {
		size,
		mtimeMs,
		record,
		summary: summarizeSession(record, ompPricing),
		ownRequests: ownRequests(record).length > 0,
	};
	sessionCache.set(path, entry);
	return entry;
}

/**
 * Drops the ledger's kept parses of files last modified before `sinceMs` (epoch ms) unless `keep(file)` holds, so the
 * cache doesn't grow for the service's lifetime. `file` is a session file as passed to {@link threadLedger}, or a
 * transcript under its artifacts dir (the session file's path without `.jsonl`).
 */
export function forgetStaleSessions(sinceMs: number, keep: (file: string) => boolean = () => false): void {
	for (const [file, entry] of sessionCache) if (entry.mtimeMs < sinceMs && !keep(file)) sessionCache.delete(file);
}

/**
 * `read` over a thread's transcripts: each session file in order (agent `main`), and when it could be read, the
 * transcripts under the folder named like it without `.jsonl` (subagents, the advisor, forks; agent: the path
 * below that folder without `.jsonl`).
 */
function threadTranscripts<T>(
	sessionFiles: readonly string[],
	read: (path: string, agent: string, main: boolean) => T | undefined,
): T[] {
	const found: T[] = [];
	for (const sessionFile of sessionFiles) {
		const main = read(sessionFile, "main", true);
		if (main === undefined) continue;
		found.push(main);
		const dir = sessionFile.replace(/\.jsonl$/, "");
		for (const path of transcriptsOf(sessionFile)) {
			const agent = relative(dir, path)
				.replaceAll("\\", "/")
				.replace(/\.jsonl$/, "");
			const nested = read(path, agent, false);
			if (nested !== undefined) found.push(nested);
		}
	}
	return found;
}

/**
 * The records of the files {@link threadLedger} reads, read with `pricing` and with copies left out
 * ({@link withoutCopies}); files that can't be read are skipped. Nothing is cached.
 */
export function threadSessionRecords(sessionFiles: readonly string[], pricing: LedgerPricing): SessionRecord[] {
	return withoutCopies(
		threadTranscripts(sessionFiles, (path, agent) => {
			try {
				return readOmpSession(readFileSync(path, "utf8"), pricing, agent);
			} catch {
				return undefined;
			}
		}),
	);
}

/**
 * A thread's ledger (0538, 0546) from its omp session files, oldest first, and the transcripts under the folder named
 * like each without `.jsonl` (subagents, the advisor, forks): spend, cache-read share and waste over all of them, a
 * copy counted once ({@link withoutCopies}), and peak context the largest of the session files' own. Unchanged files
 * aren't parsed again. Undefined when none of the session files exists.
 */
export function threadLedger(sessionFiles: readonly string[]): ThreadLedger | undefined {
	let peakContext: PeakContext | null = null;
	const sessions = threadTranscripts(sessionFiles, (path, agent, main) => {
		const session = cachedSession(path, agent);
		if (main && session?.ownRequests && session.summary.peakContext.tokens > (peakContext?.tokens ?? -1)) {
			peakContext = session.summary.peakContext;
		}
		return session;
	});
	if (sessions.length === 0) return undefined;
	const counted = withoutCopies(sessions.map((s) => s.record));
	const summaries = sessions.map((s, i) => {
		const record = counted[i] as SessionRecord;
		return record === s.record ? s.summary : summarizeSession(record, ompPricing);
	});
	const sum = (key: "input" | "cacheRead" | "cacheWrite" | "cost" | "requests") =>
		summaries.reduce((total, s) => total + s[key], 0);
	const cacheRead = sum("cacheRead");
	const all = sum("input") + cacheRead + sum("cacheWrite");
	return {
		spend: sum("cost"),
		requests: sum("requests"),
		cacheReadShare: all === 0 ? null : cacheRead / all,
		peakContext,
		waste: sumWaste(summaries.map((s) => s.waste)),
	};
}

/** Tokens as the line shows them: `82k`, or `1M`/`1.5M` from a million up. */
function tokensText(tokens: number): string {
	const thousands = Math.round(tokens / 1000);
	if (thousands < 1000) return `${thousands}k`;
	return `${Math.round(tokens / 100_000) / 10}M`;
}

const counted = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * The thread's ledger line (0546):
 * `spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 repeated call · 1 cache miss · 2 compactions`.
 * Peak context reads `peak context 82k` when the window is unknown; a part with nothing to show (no input tokens
 * recorded, no cache traffic, a zero count) is left out.
 */
export function ledgerLine(ledger: ThreadLedger): string {
	const parts = [`spend $${ledger.spend.toFixed(2)}`];
	const peak = ledger.peakContext;
	if (peak && peak.tokens > 0)
		parts.push(
			peak.window
				? `peak context ${Math.round((peak.tokens / peak.window) * 100)}% (${tokensText(peak.tokens)} of ${tokensText(peak.window)})`
				: `peak context ${tokensText(peak.tokens)}`,
		);
	if (ledger.cacheReadShare !== null) parts.push(`cache read ${Math.round(ledger.cacheReadShare * 100)}%`);
	const { rereads, repeatedCalls, cacheMisses, compactions } = ledger.waste;
	if (rereads > 0) parts.push(counted(rereads, "re-read", "re-reads"));
	if (repeatedCalls > 0) parts.push(counted(repeatedCalls, "repeated call", "repeated calls"));
	if (cacheMisses > 0) parts.push(counted(cacheMisses, "cache miss", "cache misses"));
	if (compactions > 0) parts.push(counted(compactions, "compaction", "compactions"));
	return parts.join(" · ");
}
