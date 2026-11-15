/**
 * Jocasta's own scorecard (0610, 0614, 0616): the mise task `scorecard` the weekly review runs and keeps week on week.
 * Prints one `<measure>: <value>` line per measure, in a fixed order, so two weeks compare line by line: Biome's counts
 * per rule over `src` at reporting thresholds below 0610's failing ceilings, jscpd's duplicated share of `src` and
 * `test`, and one `bun audit` line. It fails nothing: it exits 0 whenever it ran, and a measure it could not take is
 * a line that says so.
 */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { asObject } from "../src/shared/fsx.ts";
import { runProgram } from "../src/shared/process.ts";

/**
 * The rules Biome counts and the option that holds each one's reporting threshold, below the ceilings 0610 fails at
 * (complexity 40, 120 lines a function, 5 parameters, 1,000 lines a file); the steps are the ones 0610's Facts measured.
 */
const BIOME_RULES = [
	{ group: "complexity", rule: "noExcessiveCognitiveComplexity", option: "maxAllowedComplexity", over: 25 },
	{ group: "complexity", rule: "noExcessiveLinesPerFunction", option: "maxLines", over: 80 },
	{ group: "complexity", rule: "useMaxParams", option: "max", over: 4 },
	{ group: "style", rule: "noExcessiveLinesPerFile", option: "maxLines", over: 800 },
] as const;

/** The measure's label for a Biome rule, the same every week. */
function biomeLabel(entry: (typeof BIOME_RULES)[number]): string {
	return `biome src ${entry.group}/${entry.rule} >${entry.over}`;
}

/** A Biome config that reports only `BIOME_RULES`, each as a warning at its threshold. */
export function biomeConfig(): string {
	const groups: Record<string, Record<string, unknown>> = {};
	for (const { group, rule, option, over } of BIOME_RULES) {
		groups[group] = { ...groups[group], [rule]: { level: "warn", options: { [option]: over } } };
	}
	return JSON.stringify({ root: true, linter: { enabled: true, rules: { recommended: false, ...groups } } });
}

/** One line per rule of `BIOME_RULES`: the count of diagnostics in Biome's `--reporter=json` output, 0 when none. */
export function biomeLines(json: string): string[] {
	const parsed = asObject(JSON.parse(json.slice(json.indexOf("{"))));
	const diagnostics = Array.isArray(parsed?.diagnostics) ? parsed.diagnostics : [];
	const counts = new Map<string, number>();
	for (const diagnostic of diagnostics) {
		const category = asObject(diagnostic)?.category;
		if (typeof category === "string") counts.set(category, (counts.get(category) ?? 0) + 1);
	}
	return BIOME_RULES.map((entry) => `${biomeLabel(entry)}: ${counts.get(`lint/${entry.group}/${entry.rule}`) ?? 0}`);
}

/** The line for one folder from jscpd's JSON report: the share of duplicated lines, with the clones and lines behind it. */
export function jscpdLine(folder: string, json: string): string {
	const total = asObject(asObject(JSON.parse(json))?.statistics)?.total;
	const stats = asObject(total);
	const { percentage, clones, duplicatedLines, lines } = stats ?? {};
	if (typeof percentage !== "number") throw new Error("jscpd's report has no total");
	return `jscpd ${folder} duplicated: ${percentage.toFixed(2)}% (${clones} clones, ${duplicatedLines} of ${lines} lines)`;
}

/**
 * The `bun audit` line from its `--json` output (advisories by package): the counts by severity, "none" when there is
 * no advisory.
 */
export function auditLine(json: string): string {
	const found = asObject(JSON.parse(json)) ?? {};
	const bySeverity = new Map<string, number>();
	for (const advisories of Object.values(found)) {
		for (const advisory of Array.isArray(advisories) ? advisories : []) {
			const severity = asObject(advisory)?.severity;
			const name = typeof severity === "string" ? severity : "unknown";
			bySeverity.set(name, (bySeverity.get(name) ?? 0) + 1);
		}
	}
	const counts = [...bySeverity].map(([severity, count]) => `${count} ${severity}`);
	return `bun audit: ${counts.length > 0 ? counts.join(", ") : "none"}`;
}

/** `label: could not be measured (why)`: the line of a measure that failed, with the first line of the reason. */
function unmeasured(label: string, error: unknown): string {
	const why = error instanceof Error ? error.message : String(error);
	return `${label}: could not be measured (${why.trim().split("\n")[0]})`;
}

/** Runs `bun x <args>` or another bun command in the repo; its stdout, or throws with its stderr when it failed. */
async function bun(args: string[], ok: (code: number | null) => boolean = (code) => code === 0): Promise<string> {
	const run = await runProgram([process.execPath, ...args], { cwd: join(import.meta.dir, "..") });
	if (!ok(run.code)) throw new Error(run.stderr.trim() || run.stdout.trim() || `exited ${run.code}`);
	return run.stdout;
}

/** `measure()`'s lines, or one line that says it could not be taken. */
async function measured(label: string, measure: () => Promise<string[]>): Promise<string[]> {
	try {
		return await measure();
	} catch (error) {
		return [unmeasured(label, error)];
	}
}

/** Biome's counts over `src`, with a config written for the run. */
async function biome(scratch: string): Promise<string[]> {
	const config = join(scratch, "biome.json");
	writeFileSync(config, biomeConfig());
	// Warnings alone leave Biome's exit code 0; its own failure leaves no JSON to count.
	const out = await bun(
		[
			"x",
			"biome",
			"lint",
			`--config-path=${config}`,
			"--reporter=json",
			"--max-diagnostics=none",
			"--colors=off",
			"src",
		],
		(code) => code === 0 || code === 1,
	);
	return biomeLines(out);
}

/** jscpd's duplicated share of one folder. */
async function jscpd(scratch: string, folder: string): Promise<string[]> {
	const out = join(scratch, folder);
	await bun(["x", "jscpd", folder, "--format", "typescript", "--reporters", "json", "--output", out, "--silent"]);
	return [jscpdLine(folder, readFileSync(join(out, "jscpd-report.json"), "utf8"))];
}

/** The `bun audit` line; without the registry the line says so. */
async function audit(): Promise<string[]> {
	const run = await runProgram([process.execPath, "audit", "--json"], { cwd: join(import.meta.dir, "..") });
	try {
		return [auditLine(run.stdout)];
	} catch {
		const why = (run.stderr.trim() || run.stdout.trim() || `exited ${run.code}`).split("\n")[0];
		return [`bun audit: could not be reached or read (${why})`];
	}
}

if (import.meta.main) {
	const scratch = mkdtempSync(join(tmpdir(), "jocasta-scorecard-"));
	try {
		const lines = [
			...(await measured("biome src", () => biome(scratch))),
			...(await measured("jscpd src", () => jscpd(scratch, "src"))),
			...(await measured("jscpd test", () => jscpd(scratch, "test"))),
			...(await measured("bun audit", audit)),
		];
		console.log(lines.join("\n"));
	} finally {
		rmSync(scratch, { recursive: true, force: true });
	}
}
