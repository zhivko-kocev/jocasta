import type { Static, TSchema } from "typebox";
import { Value } from "typebox/value";

/** A problem in one of Jocasta's YAML files, named by file and 1-based line. */
export class YamlFileError extends Error {
	constructor(
		readonly file: string,
		readonly line: number,
		readonly problem: string,
	) {
		super(`${file}:${line}: ${problem}`);
		this.name = "YamlFileError";
	}
}

/** Path segments into a parsed document: keys and list indexes. */
export type YamlPath = (string | number)[];

interface YamlDocument<T> {
	value: T;
	/** The 1-based line that best matches `path` in the source (block-style YAML; flow style falls back to its parent). */
	lineOf(path: YamlPath): number;
}

/**
 * Parses YAML with `Bun.YAML` and validates it against `schema`. Bun's parser reports no position, so a syntax error is
 * placed on the line after the longest prefix of the file that still parses, and a schema error on the line its path
 * reaches in the source.
 */
export function parseYamlFile<S extends TSchema>(schema: S, source: string, file: string): YamlDocument<Static<S>> {
	const lines = source.split(/\r?\n/);
	let value: unknown;
	try {
		value = Bun.YAML.parse(source);
	} catch (error) {
		throw new YamlFileError(file, syntaxErrorLine(lines), (error as Error).message.replace(/^YAML Parse error: /, ""));
	}
	const lineOf = (path: YamlPath) => locate(lines, path);
	const errors = [...Value.Errors(schema, value)];
	// An unknown key yields a bare "schema is false" for the key and a clearer error for its parent; report the latter.
	const first = errors.find((e) => e.keyword !== "boolean") ?? errors[0];
	if (first) {
		const path = first.instancePath
			.split("/")
			.slice(1)
			.map((part) => (/^\d+$/.test(part) ? Number(part) : part.replace(/~1/g, "/").replace(/~0/g, "~")));
		const extra = first.keyword === "additionalProperties" ? (first.params as { additionalProperties?: string[] }) : {};
		const unknownKey = extra.additionalProperties?.[0];
		if (unknownKey !== undefined) {
			throw new YamlFileError(
				file,
				lineOf([...path, unknownKey]),
				`unknown key \`${unknownKey}\` in ${describe(path)}`,
			);
		}
		throw new YamlFileError(file, lineOf(path), `${describe(path)} ${first.message}`);
	}
	return { value: value as Static<S>, lineOf };
}

/** `turns[1].tool_calls[0]`, or `the file` for the root. */
export function describe(path: YamlPath): string {
	if (path.length === 0) return "the file";
	return path.map((part, i) => (typeof part === "number" ? `[${part}]` : i === 0 ? part : `.${part}`)).join("");
}

function syntaxErrorLine(lines: string[]): number {
	for (let n = lines.length - 1; n >= 0; n--) {
		try {
			Bun.YAML.parse(lines.slice(0, n).join("\n"));
			return n + 1;
		} catch {}
	}
	return 1;
}

interface Block {
	/** Index of the line where the block's node starts. */
	at: number;
	/** First line after the block. */
	end: number;
	/** Column of the node's own key or dash; children sit to the right of it. */
	column: number;
}

/** Walks block-style YAML by indentation to the line of `path`; stops at the deepest segment it can find. */
function locate(lines: string[], path: YamlPath): number {
	// The root starts before line 0, so a root-level list's first item is found too.
	let block: Block = { at: -1, end: lines.length, column: -1 };
	let found = Math.max(
		0,
		lines.findIndex((line) => !isBlank(line)),
	);
	for (const segment of path) {
		const next = typeof segment === "number" ? findItem(lines, block, segment) : findKey(lines, block, segment);
		if (!next) break;
		block = next;
		found = next.at;
	}
	return found + 1;
}

function isBlank(line: string): boolean {
	const trimmed = line.trim();
	return trimmed === "" || trimmed.startsWith("#") || trimmed === "---";
}

/**
 * The lines inside a node that starts at `at` with its key or dash at `column`. A key's list may sit at the key's own
 * column (`turns:\n- text: a`).
 */
function blockEnd(lines: string[], at: number, column: number, limit: number, isKey: boolean): number {
	let end = at + 1;
	for (; end < limit; end++) {
		const line = lines[end] ?? "";
		if (isBlank(line) || indentOf(line) > column) continue;
		if (isKey && indentOf(line) === column && /^\s*-(\s|$)/.test(line)) continue;
		break;
	}
	return end;
}

function indentOf(line: string): number {
	return line.length - line.trimStart().length;
}

function findKey(lines: string[], block: Block, key: string): Block | undefined {
	const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	const pattern = new RegExp(`^(\\s*(?:-\\s+)*)(["']?)${escaped}\\2\\s*:(\\s|$)`);
	for (let i = block.at; i < block.end; i++) {
		const line = lines[i] ?? "";
		const match = pattern.exec(line);
		if (!match) continue;
		const column = (match[1] ?? "").length;
		if (column <= block.column) continue;
		return { at: i, end: blockEnd(lines, i, column, block.end, true), column };
	}
	return undefined;
}

function findItem(lines: string[], block: Block, index: number): Block | undefined {
	// Items of this list are the dashes at the smallest column right of the parent, after the parent's own line.
	const items: number[] = [];
	let column = Number.POSITIVE_INFINITY;
	for (let i = block.at + 1; i < block.end; i++) {
		const line = lines[i] ?? "";
		if (!/^\s*-(\s|$)/.test(line)) continue;
		const dash = indentOf(line);
		if (dash < block.column) continue;
		if (dash < column) {
			column = dash;
			items.length = 0;
		}
		if (dash === column) items.push(i);
	}
	const at = items[index];
	if (at === undefined) return undefined;
	return { at, end: blockEnd(lines, at, column, block.end, false), column };
}
