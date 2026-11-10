// H1 (0482, amended by 0500 and 0501): the guard's verdicts on a labelled command corpus, scored per group and per
// way of writing. Only an evaluator: the corpus is a separate frozen file.
//
//   bun bench/h1-guard/evaluate.ts --corpus <corpus.jsonl> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>]
//     [--resamples 10000] [--out <dir>] [--dry-run]
//
// Corpus lines: {"id", "command", "expected": "deny"|"prompt"|"allow", "group", "variant", "base", ...}. With
// `--verdicts`, it scores omp 18.18.6's own verdicts (0535, 0589), written by `omp-verdicts.ts` (the matcher of
// record). Without it, it scores its own reimplementation below, which reads `guard.yml`'s rules only and misses omp's
// quote-aware segmenting, the git guard, the critical patterns and the allow-rule compound rule (0501); it stays for
// the unit tests and as the comparison column. Prints per group and per way of writing (0501): deny recall, must-deny
// asked, must-ask stopped, false deny, false ask and ask rate, each with a bootstrap 95% CI over commands. With
// `--out`, writes `h1-guard.json` (summary) and `h1-guard.jsonl` (one verdict per command). The `eval` tool and
// scripts are outside the guard's reach (`config/guard.yml:10`); this measures pattern recall only, not containment.
import { existsSync, readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { type GuardRule, parseGuard as parseGuardText, type Verdict, verdict } from "../../src/shared/guard.ts";
import { BenchUsageError, flag, parseBenchArgs, REPO_ROOT, runScript } from "../lib/args.ts";
import { strictJsonLines } from "../lib/jsonl.ts";
import { appendRow, conditions, printPlan, writeJson } from "../lib/output.ts";
import { type Rate, rate } from "../lib/stats.ts";

export type { GuardRule, Verdict };

export interface CorpusEntry {
	command: string;
	expected: Verdict;
	group: string;
	id?: string;
	/** How the command is written (0501): `as-written` or one of the rewrites. */
	variant?: string;
	/** The collected command this line was made from. */
	base?: string;
	category?: string;
}

const VERDICTS: readonly Verdict[] = ["deny", "prompt", "allow"];

/** The ways of writing in the corpus (0501), in report order. */
export const VARIANTS = [
	"as-written",
	"after-cd",
	"semicolon-chain",
	"pwsh-c",
	"cmd-c",
	"lowercase-powershell",
	"git-exe",
	"git-full-path",
	"git-C",
	"extra-spaces",
] as const;

/** Reads a guard file's `bash.patterns`; a malformed file is a usage error (exit 2). */
export function parseGuard(text: string, file = "guard"): GuardRule[] {
	try {
		return parseGuardText(text, file);
	} catch (error) {
		throw new BenchUsageError((error as Error).message);
	}
}

const OPTIONAL_FIELDS = ["id", "variant", "base", "category"] as const;

/** Reads the corpus JSONL; a malformed line throws with its line number. */
export function parseCorpus(text: string, file = "corpus"): CorpusEntry[] {
	const entries: CorpusEntry[] = [];
	for (const { row, where } of strictJsonLines(text, file)) {
		if (
			typeof row.command !== "string" ||
			!VERDICTS.includes(row.expected as Verdict) ||
			typeof row.group !== "string" ||
			row.group === ""
		)
			throw new BenchUsageError(`${where}: needs command, expected (deny|prompt|allow) and group`);
		const entry: CorpusEntry = { command: row.command, expected: row.expected as Verdict, group: row.group };
		for (const field of OPTIONAL_FIELDS) {
			const value = row[field];
			if (typeof value === "string") entry[field] = value;
		}
		entries.push(entry);
	}
	return entries;
}

/**
 * The corpus at `path`, parsed, with the SHA-256 of its text that every H1 script's summary records as
 * `corpusSha256` (BEN-19: one read and hash instead of four).
 */
export function readCorpus(path: string): { entries: CorpusEntry[]; sha256: string } {
	const text = readFileSync(path, "utf8");
	return { entries: parseCorpus(text, path), sha256: new Bun.CryptoHasher("sha256").update(text).digest("hex") };
}

export interface Metrics {
	counts: Record<Verdict, number>;
	/** must-deny judged deny. */
	denyRecall: Rate;
	/** must-deny judged prompt (stopped, but only by asking). */
	denyAsked: Rate;
	/** must-ask judged prompt. */
	askRecall: Rate;
	/** must-ask judged deny or prompt. */
	askStopped: Rate;
	/** must-allow judged deny. */
	falseDeny: Rate;
	/** must-allow judged prompt. */
	falseAsk: Rate;
	/** Every command judged prompt. */
	askRate: Rate;
}

export interface GroupReport extends Metrics {
	group: string;
}

export interface VariantReport extends Metrics {
	variant: string;
}

export interface Judged extends CorpusEntry {
	verdict: Verdict;
	/** The guard pattern that decided, if one did. */
	rule: string | null;
	/** What decided, with `--verdicts` (omp-verdicts.ts): guard-pattern, git-guard, critical-pattern, mode-default… */
	mechanism?: string;
	/** The reimplementation's verdict beside omp's, with `--verdicts`. */
	reimplemented?: Verdict;
}

export function judge(corpus: readonly CorpusEntry[], rules: readonly GuardRule[]): Judged[] {
	return corpus.map((entry) => ({ ...entry, ...verdict(entry.command, rules) }));
}

/** One row of `omp-verdicts.jsonl` as scoring reads it. */
export interface OmpVerdictRow {
	id: string;
	command: string;
	verdict: Verdict;
	mechanism: string;
	match: string | null;
}

/** Reads `omp-verdicts.jsonl`; a malformed line throws with its line number. */
export function parseVerdicts(text: string, file = "verdicts"): OmpVerdictRow[] {
	const rows: OmpVerdictRow[] = [];
	for (const { row, where } of strictJsonLines(text, file)) {
		if (
			typeof row.id !== "string" ||
			typeof row.command !== "string" ||
			!VERDICTS.includes(row.verdict as Verdict) ||
			typeof row.mechanism !== "string"
		)
			throw new BenchUsageError(`${where}: needs id, command, verdict (deny|prompt|allow) and mechanism`);
		rows.push({
			id: row.id,
			command: row.command,
			verdict: row.verdict as Verdict,
			mechanism: row.mechanism,
			match: typeof row.match === "string" ? row.match : null,
		});
	}
	return rows;
}

/**
 * omp's verdicts joined to the corpus by id, with the reimplementation's verdict beside each. Every corpus line needs
 * exactly one verdict for the same command.
 */
export function judgeFromVerdicts(
	corpus: readonly CorpusEntry[],
	verdicts: readonly OmpVerdictRow[],
	rules: readonly GuardRule[],
): Judged[] {
	const byId = new Map<string, OmpVerdictRow>();
	for (const row of verdicts) {
		if (byId.has(row.id)) throw new BenchUsageError(`verdicts: ${row.id} appears twice`);
		byId.set(row.id, row);
	}
	if (byId.size !== corpus.length)
		throw new BenchUsageError(`verdicts: ${byId.size} rows for ${corpus.length} corpus lines`);
	return corpus.map((entry) => {
		const row = entry.id === undefined ? undefined : byId.get(entry.id);
		if (row === undefined) throw new BenchUsageError(`verdicts: no verdict for corpus line ${entry.id ?? "(no id)"}`);
		if (row.command !== entry.command)
			throw new BenchUsageError(`verdicts: ${row.id}'s command differs from the corpus's`);
		return {
			...entry,
			verdict: row.verdict,
			rule: row.mechanism === "guard-pattern" ? row.match : null,
			mechanism: row.mechanism,
			reimplemented: verdict(entry.command, rules).verdict,
		};
	});
}

/** The rates with bootstrap CIs over `rows`. */
export function metrics(rows: readonly Judged[], resamples?: number): Metrics {
	const of = (expected: Verdict) => rows.filter((row) => row.expected === expected);
	const options = { resamples };
	return {
		counts: { deny: of("deny").length, prompt: of("prompt").length, allow: of("allow").length },
		denyRecall: rate(
			of("deny").map((row) => row.verdict === "deny"),
			options,
		),
		denyAsked: rate(
			of("deny").map((row) => row.verdict === "prompt"),
			options,
		),
		askRecall: rate(
			of("prompt").map((row) => row.verdict === "prompt"),
			options,
		),
		askStopped: rate(
			of("prompt").map((row) => row.verdict !== "allow"),
			options,
		),
		falseDeny: rate(
			of("allow").map((row) => row.verdict === "deny"),
			options,
		),
		falseAsk: rate(
			of("allow").map((row) => row.verdict === "prompt"),
			options,
		),
		askRate: rate(
			rows.map((row) => row.verdict === "prompt"),
			options,
		),
	};
}

/** Per group (plus `all`), the rates with bootstrap CIs. */
export function report(judged: readonly Judged[], resamples?: number): GroupReport[] {
	const groups = [...new Set(judged.map((row) => row.group))].sort();
	return [...groups, "all"].map((group) => ({
		group,
		...metrics(group === "all" ? judged : judged.filter((row) => row.group === group), resamples),
	}));
}

/**
 * The lines of one way of writing (0501). `as-written` is every line whose command is one of the collected commands
 * as written: the builder keeps one line per command, so a collected command that equals another's rewrite (the
 * corpus's `rm -rf node_modules`, also `rm -Rf node_modules` lowercased) sits under that rewrite's variant and counts
 * in both rows.
 */
export function variantRows(judged: readonly Judged[], variant: string): Judged[] {
	if (variant !== "as-written") return judged.filter((row) => row.variant === variant);
	const bases = new Set(judged.flatMap((row) => (row.base === undefined ? [] : [row.base])));
	return judged.filter((row) => row.variant === "as-written" || bases.has(row.command));
}

/** Per way of writing, in `VARIANTS` order then any others sorted; variants without lines are left out. */
export function variantReport(judged: readonly Judged[], resamples?: number): VariantReport[] {
	const present = new Set(judged.flatMap((row) => (row.variant === undefined ? [] : [row.variant])));
	const known: readonly string[] = VARIANTS;
	const order = [
		...VARIANTS.filter((variant) => present.has(variant)),
		...[...present].filter((v) => !known.includes(v)).sort(),
	];
	return order.map((variant) => ({ variant, ...metrics(variantRows(judged, variant), resamples) }));
}

const percent = (value: Rate) =>
	value.n === 0
		? "—"
		: `${(100 * value.rate).toFixed(1)}% [${(100 * value.ci95.low).toFixed(1)}, ${(100 * value.ci95.high).toFixed(1)}] (${value.hits}/${value.n})`;

const metricCells = (r: Metrics) =>
	`${r.counts.deny}/${r.counts.prompt}/${r.counts.allow} | ${percent(r.denyRecall)} | ${percent(r.denyAsked)} | ${percent(r.askStopped)} | ${percent(r.falseDeny)} | ${percent(r.falseAsk)} | ${percent(r.askRate)} |`;
const HEADER_CELLS =
	"deny/ask/allow | Deny recall | Must-deny asked | Must-ask stopped | False deny | False ask | Ask rate |";

export function renderReport(reports: readonly GroupReport[], variants: readonly VariantReport[] = []): string {
	return [
		`| Group | ${HEADER_CELLS}`,
		"|---|---|---|---|---|---|---|---|",
		...reports.map((r) => `| ${r.group} | ${metricCells(r)}`),
		...(variants.length === 0
			? []
			: [
					"",
					`| Way of writing | ${HEADER_CELLS}`,
					"|---|---|---|---|---|---|---|---|",
					...variants.map((r) => `| ${r.variant} | ${metricCells(r)}`),
				]),
		"",
		"Pattern recall only: omp's `eval` tool, scripts and interpreters run outside the guard (config/guard.yml:10-12).",
	].join("\n");
}

if (import.meta.main) {
	if (process.argv.slice(2).some((arg) => arg === "--help" || arg === "-h")) {
		console.log(
			"usage: bun bench/h1-guard/evaluate.ts --corpus <corpus.jsonl> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>] [--resamples 10000] [--out <dir>] [--dry-run]",
		);
		process.exit(0);
	}
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: ["--corpus", "--guard", "--resamples", "--verdicts"],
		});
		const corpusPath = flag(args, "corpus");
		if (corpusPath === undefined) throw new BenchUsageError("--corpus <path.jsonl> is required");
		const guardPath = resolve(flag(args, "guard", resolve(REPO_ROOT, "config", "guard.yml")));
		const verdictsText = flag(args, "verdicts");
		const verdictsPath = verdictsText === undefined ? undefined : resolve(verdictsText);
		const resamplesText = flag(args, "resamples", "10000");
		if (!/^\d+$/.test(resamplesText) || Number(resamplesText) < 1)
			throw new BenchUsageError(`--resamples needs a whole number, not "${resamplesText}"`);
		const resamples = Number(resamplesText);
		for (const path of [corpusPath, guardPath, ...(verdictsPath === undefined ? [] : [verdictsPath])])
			if (!existsSync(path)) throw new BenchUsageError(`${path} does not exist`);
		const { entries: corpus, sha256: corpusSha256 } = readCorpus(corpusPath);
		const rules = parseGuard(readFileSync(guardPath, "utf8"), guardPath);
		const matcher =
			verdictsPath === undefined
				? "reimplementation (evaluate.ts: guard.yml's rules only, raw-text segments)"
				: `omp's own verdicts (${verdictsPath})`;
		if (args.dryRun) {
			const groups = [...new Set(corpus.map((entry) => entry.group))].sort();
			printPlan(
				"H1 guard",
				[
					`judge ${corpus.length} commands (groups: ${groups.join(", ") || "none"}) against ${rules.length} rules in ${guardPath}`,
					verdictsPath === undefined
						? "matcher: the reimplementation (guard.yml's rules only; `*` wildcard, anchored, case-sensitive, whole command or any raw-text segment at && || ; |); omp's own verdicts need --verdicts"
						: `matcher: omp's own verdicts from ${verdictsPath}, the reimplementation beside them`,
					`report deny recall, must-ask stopped, false deny, false ask and ask rate per group and per way of writing with ${resamples}-resample bootstrap 95% CIs`,
				],
				args.out,
			);
			return;
		}
		const started = conditions([]);
		const judged =
			verdictsPath === undefined
				? judge(corpus, rules)
				: judgeFromVerdicts(corpus, parseVerdicts(readFileSync(verdictsPath, "utf8"), verdictsPath), rules);
		const reports = report(judged, resamples);
		const variants = variantReport(judged, resamples);
		const asWritten = variantRows(judged, "as-written");
		const filedElsewhere = asWritten.filter((row) => row.variant !== "as-written");
		const asWrittenNote = `the as-written row is the ${asWritten.length} lines whose command is one of the collected commands as written; ${asWritten.length - filedElsewhere.length} carry the as-written tag, and ${filedElsewhere.map((row) => `${row.id ?? "?"} (${row.command}, tagged ${row.variant ?? "?"})`).join(", ") || "none"} ${filedElsewhere.length === 1 ? "is" : "are"} filed under another rewrite because the builder keeps one line per command (0501); such a line counts in both rows`;
		console.log(`matcher: ${matcher}`);
		console.log(renderReport(reports, variants));
		if (args.out === undefined) return;
		for (const row of judged) appendRow(args.out, "h1-guard", row);
		const file = writeJson(args.out, "h1-guard", {
			benchmark: "H1",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			matcher,
			guard: relative(REPO_ROOT, guardPath).startsWith("..") ? guardPath : relative(REPO_ROOT, guardPath),
			corpusSha256,
			...(verdictsPath === undefined
				? {}
				: {
						verdictsSha256: new Bun.CryptoHasher("sha256").update(readFileSync(verdictsPath)).digest("hex"),
					}),
			rules: rules.length,
			notes: { asWritten: asWrittenNote },
			commands: corpus.length,
			reports,
			variants,
		});
		console.log(`wrote ${file}`);
	});
}
