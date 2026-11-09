/**
 * His omp config files as setup and uninstall read and write them (0466, 0479): YAML or JSON by the file's extension.
 * In a YAML file of flat top-level keys (`keybindings.yml`), only the lines of the keys that change are edited, so
 * his comments and formatting stay (SET-5, 0584); a file whose lines don't edit cleanly is never rewritten whole: the
 * change is refused.
 */
import { UserError } from "../shared/errors.ts";
import { asObject } from "../shared/fsx.ts";
import { ompYaml } from "./omp-settings.ts";

const isJson = (path: string) => path.toLowerCase().endsWith(".json");

/** `text` of `path` as a JSON object; anything else is refused with how to go on. */
export function parseJsonObject(path: string, text: string): Record<string, unknown> {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch (error) {
		throw new UserError(`${path} is not JSON (${(error as Error).message}); fix it and run setup again`, 1);
	}
	const object = asObject(value);
	if (!object) throw new UserError(`${path} is not a JSON object; fix it and run setup again`, 1);
	return object;
}

/** `text` of `path` as a YAML mapping (an empty file is an empty one); anything else is refused. */
function parseYamlObject(path: string, text: string): Record<string, unknown> {
	let value: unknown;
	try {
		value = text.trim() === "" ? {} : Bun.YAML.parse(text);
	} catch (error) {
		throw new UserError(`${path} is not YAML (${(error as Error).message}); fix it and run setup again`, 1);
	}
	const object = asObject(value);
	if (!object) throw new UserError(`${path} is not a YAML mapping; fix it and run setup again`, 1);
	return object;
}

/** One of his key files (`keybindings.yml`, `lsp.json`) as an object; an empty file is an empty one. */
export function parseConfig(path: string, text: string): Record<string, unknown> {
	if (!isJson(path)) return parseYamlObject(path, text);
	return text.trim() === "" ? {} : parseJsonObject(path, text);
}

/** `value` as the whole file: YAML as omp writes it, or JSON. */
export function renderConfig(path: string, value: Record<string, unknown>): string {
	return isJson(path) ? `${JSON.stringify(value, null, 2)}\n` : ompYaml(value);
}

/** A top-level key on its line: plain, or quoted. */
const KEY_LINE = /^(?:"((?:[^"\\]|\\.)*)"|'((?:[^']|'')*)'|([^\s#'"\-?:][^#]*?))\s*:(?:\s|$)/;

/** The key a top-level line starts, or undefined (a comment, a blank, a value's line). */
function keyOf(line: string): string | undefined {
	const found = KEY_LINE.exec(line);
	if (!found) return undefined;
	if (found[1] !== undefined) return JSON.parse(`"${found[1]}"`) as string;
	if (found[2] !== undefined) return found[2].replaceAll("''", "'");
	return found[3];
}

/** Whether `line` belongs to the key above it: indented, or a list item at the key's own indent. */
const continues = (line: string) => /^[ \t]/.test(line) || /^-(\s|$)/.test(line);

/**
 * `text` with the top-level keys in `set` written as `expected` has them (a key's lines replaced, a new key appended)
 * and the keys in `remove` dropped with their lines; every other line stays as it was. `expected` is what the file
 * must read as afterwards: a JSON file (no comments to keep) is written whole from it; YAML whose edited lines don't
 * read as it gives undefined, and the caller refuses the change rather than rewrite his file (0584).
 */
export function editConfig(
	path: string,
	text: string,
	expected: Record<string, unknown>,
	set: string[],
	remove: string[],
): string | undefined {
	if (isJson(path)) return renderConfig(path, expected);
	const eol = text.includes("\r\n") ? "\r\n" : "\n";
	const lines = text.split(/\r?\n/);
	const rendered = (key: string) =>
		ompYaml({ [key]: expected[key] })
			.replace(/\n$/, "")
			.split("\n");
	const out: string[] = [];
	const placed = new Set<string>();
	for (let i = 0; i < lines.length; ) {
		const line = lines[i] as string;
		const key = keyOf(line);
		let end = i + 1;
		if (key !== undefined) while (end < lines.length && continues(lines[end] as string)) end++;
		if (key !== undefined && remove.includes(key)) {
			i = end;
			continue;
		}
		if (key !== undefined && set.includes(key) && !placed.has(key)) {
			out.push(...rendered(key));
			placed.add(key);
			i = end;
			continue;
		}
		out.push(...lines.slice(i, end));
		i = end;
	}
	const added = set.filter((key) => !placed.has(key)).flatMap(rendered);
	if (added.length > 0) {
		// Before the file's last line break; a file without one gets it.
		if (out.at(-1) === "") out.splice(out.length - 1, 0, ...added);
		else out.push(...added, "");
	}
	const edited = out.join(eol);
	let reads: unknown;
	try {
		// A file left with only his comments reads as an empty mapping.
		reads = edited.trim() === "" ? {} : (Bun.YAML.parse(edited) ?? {});
	} catch {
		return undefined;
	}
	return Bun.deepEquals(reads, expected, true) ? edited : undefined;
}
