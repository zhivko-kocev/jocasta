import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { REPO_ROOT } from "./args.ts";

/**
 * The benchmark's one price table (0538): LiteLLM's `model_prices_and_context_window.json`, fetched once and committed
 * under `bench/prices/` with its source (`source.json`: URL, commit, date, SHA-256). Every arm's tokens are priced with
 * it, so omp, Claude Code and opencode compare on one bill; omp's own `usage.cost` stays the product's figure. Cache
 * read and cache write are priced apart, a 1-hour cache write at its own rate, and models with a 200k tier at their
 * higher rates past 200k prompt tokens.
 */

export const PRICES_DIR = join(REPO_ROOT, "bench", "prices");
export const PRICE_TABLE_FILE = join(PRICES_DIR, "model_prices_and_context_window.json");
export const PRICE_SOURCE_FILE = join(PRICES_DIR, "source.json");

/** Where the committed table came from. */
export interface PriceSource {
	url: string;
	commit: string;
	commitDate: string;
	fetchedAt: string;
	sha256: string;
}

/** One model's prices in dollars per token, and its context window in tokens. */
export interface ModelPrice {
	/** The table's key it was found under. */
	key: string;
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite: number;
	/** A 1-hour cache write; the 5-minute rate when the table has none. */
	cacheWrite1h: number;
	/** Rates past {@link TIER_TOKENS} prompt tokens, when the table has them. */
	above200k: { input: number; output: number; cacheRead: number; cacheWrite: number } | null;
	contextWindow: number | null;
}

/** One request's tokens as the ledger has them; `cacheWrite1h` is the part of `cacheWrite` written for an hour. */
export interface RequestTokens {
	input: number;
	cacheRead: number;
	cacheWrite: number;
	cacheWrite1h: number;
	output: number;
}

export const TIER_TOKENS = 200_000;

export interface PriceTable {
	source: PriceSource;
	/** The model's prices; null when the table doesn't have it. */
	lookup(provider: string | null, model: string): ModelPrice | null;
}

type Entry = Record<string, unknown>;

const num = (value: unknown): number | undefined =>
	typeof value === "number" && Number.isFinite(value) ? value : undefined;

function toPrice(key: string, entry: Entry): ModelPrice | null {
	const input = num(entry.input_cost_per_token);
	const output = num(entry.output_cost_per_token);
	if (input === undefined || output === undefined) return null;
	const cacheRead = num(entry.cache_read_input_token_cost) ?? input;
	const cacheWrite = num(entry.cache_creation_input_token_cost) ?? input;
	const tierInput = num(entry.input_cost_per_token_above_200k_tokens);
	const tierOutput = num(entry.output_cost_per_token_above_200k_tokens);
	return {
		key,
		input,
		output,
		cacheRead,
		cacheWrite,
		cacheWrite1h: num(entry.cache_creation_input_token_cost_above_1hr) ?? cacheWrite,
		above200k:
			tierInput === undefined
				? null
				: {
						input: tierInput,
						output: tierOutput ?? output,
						cacheRead: num(entry.cache_read_input_token_cost_above_200k_tokens) ?? cacheRead,
						cacheWrite: num(entry.cache_creation_input_token_cost_above_200k_tokens) ?? cacheWrite,
					},
		contextWindow: num(entry.max_input_tokens) ?? null,
	};
}

/**
 * The model names tried, in order: `<provider>/<model>`, the model as given, and its last path segment, each without a
 * bracketed suffix such as Claude Code's `[1m]`. No fuzzy matching: an unknown model stays unpriced and is reported.
 */
export function candidateKeys(provider: string | null, model: string): string[] {
	const bare = model.replace(/\[[^\]]*\]$/, "").trim();
	const last = bare.includes("/") ? (bare.split("/").at(-1) ?? bare) : bare;
	return [...new Set([...(provider ? [`${provider}/${last}`] : []), bare, last])];
}

/** A table from parsed JSON (tests build small ones). */
export function priceTableFrom(json: Record<string, unknown>, source: PriceSource): PriceTable {
	const cache = new Map<string, ModelPrice | null>();
	return {
		source,
		lookup(provider, model) {
			const id = `${provider ?? ""}\u0000${model}`;
			if (cache.has(id)) return cache.get(id) ?? null;
			let found: ModelPrice | null = null;
			for (const key of candidateKeys(provider, model)) {
				const entry = json[key];
				if (typeof entry !== "object" || entry === null) continue;
				found = toPrice(key, entry as Entry);
				if (found) break;
			}
			cache.set(id, found);
			return found;
		},
	};
}

let loaded: PriceTable | undefined;

/** The committed table; throws when its SHA-256 isn't the one `source.json` pins. Read once per process. */
export function loadPriceTable(): PriceTable {
	if (loaded) return loaded;
	const source = JSON.parse(readFileSync(PRICE_SOURCE_FILE, "utf8")) as PriceSource;
	const bytes = readFileSync(PRICE_TABLE_FILE);
	const sha256 = createHash("sha256").update(bytes).digest("hex");
	if (sha256 !== source.sha256)
		throw new Error(`${PRICE_TABLE_FILE} has SHA-256 ${sha256}, not the pinned ${source.sha256} (source.json)`);
	loaded = priceTableFrom(JSON.parse(bytes.toString("utf8")) as Record<string, unknown>, source);
	return loaded;
}

/** One request's dollars at `price`; the 200k tier applies when the prompt (input + cache read + cache write) passes it. */
export function priceRequest(price: ModelPrice, tokens: RequestTokens): number {
	const prompt = tokens.input + tokens.cacheRead + tokens.cacheWrite;
	const tier = price.above200k !== null && prompt > TIER_TOKENS ? price.above200k : null;
	const fiveMinute = Math.max(0, tokens.cacheWrite - tokens.cacheWrite1h);
	return (
		tokens.input * (tier?.input ?? price.input) +
		tokens.output * (tier?.output ?? price.output) +
		tokens.cacheRead * (tier?.cacheRead ?? price.cacheRead) +
		fiveMinute * (tier?.cacheWrite ?? price.cacheWrite) +
		tokens.cacheWrite1h * price.cacheWrite1h
	);
}
