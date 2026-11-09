// Benchmark H1's corpus builder (0482). It takes the source lines a researcher collected from public documentation
// without reading the guard (`h1-sources.jsonl`: {command, class, category, source}) and derives the frozen corpus
// mechanically: every source line as written, plus the wrapped and respelled forms a shell user or an agent produces
// without changing what the command does. No command is written here by hand; the derivations are the transforms below.
//
//   bun bench/h1-guard/build-corpus.ts --sources D:/thesis/data/bench/h1-sources.jsonl --out D:/thesis/data/bench/h1-corpus.jsonl
//
// Output lines: {id, command, expected, group, variant, base, category, source}. `expected` maps the class to the
// guard's verdicts (must-deny → deny, must-ask → prompt, must-allow → allow). `group` is `held-out` for every
// derived line; the `dev` group (the security review's own bypass list, which the guard's author saw) is appended
// from `--dev <file>` when given. The corpus file's SHA-256 goes beside it in `h1-corpus.sha256`.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

interface SourceLine {
	command: string;
	class: "must-deny" | "must-ask" | "must-allow";
	category?: string;
	source?: string;
}

interface CorpusLine {
	id: string;
	command: string;
	expected: "deny" | "prompt" | "allow";
	group: "held-out" | "dev";
	variant: string;
	base: string;
	category: string;
	source: string;
}

const EXPECTED: Record<SourceLine["class"], CorpusLine["expected"]> = {
	"must-deny": "deny",
	"must-ask": "prompt",
	"must-allow": "allow",
};

/** PowerShell cmdlets and parameters are case-insensitive: the respelled form runs the same way. */
const POWERSHELL =
	/^(Remove-Item|Invoke-Expression|Invoke-RestMethod|Invoke-WebRequest|Get-Content|Set-Content|ri|rm|iex|irm|iwr)\b/;

const TRANSFORMS: { variant: string; apply: (command: string) => string | null }[] = [
	{ variant: "as-written", apply: (c) => c },
	{ variant: "after-cd", apply: (c) => `cd C:\\work\\demo && ${c}` },
	{ variant: "semicolon-chain", apply: (c) => `echo start; ${c}; echo done` },
	{
		variant: "pwsh-c",
		apply: (c) => (c.includes('"') ? null : `pwsh -c "${c}"`),
	},
	{
		variant: "cmd-c",
		apply: (c) => (/^(rd|rmdir|del|erase|format|diskpart|reg|vssadmin)\b/.test(c) ? `cmd /c "${c}"` : null),
	},
	{
		variant: "lowercase-powershell",
		apply: (c) => (POWERSHELL.test(c) && c !== c.toLowerCase() ? c.toLowerCase() : null),
	},
	{
		variant: "git-exe",
		apply: (c) => (/^git /.test(c) ? c.replace(/^git /, "git.exe ") : null),
	},
	{
		variant: "git-full-path",
		apply: (c) => (/^git /.test(c) ? c.replace(/^git /, '"C:\\Program Files\\Git\\cmd\\git.exe" ') : null),
	},
	{
		variant: "git-C",
		apply: (c) => (/^git /.test(c) ? c.replace(/^git /, "git -C C:\\work\\demo ") : null),
	},
	{
		variant: "extra-spaces",
		apply: (c) => (c.includes(" ") ? c.replace(/ /g, "  ") : null),
	},
];

function args(): { sources: string; out: string; dev?: string } {
	const argv = process.argv.slice(2);
	const get = (flag: string) => {
		const i = argv.indexOf(flag);
		return i >= 0 ? argv[i + 1] : undefined;
	};
	const sources = get("--sources");
	const out = get("--out");
	if (!sources || !out) {
		console.error(
			"usage: bun bench/h1-guard/build-corpus.ts --sources <h1-sources.jsonl> --out <h1-corpus.jsonl> [--dev <dev.jsonl>]",
		);
		process.exit(2);
	}
	return { sources, out, dev: get("--dev") };
}

function readLines(path: string): SourceLine[] {
	return readFileSync(path, "utf8")
		.split(/\r?\n/)
		.filter((line) => line.trim())
		.map((line) => JSON.parse(line) as SourceLine);
}

export function buildCorpus(sources: SourceLine[], dev: SourceLine[] = []): CorpusLine[] {
	const lines: CorpusLine[] = [];
	const seen = new Set<string>();
	let n = 0;
	const push = (line: Omit<CorpusLine, "id">) => {
		const key = `${line.group}\u0000${line.command}`;
		if (seen.has(key)) return;
		seen.add(key);
		n += 1;
		lines.push({ id: `h1-${String(n).padStart(4, "0")}`, ...line });
	};
	for (const source of sources) {
		for (const { variant, apply } of TRANSFORMS) {
			const command = apply(source.command);
			if (command === null) continue;
			push({
				command,
				expected: EXPECTED[source.class],
				group: "held-out",
				variant,
				base: source.command,
				category: source.category ?? "",
				source: source.source ?? "",
			});
		}
	}
	for (const source of dev) {
		push({
			command: source.command,
			expected: EXPECTED[source.class],
			group: "dev",
			variant: "as-written",
			base: source.command,
			category: source.category ?? "",
			source: source.source ?? "",
		});
	}
	return lines;
}

if (import.meta.main) {
	const { sources, out, dev } = args();
	const corpus = buildCorpus(readLines(sources), dev ? readLines(dev) : []);
	const text = `${corpus.map((line) => JSON.stringify(line)).join("\n")}\n`;
	writeFileSync(out, text);
	const sha = createHash("sha256").update(text).digest("hex");
	writeFileSync(`${out.replace(/\.jsonl$/, "")}.sha256`, `${sha}  ${out.split(/[\\/]/).pop()}\n`);
	const by = new Map<string, number>();
	for (const line of corpus)
		by.set(`${line.group}/${line.expected}`, (by.get(`${line.group}/${line.expected}`) ?? 0) + 1);
	console.log(`${corpus.length} lines → ${out}; sha256 ${sha}`);
	for (const [key, count] of [...by.entries()].sort()) console.log(`  ${key}: ${count}`);
}
