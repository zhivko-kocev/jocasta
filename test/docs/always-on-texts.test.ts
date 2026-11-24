// The always-on texts (config/ponytail.md, config/boss-manual.md, AGENTS.md) name commands and paths; a stale one is
// followed whenever a task leads through it (0608). Every one still has to resolve.
import { expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..", "..");
const TEXTS = ["config/ponytail.md", "config/boss-manual.md", "AGENTS.md"];
const TOP = new Set(readdirSync(ROOT));

/** The `jocasta` command words: the variants of `enum Word` in `src/shared/route.rs`, in kebab-case. */
const COMMANDS = new Set(
	[
		...(
			readFileSync(join(ROOT, "src", "shared", "route.rs"), "utf8")
				.split("enum Word {")[1]
				?.split("\n}")[0] ?? ""
		).matchAll(/^\t([A-Z]\w*)/gm),
	].map((m) => (m[1] ?? "").replace(/(?<!^)([A-Z])/g, "-$1").toLowerCase()),
);

/**
 * What counts as a thing to resolve, by rule (a token that is none of these is prose, a placeholder or another
 * project's example and is skipped):
 * - `mise run <task>` and `bun run <script>`: the task is in mise.toml, the script in package.json;
 * - `jocasta <word>`: the word is one `src/shared/route.rs` knows (a variant of its `Word` enum);
 * - a path: only word characters, `.`, `-`, `@` and `/` (so no placeholder, glob, URL, `~`, drive or `%VAR%`), and its
 *   first segment is a top-level entry of this repo; with no `/` it also needs a file extension (so `test` is a word).
 * Fenced blocks and HTML comments are not scanned: they hold layouts and licence text, not named commands.
 */
const EXCEPTIONS: Record<string, string> = {
	// token (exact) -> reason; none needed today. Add only what no rule above can tell.
};

type Finding = { file: string; line: number; token: string; why: string };

/** The inline backtick spans of a text, with their 1-based lines; fenced blocks and comments left out. */
function spans(file: string): { line: number; token: string }[] {
	const out: { line: number; token: string }[] = [];
	let fenced = false;
	let comment = false;
	readFileSync(join(ROOT, file), "utf8")
		.split(/\r?\n/)
		.forEach((text, i) => {
			if (text.startsWith("```")) {
				fenced = !fenced;
				return;
			}
			if (comment || text.includes("<!--")) {
				comment = !text.includes("-->");
				return;
			}
			if (fenced) return;
			for (const m of text.matchAll(/`([^`]+)`/g)) out.push({ line: i + 1, token: (m[1] ?? "").trim() });
		});
	return out;
}

const miseTasks = new Set(
	[...readFileSync(join(ROOT, "mise.toml"), "utf8").matchAll(/^\[tasks\.("?)([^\]"]+)\1\]/gm)].map((m) => m[2]),
);
const scripts = new Set(
	Object.keys((JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")) as { scripts: object }).scripts),
);

/** Why `token` doesn't resolve, or undefined when it does or isn't something to resolve. */
function stale(token: string): string | undefined {
	if (token in EXCEPTIONS) return undefined;
	const run = /^(mise|bun) run ([\w:-]+(?:\|[\w:-]+)*)(?: |$)/.exec(token);
	if (run) {
		const known = run[1] === "mise" ? miseTasks : scripts;
		const missing = (run[2] ?? "").split("|").filter((name) => !known.has(name));
		return missing.length
			? `no ${run[1] === "mise" ? "mise.toml task" : "package.json script"} ${missing[0]}`
			: undefined;
	}
	const cli = /^jocasta ([a-z-]+)/.exec(token);
	if (cli) return COMMANDS.has(cli[1] ?? "") ? undefined : `route.rs has no command ${cli[1]}`;
	if (!/^[\w.@/-]+$/.test(token) || (!token.includes("/") && !/\.\w+$/.test(token))) return undefined;
	if (!TOP.has(token.split("/")[0] ?? "")) return undefined;
	return existsSync(join(ROOT, token)) ? undefined : "path doesn't exist";
}

test("every command and path the always-on texts name in backticks still resolves", () => {
	const findings: Finding[] = [];
	for (const file of TEXTS) {
		for (const { line, token } of spans(file)) {
			const why = stale(token);
			if (why) findings.push({ file, line, token, why });
		}
	}
	expect(findings.map((f) => `${f.file}:${f.line} \`${f.token}\`: ${f.why}`)).toEqual([]);
});

test("the rule tells a stale command or path from prose", () => {
	expect(stale("mise run check")).toBeUndefined();
	expect(stale("mise run no-such-task")).toContain("no-such-task");
	expect(stale("bun run typecheck")).toBeUndefined();
	expect(stale("bun run no-such-script")).toContain("no-such-script");
	expect(stale("jocasta setup")).toBeUndefined();
	expect(stale("jocasta nonsense")).toContain("nonsense");
	expect(stale("src/shared/route.rs")).toBeUndefined();
	expect(stale("jocasta push-hook")).toBeUndefined();
	expect(stale("src/shared/no-such-file.ts")).toBe("path doesn't exist");
	// Skipped by rule: placeholders, globs, URLs, home and absolute paths, other projects' names, plain words.
	for (const token of [
		"src/<x>.ts",
		"src/*.ts",
		"https://example.org/a",
		"~/src/a.ts",
		"/etc/hosts",
		"D:/x/y",
		"app/main.ts",
		"test",
		"bun test",
	])
		expect(stale(token)).toBeUndefined();
});
