import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { jsonLines } from "../../src/shared/jsonl.ts";
import {
	type LedgerHarness,
	type LedgerPricing,
	newRecord,
	pushTool,
	readOmpSession,
	requestRow,
	resultText,
	type SessionRecord,
	type TrialLedger,
	timeOf,
	tokenCount,
	trialLedger,
	utf8Bytes,
} from "../../src/shared/ledger.ts";
import { type PriceTable, priceRequest } from "./prices.ts";

/**
 * The benchmark's side of the cost ledger (0538, 0546; the rows, waste and peak are `src/shared/ledger.ts`'s): each
 * trial's saved records, read once at its end: omp's session files (`.jsonl`, subagents' files beside the main one),
 * Claude Code's transcripts (`.jsonl`, subagents under `<session>/subagents/`) and opencode's `opencode export` JSON,
 * priced at the pinned table.
 */

/** The ledger's pricing from the pinned table: a request at {@link priceRequest}, the window its `contextWindow`. */
export function benchPricing(prices: PriceTable): LedgerPricing {
	return {
		cost(request) {
			const price = prices.lookup(request.provider, request.model);
			return price ? priceRequest(price, request) : null;
		},
		window: (provider, model) => prices.lookup(provider, model)?.contextWindow ?? null,
	};
}

/**
 * One Claude Code transcript: an API response is written as several `assistant` lines sharing `message.id`, each with
 * the response's usage, so a request is one id (its last line's usage); `<synthetic>` messages aren't requests.
 * `tool_result` blocks in user lines carry results; a `compact_boundary` system line is a compaction.
 */
export function readClaudeTranscript(text: string, pricing: LedgerPricing, agent = "main"): SessionRecord {
	const entries = jsonLines(text);
	const sessionId = entries.find((entry) => typeof entry.sessionId === "string")?.sessionId;
	const record = newRecord("claude", typeof sessionId === "string" ? sessionId : agent, agent);
	const results = new Map<string, { bytes: number; error: boolean }>();
	const byId = new Map<string, { at: number; message: Record<string, unknown> }>();
	const calls: { at: number; id: string; tool: string; args: unknown }[] = [];
	for (const entry of entries) {
		const at = timeOf(entry.timestamp);
		const message = entry.message as Record<string, unknown> | undefined;
		if (entry.type === "system" && entry.subtype === "compact_boundary") record.compactions.push(at);
		if (entry.type === "user" && Array.isArray(message?.content))
			for (const block of message.content as Record<string, unknown>[])
				if (block?.type === "tool_result" && typeof block.tool_use_id === "string")
					results.set(block.tool_use_id, {
						bytes: utf8Bytes(resultText(block.content)),
						error: block.is_error === true,
					});
		if (entry.type !== "assistant" || !message || message.model === "<synthetic>") continue;
		const id = typeof message.id === "string" ? message.id : `${at}`;
		const first = byId.get(id);
		byId.set(id, { at: first?.at ?? at, message });
		for (const block of Array.isArray(message.content) ? (message.content as Record<string, unknown>[]) : [])
			if (block?.type === "tool_use" && typeof block.id === "string")
				calls.push({ at, id: block.id, tool: String(block.name ?? "unknown"), args: block.input });
	}
	for (const { at, message } of byId.values()) {
		const usage = (message.usage ?? {}) as Record<string, unknown>;
		const creation = (usage.cache_creation ?? {}) as Record<string, unknown>;
		record.requests.push(
			requestRow(
				record,
				pricing,
				{
					at,
					provider: "anthropic",
					model: String(message.model ?? "unknown"),
					input: tokenCount(usage.input_tokens),
					cacheRead: tokenCount(usage.cache_read_input_tokens),
					cacheWrite: tokenCount(usage.cache_creation_input_tokens),
					cacheWrite1h: tokenCount(creation.ephemeral_1h_input_tokens),
					output: tokenCount(usage.output_tokens),
				},
				usage,
			),
		);
	}
	for (const call of calls) {
		const result = results.get(call.id);
		pushTool(record, { ...call, resultBytes: result?.bytes ?? null, error: result?.error ?? false });
	}
	record.requests.sort((a, b) => a.at - b.at);
	return record;
}

/**
 * One `opencode export <session>` JSON: each `step-finish` part of an assistant message is a model request (output
 * plus reasoning tokens count as output), timed at the latest part end before it or the message's start; `tool` parts
 * are calls, `compaction` parts compactions.
 */
export function readOpencodeExport(text: string, pricing: LedgerPricing, agent = "main"): SessionRecord {
	const exported = JSON.parse(text) as {
		info?: { id?: string };
		messages?: { info?: Record<string, unknown>; parts?: Record<string, unknown>[] }[];
	};
	const record = newRecord("opencode", exported.info?.id ?? agent, agent);
	for (const message of exported.messages ?? []) {
		const info = message.info ?? {};
		const time = (info.time ?? {}) as Record<string, unknown>;
		let clock = timeOf(time.created);
		for (const part of message.parts ?? []) {
			const partTime = (part.time ?? {}) as Record<string, unknown>;
			const state = (part.state ?? {}) as Record<string, unknown>;
			const stateTime = (state.time ?? {}) as Record<string, unknown>;
			if (part.type === "compaction") record.compactions.push(clock);
			if (part.type === "tool") {
				const output =
					typeof state.output === "string" ? state.output : typeof state.error === "string" ? state.error : null;
				pushTool(record, {
					at: Number.isFinite(timeOf(stateTime.start)) ? timeOf(stateTime.start) : clock,
					tool: String(part.tool ?? "unknown"),
					args: state.input,
					resultBytes: output === null ? null : utf8Bytes(output),
					error: state.status === "error",
				});
			}
			if (part.type === "step-finish" && info.role === "assistant") {
				const tokens = (part.tokens ?? {}) as Record<string, unknown>;
				const cache = (tokens.cache ?? {}) as Record<string, unknown>;
				record.requests.push(
					requestRow(
						record,
						pricing,
						{
							at: clock,
							provider: typeof info.providerID === "string" ? info.providerID : null,
							model: String(info.modelID ?? "unknown"),
							input: tokenCount(tokens.input),
							cacheRead: tokenCount(cache.read),
							cacheWrite: tokenCount(cache.write),
							cacheWrite1h: 0,
							output: tokenCount(tokens.output) + tokenCount(tokens.reasoning),
						},
						tokens,
					),
				);
			}
			for (const end of [timeOf(partTime.end), timeOf(stateTime.end)])
				if (Number.isFinite(end) && end > clock) clock = end;
		}
	}
	return record;
}

function filesUnder(dir: string, extension: string): string[] {
	if (!existsSync(dir)) return [];
	return readdirSync(dir, { recursive: true, encoding: "utf8" })
		.map((name) => join(dir, name))
		.filter((path) => extname(path) === extension && statSync(path).isFile())
		.sort();
}

/**
 * The sessions saved for one trial in `dir`: omp's and Claude Code's `.jsonl` files (a file in a subfolder is a
 * subagent, named by its path), opencode's exported `.json` files. Files that aren't a session are skipped.
 */
export function readTrialSessions(dir: string, harness: LedgerHarness, pricing: LedgerPricing): SessionRecord[] {
	const records: SessionRecord[] = [];
	const agentOf = (path: string) => {
		const rel = relative(dir, path).replaceAll("\\", "/");
		return rel.includes("/") ? rel.replace(/\.jsonl?$/, "") : "main";
	};
	if (harness === "opencode") {
		for (const path of filesUnder(dir, ".json"))
			records.push(readOpencodeExport(readFileSync(path, "utf8"), pricing, agentOf(path)));
		return records;
	}
	for (const path of filesUnder(dir, ".jsonl")) {
		const text = readFileSync(path, "utf8");
		const record =
			harness === "omp"
				? readOmpSession(text, pricing, agentOf(path))
				: readClaudeTranscript(text, pricing, agentOf(path));
		if (record.requests.length > 0 || record.tools.length > 0) records.push(record);
	}
	return records;
}

/** A trial's ledger from its saved sessions folder. */
export const ledgerOfDir = (dir: string, harness: LedgerHarness, pricing: LedgerPricing): TrialLedger =>
	trialLedger(readTrialSessions(dir, harness, pricing), pricing);
