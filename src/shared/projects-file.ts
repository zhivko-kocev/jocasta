import { readFileSync } from "node:fs";
import { type Static, Type } from "typebox";
import { UserError } from "./errors.ts";
import { writeAtomically } from "./fsx.ts";
import { samePath } from "./pathkey.ts";
import { projectsFile } from "./paths.ts";
import { parseYamlFile, YamlFileError, type YamlPath } from "./yaml.ts";

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
	/** What each repo (its main checkout's folder, as the file writes it) answered about pushes to trunk (0613), with its line. */
	push: Record<string, { way: PushWay; line: number }>;
}

/** How a repo takes changes to its protected branches (0613): pushes straight onto them, or pull requests. */
export type PushWay = "direct" | "pull-requests";

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
		push: Type.Optional(
			Type.Union([
				Type.Record(Type.String(), Type.Union([Type.Literal("direct"), Type.Literal("pull-requests")])),
				Type.Null(),
			]),
		),
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
		if ((error as NodeJS.ErrnoException).code === "ENOENT") {
			return { roots: [], add: [], exclude: [], names: {}, push: {} };
		}
		throw error;
	}
	let doc: Static<typeof ProjectsFileSchema>;
	let lineOf: (path: YamlPath) => number;
	try {
		({ value: doc, lineOf } = parseYamlFile(ProjectsFileSchema, text, file));
	} catch (error) {
		if (error instanceof YamlFileError) throw new UserError(error.message, 1);
		throw error;
	}
	return {
		roots: (doc?.roots ?? []).map((root) => ({ path: root.path, depth: root.depth ?? 1, ignore: root.ignore ?? [] })),
		add: doc?.add ?? [],
		exclude: doc?.exclude ?? [],
		names: doc?.names ?? {},
		push: Object.fromEntries(
			Object.entries(doc?.push ?? {}).map(([repo, way]) => [repo, { way, line: lineOf(["push", repo]) }]),
		),
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

/**
 * Records `way` for `repo` in the `push:` map (0613), keeping the rest of the file, its comments and its order: an
 * entry for the same folder is changed in place, a new one goes at the map's end, and a file without the map gets it
 * at its end. False when the file already says so.
 */
export function setPushWay(repo: string, way: PushWay, file = projectsFile()): boolean {
	return edit(file, (lines) => {
		const entry = `${yamlScalar(repo)}: ${way}`;
		const keyLine = lines.findIndex((l) => /^push:(\s|$)/.test(l));
		if (keyLine === -1) {
			lines.push("push:", `  ${entry}`);
			return true;
		}
		// A flow map (`push: {}`) is rewritten as a block first, as `findBlock` does for a list.
		const flow = /^push:\s*(\{.*\})\s*(#.*)?$/.exec(lines[keyLine] ?? "");
		if (flow?.[1]) {
			const parsed: unknown = Bun.YAML.parse(`x: ${flow[1]}`);
			const map = typeof parsed === "object" && parsed !== null && "x" in parsed ? parsed.x : undefined;
			const rows = Object.entries((map ?? {}) as Record<string, unknown>);
			lines.splice(
				keyLine,
				1,
				`push:${flow[2] ? ` ${flow[2]}` : ""}`,
				...rows.map(([key, value]) => `  ${yamlScalar(key)}: ${String(value)}`),
			);
		}
		let end = keyLine + 1;
		let indent = "  ";
		for (let i = keyLine + 1; i < lines.length; i++) {
			const line = lines[i] ?? "";
			if (line.trim() === "" || /^\s+#/.test(line)) continue;
			if (!/^\s/.test(line)) break;
			indent = /^\s*/.exec(line)?.[0] ?? indent;
			end = i + 1;
			const parsed: unknown = Bun.YAML.parse(line.trim());
			const key = typeof parsed === "object" && parsed !== null ? Object.keys(parsed)[0] : undefined;
			if (key === undefined || !samePath(key, repo)) continue;
			const old = /:\s+(direct|pull-requests)(\s+#.*)?$/.exec(line);
			if (old?.[1] === way) return false;
			lines[i] = `${indent}${entry}${old?.[2] ?? ""}`;
			return true;
		}
		lines.splice(end, 0, `${indent}${entry}`);
		return true;
	});
}
