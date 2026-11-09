import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { UserError } from "./errors.ts";
import { writeAtomically } from "./fsx.ts";
import { normalizePathKey } from "./identity.ts";
import { projectsFile } from "./paths.ts";

/** One watched root (0109). */
interface WatchedRoot {
	path: string;
	depth: number;
	ignore: string[];
}

/** `~/.jocasta/agent/projects.yaml` (0109, 0112, 0262). Keys it doesn't name (an old `scopes`) are kept untouched. */
export interface ProjectsFile {
	roots: WatchedRoot[];
	add: string[];
	exclude: string[];
	names: Record<string, string>;
}

/** A malformed projects file stops the run with one line naming it (principle 2); exit 1 (0140). */
function fail(file: string, why: string): never {
	throw new UserError(`${file}: ${why}`, 1);
}

function stringList(file: string, key: string, value: unknown): string[] {
	if (value === undefined || value === null) return [];
	if (!Array.isArray(value) || value.some((v) => typeof v !== "string")) fail(file, `${key} must be a list of paths`);
	return value;
}

/** Reads the projects file; a missing file is an empty one. A malformed file throws, naming it (principle 2). */
export function readProjectsFile(file = projectsFile()): ProjectsFile {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return { roots: [], add: [], exclude: [], names: {} };
		throw error;
	}
	const doc: unknown = Bun.YAML.parse(text) ?? {};
	if (typeof doc !== "object" || doc === null || Array.isArray(doc)) fail(file, "not a YAML mapping");
	const version = "version" in doc ? doc.version : undefined;
	if (version !== undefined && version !== 1) fail(file, `version ${String(version)} is not supported`);
	const rawRoots = "roots" in doc ? doc.roots : undefined;
	if (rawRoots !== undefined && rawRoots !== null && !Array.isArray(rawRoots)) fail(file, "roots must be a list");
	const roots = (rawRoots ?? []).map((root: unknown): WatchedRoot => {
		if (typeof root !== "object" || root === null || !("path" in root) || typeof root.path !== "string") {
			fail(file, "each root needs a path");
		}
		const depth = "depth" in root ? root.depth : 1;
		if (typeof depth !== "number" || !Number.isInteger(depth) || depth < 1)
			fail(file, "a root's depth must be 1 or more");
		const ignore = stringList(file, "ignore", "ignore" in root ? root.ignore : undefined);
		return { path: root.path, depth, ignore };
	});
	const rawNames = "names" in doc ? doc.names : undefined;
	const names: Record<string, string> = {};
	if (rawNames !== undefined && rawNames !== null) {
		if (typeof rawNames !== "object" || Array.isArray(rawNames)) fail(file, "names must map project ids to names");
		for (const [id, name] of Object.entries(rawNames)) {
			if (typeof name !== "string") fail(file, `names.${id} must be a string`);
			names[id] = name;
		}
	}
	return {
		roots,
		add: stringList(file, "add", "add" in doc ? doc.add : undefined),
		exclude: stringList(file, "exclude", "exclude" in doc ? doc.exclude : undefined),
		names,
	};
}

/** Paths as the file writes them: forward slashes, like the 0109 preview. */
function yamlScalar(path: string): string {
	const value = path.replaceAll("\\", "/");
	const plain = /^[A-Za-z0-9_./~$][^#]*$/.test(value) && !value.includes(": ") && !value.endsWith(":");
	return plain ? value : JSON.stringify(value);
}

const samePath = (a: string, b: string) => normalizePathKey(resolve(a)) === normalizePathKey(resolve(b));

interface ListBlock {
	keyLine: number;
	/** Index after the block's last line. */
	end: number;
	indent: string;
	/** Item line indexes with their values. */
	items: { line: number; value: string }[];
}

function parseItem(line: string): string | undefined {
	const parsed: unknown = Bun.YAML.parse(`x:\n${line}`);
	if (typeof parsed !== "object" || parsed === null || !("x" in parsed) || !Array.isArray(parsed.x)) return undefined;
	const [value] = parsed.x;
	return typeof value === "string" ? value : undefined;
}

/** Finds a top-level `key:` block-sequence; a flow sequence (`key: []`) is rewritten as a block first. */
function findBlock(lines: string[], key: string): ListBlock | undefined {
	const keyLine = lines.findIndex((l) => new RegExp(`^${key}:(\\s|$)`).test(l));
	if (keyLine === -1) return undefined;
	const after = lines[keyLine]?.slice(key.length + 1) ?? "";
	const flow = /^\s*(\[.*\])\s*(#.*)?$/.exec(after);
	if (flow?.[1]) {
		const parsed: unknown = Bun.YAML.parse(`x: ${flow[1]}`);
		const values =
			typeof parsed === "object" && parsed !== null && "x" in parsed && Array.isArray(parsed.x) ? parsed.x : [];
		lines.splice(
			keyLine,
			1,
			`${key}:${flow[2] ? ` ${flow[2]}` : ""}`,
			...values.map((v) => `  - ${yamlScalar(String(v))}`),
		);
	}
	const items: ListBlock["items"] = [];
	let indent = "  ";
	let end = keyLine + 1;
	for (let i = keyLine + 1; i < lines.length; i++) {
		const line = lines[i] ?? "";
		if (line.trim() === "" || /^\s+#/.test(line)) continue;
		const item = /^(\s*)- /.exec(line);
		if (!item && !/^\s/.test(line)) break;
		if (item) {
			if (items.length === 0) indent = item[1] ?? "";
			const value = parseItem(line.slice(item[1]?.length ?? 0));
			if (value !== undefined) items.push({ line: i, value });
		}
		end = i + 1;
	}
	return { keyLine, end, indent, items };
}

function edit(file: string, change: (lines: string[]) => boolean): boolean {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
		text = "version: 1\n";
	}
	const eol = text.includes("\r\n") ? "\r\n" : "\n";
	const lines = text.split(/\r?\n/);
	if (lines.at(-1) === "") lines.pop();
	if (!change(lines)) return false;
	writeAtomically(file, `${lines.join(eol)}${eol}`);
	return true;
}

/** Adds `path` to the `key` list (`add` or `exclude`), keeping the rest of the file and its comments (0109). */
export function addToList(key: "add" | "exclude", path: string, file = projectsFile()): boolean {
	return edit(file, (lines) => {
		const block = findBlock(lines, key);
		if (!block) {
			lines.push(`${key}:`, `  - ${yamlScalar(path)}`);
			return true;
		}
		if (block.items.some((item) => samePath(item.value, path))) return false;
		lines.splice(block.end, 0, `${block.indent}- ${yamlScalar(path)}`);
		return true;
	});
}

/** Removes `path` from the `key` list, keeping the rest of the file and its comments. */
export function removeFromList(key: "add" | "exclude", path: string, file = projectsFile()): boolean {
	return edit(file, (lines) => {
		const block = findBlock(lines, key);
		const matches = block?.items.filter((item) => samePath(item.value, path)) ?? [];
		for (const item of matches.reverse()) lines.splice(item.line, 1);
		return matches.length > 0;
	});
}
