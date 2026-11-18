// Process rules checked by machine instead of by hand (0608): the decisions index equals the record files and names
// every record that amended each one (0622), every JOCASTA_* variable in src/ (Rust) is in plan appendix I, and Bun's version is the same in its three
// files.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "..");
const read = (file: string) => readFileSync(join(ROOT, file), "utf8");

/** The 1-based line of the first match of `re` in `text`. */
const lineOf = (text: string, index: number) => text.slice(0, index).split("\n").length;

test("the decisions index has exactly one row per record file, and every row's file exists", () => {
	const files = readdirSync(join(ROOT, "docs/decisions")).filter((f) => /^\d{4}-.+\.md$/.test(f));
	const rows = new Map<string, number[]>();
	read("docs/decisions/README.md")
		.split(/\r?\n/)
		.forEach((text, i) => {
			const target = /^\| \[\d{4}\]\(([^)]+)\)/.exec(text)?.[1];
			if (target) rows.set(target, [...(rows.get(target) ?? []), i + 1]);
		});
	const problems: string[] = [];
	for (const file of files) if (!rows.has(file)) problems.push(`docs/decisions/README.md: no row for \`${file}\``);
	for (const [target, lines] of rows) {
		if (lines.length > 1)
			problems.push(`docs/decisions/README.md:${lines[1]} \`${target}\`: second row for one record`);
		if (!files.includes(target)) problems.push(`docs/decisions/README.md:${lines[0]} \`${target}\`: no such file`);
	}
	expect(problems).toEqual([]);
});

/**
 * Who amended each record, from the records' own text (0622): on a line that is not an options-table row, each
 * "amends" up to the clause's end (`;` or a sentence's `. `) names the 4-digit ids in it that are record files, other
 * than the record's own.
 */
function amenders(files: readonly string[]): Map<string, Set<string>> {
	const ids = new Set(files.map((file) => file.slice(0, 4)));
	const found = new Map<string, Set<string>>();
	for (const file of files) {
		const own = file.slice(0, 4);
		for (const line of read(`docs/decisions/${file}`).split(/\r?\n/)) {
			if (line.trimStart().startsWith("|")) continue;
			for (const m of line.matchAll(/\bamends\b/gi)) {
				const clause = line.slice(m.index).split(/;|\.\s/)[0] ?? "";
				for (const id of clause.match(/\b\d{4}\b/g) ?? []) {
					if (id === own || !ids.has(id)) continue;
					found.set(id, (found.get(id) ?? new Set()).add(own));
				}
			}
		}
	}
	return found;
}

test("each index row's Amended by names every record whose text amends it (0622)", () => {
	const files = readdirSync(join(ROOT, "docs/decisions")).filter((f) => /^\d{4}-.+\.md$/.test(f));
	const named = new Map<string, Set<string>>();
	const lines = new Map<string, number>();
	read("docs/decisions/README.md")
		.split(/\r?\n/)
		.forEach((text, i) => {
			const id = /^\| \[(\d{4})\]\(/.exec(text)?.[1];
			if (!id) return;
			named.set(
				id,
				new Set(
					text
						.split("|")
						.at(-2)
						?.match(/\b\d{4}\b/g) ?? [],
				),
			);
			lines.set(id, i + 1);
		});
	const found = amenders(files);
	// A known link, so a reader that finds nothing can't pass: 0624 amends 0614 in its Consequences.
	expect(found.get("0614")?.has("0624")).toBe(true);
	const problems: string[] = [];
	for (const [id, by] of found) {
		const missing = [...by].filter((amender) => !named.get(id)?.has(amender)).sort();
		if (missing.length > 0)
			problems.push(`docs/decisions/README.md:${lines.get(id) ?? 0} ${id}: Amended by lacks ${missing.join(", ")}`);
	}
	expect(problems).toEqual([]);
});

/** The Rust files under `dir`, the unit tests' own files (`*_tests.rs`) left out: only the product's names count. */
function sources(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
		e.isDirectory()
			? sources(join(dir, e.name))
			: e.name.endsWith(".rs") && !e.name.endsWith("_tests.rs")
				? [join(dir, e.name)]
				: [],
	);
}

/** The names the Rust unit tests' stand-ins and probes read (`JOCASTA_TEST_*`, `JOCASTA_TESTING_*`, `JOCASTA_FAKE_*`) are no setting. */
const TEST_ONLY = /^JOCASTA_(?:TEST|TESTING|FAKE)_/;

test("every JOCASTA_* variable named in src/ (Rust) is in plan appendix I", () => {
	const plan = read("docs/plan.md");
	const start = plan.search(/^## Appendix I\b/m);
	expect(start).toBeGreaterThan(-1);
	const rest = plan.slice(start + 1);
	const end = rest.search(/^## /m);
	const appendix = end < 0 ? rest : rest.slice(0, end);
	const known = new Set(appendix.match(/JOCASTA_[A-Z0-9_]*[A-Z0-9]/g));
	const problems: string[] = [];
	const seen = new Set<string>();
	for (const file of sources(join(ROOT, "src"))) {
		const text = readFileSync(file, "utf8");
		// A name the file declares as a code identifier (`const JOCASTA_EXE: &str = …`) is not an environment variable.
		const declared = new Set(
			[...text.matchAll(/\b(?:const|static|let|fn|type|struct|enum)\s+(JOCASTA_[A-Z0-9_]*[A-Z0-9])/g)].map((m) => m[1]),
		);
		for (const m of text.matchAll(/JOCASTA_[A-Z0-9_]*[A-Z0-9]/g)) {
			if (known.has(m[0]) || seen.has(m[0]) || declared.has(m[0]) || TEST_ONLY.test(m[0])) continue;
			seen.add(m[0]);
			problems.push(
				`${relative(ROOT, file).replaceAll("\\", "/")}:${lineOf(text, m.index)} \`${m[0]}\`: not in plan appendix I`,
			);
		}
	}
	expect(problems).toEqual([]);
});

test("Bun's version is the same in package.json and mise.toml, and CI takes it from mise", () => {
	const pick = (file: string, re: RegExp) => {
		const text = read(file);
		const m = re.exec(text);
		return { file, line: m ? lineOf(text, m.index) : 0, version: m?.[1] };
	};
	const found = [pick("package.json", /"packageManager":\s*"bun@([^"]+)"/), pick("mise.toml", /^bun = "([^"]+)"/m)];
	for (const f of found) expect(f.version, `${f.file} names no Bun version`).toBeDefined();
	const versions = new Set(found.map((f) => f.version));
	expect(
		versions.size === 1 ? [] : found.map((f) => `${f.file}:${f.line} \`${f.version}\``),
		"Bun's version differs between files",
	).toEqual([]);
	// No second pin to drift: the workflows install the tools mise.toml names.
	for (const file of [".github/workflows/ci.yml", ".github/workflows/release.yml"]) {
		const text = read(file);
		expect(text, `${file} pins Bun itself`).not.toMatch(/bun-version:/);
		expect(text, `${file} doesn't install mise's tools`).toContain("mise install --locked");
	}
});
