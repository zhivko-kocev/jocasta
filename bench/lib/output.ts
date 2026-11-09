import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, totalmem } from "node:os";
import { join } from "node:path";
import { BenchUsageError } from "./args.ts";

/**
 * Every benchmark writes `<out>/<benchmark>.json` (the summary with the run's conditions, 0482 / [S16]) and, where it
 * has them, raw rows in `<out>/<benchmark>.jsonl`. No host or user names: the conditions are the OS, CPU, memory,
 * load and the pinned tools' versions.
 */

export interface Conditions {
	startedAt: string;
	finishedAt?: string;
	/** Local hour 0–23: the design interleaves runs over times of day ([S16]). */
	localHour: number;
	platform: string;
	osRelease: string;
	arch: string;
	cpu: string;
	cpuCount: number;
	memoryGb: number;
	versions: Record<string, string | null>;
	/**
	 * SHA-256 of the `jocasta` on PATH, when the tools include it (0576): with `versions.jocasta` (`jocasta --version`)
	 * it ties each row to the build it measured (0533); null when it isn't on PATH.
	 */
	jocastaSha256?: string | null;
}

/**
 * `<launcher> --version`'s first line: the eval runner's harness version and every benchmark's tool versions (EVL-14:
 * one reader). A failure or an empty first line throws, naming the launcher.
 */
export function readVersion(launcher: readonly string[]): string {
	const [command = "", ...args] = launcher;
	const result = spawnSync(command, [...args, "--version"], { encoding: "utf8", windowsHide: true, timeout: 30_000 });
	const version = result.stdout?.trim().split(/\r?\n/)[0];
	if (result.status !== 0 || !version)
		throw new Error(`${launcher.join(" ")} --version failed: ${result.stderr || result.error?.message}`);
	return version;
}

/** `<command> --version`'s first line, or null when it isn't on PATH or fails. Never starts a session. */
export function toolVersion(command: string): string | null {
	const found = Bun.which(command);
	if (!found) return null;
	try {
		return readVersion([found]);
	} catch {
		return null;
	}
}

/** SHA-256 of the program `command` runs on PATH, or null. */
export function programSha256(command: string): string | null {
	const found = Bun.which(command);
	if (!found) return null;
	try {
		return new Bun.CryptoHasher("sha256").update(readFileSync(found)).digest("hex");
	} catch {
		return null;
	}
}

export function conditions(tools: readonly string[]): Conditions {
	const now = new Date();
	return {
		startedAt: now.toISOString(),
		localHour: now.getHours(),
		platform: platform(),
		osRelease: release(),
		arch: arch(),
		cpu: cpus()[0]?.model ?? "unknown",
		cpuCount: cpus().length,
		memoryGb: Math.round((totalmem() / 2 ** 30) * 10) / 10,
		versions: Object.fromEntries(tools.map((tool) => [tool, toolVersion(tool)])),
		...(tools.includes("jocasta") ? { jocastaSha256: programSha256("jocasta") } : {}),
	};
}

/** Writes `<out>/<name>.json`, pretty, creating `out`. */
export function writeJson(out: string, name: string, data: unknown): string {
	mkdirSync(out, { recursive: true });
	const file = join(out, `${name}.json`);
	writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`);
	return file;
}

/**
 * Refuses a run whose `<out>/<name>.jsonl` exists (as the H1 scripts do): a rerun in the same folder would mix earlier
 * rows into the raw file while `<name>.json` summarizes only the new run. Called once at a script's start.
 */
export function claimRows(out: string, ...names: string[]): void {
	for (const name of names) {
		const file = join(out, `${name}.jsonl`);
		if (existsSync(file)) throw new BenchUsageError(`${file} exists; results are never overwritten`);
	}
}

/** Appends one row to `<out>/<name>.jsonl`. */
export function appendRow(out: string, name: string, row: unknown): void {
	mkdirSync(out, { recursive: true });
	appendFileSync(join(out, `${name}.jsonl`), `${JSON.stringify(row)}\n`);
}

/** Writes `<out>/<name>.jsonl` whole: rows derived again from the run's own files each time (tokscale parity). */
export function writeRows(out: string, name: string, rows: readonly unknown[]): void {
	mkdirSync(out, { recursive: true });
	writeFileSync(join(out, `${name}.jsonl`), rows.map((row) => `${JSON.stringify(row)}\n`).join(""));
}

/** A dry run's plan: the benchmark, what each run measures, and where results would go. */
export function printPlan(benchmark: string, lines: readonly string[], out: string | undefined): void {
	console.log(`${benchmark} (0482) dry run: nothing is started, killed or measured.`);
	for (const line of lines) console.log(`  - ${line}`);
	console.log(`  results: ${out === undefined ? "<--out dir>" : out}`);
}
