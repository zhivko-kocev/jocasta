/**
 * Jocasta's own scorecard (0610, 0614, 0616, 0645): the mise task `scorecard` the weekly review runs and keeps week on
 * week. Prints one `<measure>: <value>` line per measure, in a fixed order, so two weeks compare line by line: clippy's
 * counts per lint over the Rust sources at reporting thresholds below 0610's failing ceilings, jscpd's duplicated
 * share of `src`, and one `cargo deny` line for the crates' advisories. It fails nothing: it exits 0 whenever it ran,
 * and a measure it could not take is a line that says so.
 */
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const REPO = join(import.meta.dir, "..");

/**
 * The lints clippy counts and the `clippy.toml` option that holds each one's reporting threshold, below the ceilings
 * 0610 fails at (complexity 40, 120 lines a function, 5 parameters); the steps are the ones 0610's Facts measured.
 */
const CLIPPY_LINTS = [
	{ lint: "cognitive_complexity", option: "cognitive-complexity-threshold", over: 25 },
	{ lint: "too_many_lines", option: "too-many-lines-threshold", over: 80 },
	{ lint: "too_many_arguments", option: "too-many-arguments-threshold", over: 4 },
] as const;

/** The measure's label for a clippy lint, the same every week. */
function clippyLabel(entry: (typeof CLIPPY_LINTS)[number]): string {
	return `clippy ${entry.lint} >${entry.over}`;
}

/** A `clippy.toml` holding the reporting thresholds. */
export function clippyConfig(): string {
	return `${CLIPPY_LINTS.map(({ option, over }) => `${option} = ${over}`).join("\n")}\n`;
}

/** The `-W` flags that report each of `CLIPPY_LINTS` as a warning, whatever level Cargo.toml gives it. */
const CLIPPY_FLAGS = CLIPPY_LINTS.flatMap(({ lint }) => ["-W", `clippy::${lint}`]);

/**
 * One line per lint of `CLIPPY_LINTS`: the count of its diagnostics in `cargo clippy --message-format=json` output
 * (one JSON object a line), 0 when none; a diagnostic that two targets report counts once.
 */
export function clippyLines(output: string): string[] {
	const seen = new Set<string>();
	const counts = new Map<string, number>();
	for (const line of output.split("\n")) {
		if (!line.startsWith("{")) continue;
		const message = (JSON.parse(line) as { message?: ClippyMessage }).message;
		const code = message?.code?.code;
		if (code === undefined) continue;
		const span = message?.spans?.find((candidate) => candidate.is_primary);
		const key = `${code} ${span?.file_name}:${span?.line_start}`;
		if (seen.has(key)) continue;
		seen.add(key);
		counts.set(code, (counts.get(code) ?? 0) + 1);
	}
	return CLIPPY_LINTS.map((entry) => `${clippyLabel(entry)}: ${counts.get(`clippy::${entry.lint}`) ?? 0}`);
}

/** The part of a compiler message the scorecard reads. */
interface ClippyMessage {
	code?: { code: string } | null;
	spans?: { file_name: string; line_start: number; is_primary: boolean }[];
}

/** The line for one folder from jscpd's JSON report: the share of duplicated lines, with the clones and lines behind it. */
export function jscpdLine(folder: string, json: string): string {
	const stats = (JSON.parse(json) as { statistics?: { total?: Record<string, unknown> } }).statistics?.total;
	const { percentage, clones, duplicatedLines, lines } = stats ?? {};
	if (typeof percentage !== "number") throw new Error("jscpd's report has no total");
	return `jscpd ${folder} duplicated: ${percentage.toFixed(2)}% (${clones} clones, ${duplicatedLines} of ${lines} lines)`;
}

/**
 * The `cargo deny` line from `cargo deny --format json check advisories`'s output (one JSON object a line, its
 * summary last): the counts by severity, "none" when there is no advisory, note or warning.
 */
export function denyLine(output: string): string {
	const summary = output
		.split("\n")
		.filter((line) => line.startsWith("{"))
		.map((line) => JSON.parse(line) as { type?: string; fields?: { advisories?: Record<string, number> } })
		.find((row) => row.type === "summary")?.fields?.advisories;
	if (summary === undefined) throw new Error("cargo deny's output has no summary");
	const counts = (["errors", "warnings", "notes"] as const)
		.filter((severity) => (summary[severity] ?? 0) > 0)
		.map((severity) => `${summary[severity]} ${severity}`);
	return `cargo deny advisories: ${counts.length > 0 ? counts.join(", ") : "none"}`;
}

/** `label: could not be measured (why)`: the line of a measure that failed, with the first line of the reason. */
function unmeasured(label: string, error: unknown): string {
	const why = error instanceof Error ? error.message : String(error);
	return `${label}: could not be measured (${why.trim().split("\n")[0]})`;
}

/** Runs `program` in the repo; its stdout and stderr and exit code. */
function run(
	program: string,
	args: string[],
	env: Record<string, string> = {},
): Promise<{ stdout: string; stderr: string; code: number | null }> {
	return new Promise((resolve, reject) => {
		const child = spawn(program, args, { cwd: REPO, env: { ...process.env, ...env }, windowsHide: true });
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", (chunk) => {
			stdout += chunk;
		});
		child.stderr.on("data", (chunk) => {
			stderr += chunk;
		});
		child.on("error", reject);
		child.on("close", (code) => resolve({ stdout, stderr, code }));
	});
}

/** `run`, its stdout when the exit code is acceptable, else a throw with what it said. */
async function ran(
	program: string,
	args: string[],
	env: Record<string, string> = {},
	ok: (code: number | null) => boolean = (code) => code === 0,
): Promise<string> {
	const result = await run(program, args, env);
	if (!ok(result.code)) throw new Error(result.stderr.trim() || result.stdout.trim() || `exited ${result.code}`);
	return result.stdout;
}

/** `measure()`'s lines, or one line that says it could not be taken. */
async function measured(label: string, measure: () => Promise<string[]>): Promise<string[]> {
	try {
		return await measure();
	} catch (error) {
		return [unmeasured(label, error)];
	}
}

/** clippy's counts over the library and both binaries (`src`, `bench`, `evals`), with a config written for the run. */
async function clippy(scratch: string): Promise<string[]> {
	writeFileSync(join(scratch, "clippy.toml"), clippyConfig());
	// Warnings alone leave cargo's exit code 0; its own failure leaves no diagnostics to count.
	const out = await ran(
		"cargo",
		["clippy", "--locked", "--lib", "--bins", "--message-format=json", "--", ...CLIPPY_FLAGS],
		{ CLIPPY_CONF_DIR: scratch },
	);
	return clippyLines(out);
}

/** jscpd's duplicated share of one folder of Rust. */
async function jscpd(scratch: string, folder: string): Promise<string[]> {
	const out = join(scratch, folder);
	await ran(process.execPath, [
		"x",
		"jscpd",
		folder,
		"--format",
		"rust",
		"--reporters",
		"json",
		"--output",
		out,
		"--silent",
	]);
	return [jscpdLine(folder, readFileSync(join(out, "jscpd-report.json"), "utf8"))];
}

/** The `cargo deny` line; without the advisory database the line says so. */
async function deny(): Promise<string[]> {
	const result = await run("cargo", ["deny", "--format", "json", "check", "advisories"]);
	try {
		return [denyLine(`${result.stdout}\n${result.stderr}`)];
	} catch {
		const why = (result.stderr.trim() || result.stdout.trim() || `exited ${result.code}`).split("\n")[0];
		return [`cargo deny advisories: could not be reached or read (${why})`];
	}
}

if (import.meta.main) {
	const scratch = mkdtempSync(join(tmpdir(), "jocasta-scorecard-"));
	try {
		const lines = [
			...(await measured("clippy", () => clippy(scratch))),
			...(await measured("jscpd src", () => jscpd(scratch, "src"))),
			...(await measured("cargo deny", deny)),
		];
		console.log(lines.join("\n"));
	} finally {
		rmSync(scratch, { recursive: true, force: true });
	}
}
