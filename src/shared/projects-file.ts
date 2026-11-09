import { readFileSync } from "node:fs";
import { type Static, Type } from "typebox";
import { UserError } from "./errors.ts";
import { writeAtomically } from "./fsx.ts";
import { samePath } from "./pathkey.ts";
import { projectsFile } from "./paths.ts";
import { parseYamlFile, YamlFileError } from "./yaml.ts";

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

/** A list key may be left empty (`add:`), which YAML reads as null. */
const PathList = Type.Union([Type.Array(Type.String()), Type.Null()]);

/**
 * The file's shape (0109): the mapping first, so a list or a scalar is told it must be a mapping. A root's other keys
 * and the file's other keys (an old `scopes`) are allowed and left alone.
 */
const ProjectsFileSchema = Type.Union([
	Type.Object({
		version: Type.Optional(Type.Literal(1)),
		roots: Type.Optional(
			Type.Union([
				Type.Array(
					Type.Object({
						path: Type.String(),
						depth: Type.Optional(Type.Integer({ minimum: 1 })),
						ignore: Type.Optional(PathList),
					}),
				),
				Type.Null(),
			]),
		),
		add: Type.Optional(PathList),
		exclude: Type.Optional(PathList),
		names: Type.Optional(Type.Union([Type.Record(Type.String(), Type.String()), Type.Null()])),
	}),
	Type.Null(),
]);

/**
 * Reads the projects file; a missing file is an empty one. A malformed file stops the run with one line naming the
 * file and line, `<file>:<line>: <reason>` (principle 2), exit 1 (0140, 0582).
 */
export function readProjectsFile(file = projectsFile()): ProjectsFile {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return { roots: [], add: [], exclude: [], names: {} };
		throw error;
	}
	let doc: Static<typeof ProjectsFileSchema>;
	try {
		doc = parseYamlFile(ProjectsFileSchema, text, file).value;
	} catch (error) {
		if (error instanceof YamlFileError) throw new UserError(error.message, 1);
		throw error;
	}
	return {
		roots: (doc?.roots ?? []).map((root) => ({ path: root.path, depth: root.depth ?? 1, ignore: root.ignore ?? [] })),
		add: doc?.add ?? [],
		exclude: doc?.exclude ?? [],
		names: doc?.names ?? {},
	};
}

/** Paths as the file writes them: forward slashes, like the 0109 preview. */
function yamlScalar(path: string): string {
	const value = path.replaceAll("\\", "/");
	const plain = /^[A-Za-z0-9_./~$][^#]*$/.test(value) && !value.includes(": ") && !value.endsWith(":");
	return plain ? value : JSON.stringify(value);
}

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
