import { spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, totalmem } from "node:os";
import { join } from "node:path";

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
}

/** `<command> --version`'s first line, or null when it isn't on PATH or fails. Never starts a session. */
export function toolVersion(command: string): string | null {
	const found = Bun.which(command);
	if (!found) return null;
	const result = spawnSync(found, ["--version"], { encoding: "utf8", windowsHide: true, timeout: 30_000 });
	const line = result.stdout?.trim().split(/\r?\n/)[0];
	return result.status === 0 && line ? line : null;
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
	};
}

/** Writes `<out>/<name>.json`, pretty, creating `out`. */
export function writeJson(out: string, name: string, data: unknown): string {
	mkdirSync(out, { recursive: true });
	const file = join(out, `${name}.json`);
	writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`);
	return file;
}

/** Appends one row to `<out>/<name>.jsonl`. */
export function appendRow(out: string, name: string, row: unknown): void {
	mkdirSync(out, { recursive: true });
	appendFileSync(join(out, `${name}.jsonl`), `${JSON.stringify(row)}\n`);
}

/** A dry run's plan: the benchmark, what each run measures, and where results would go. */
export function printPlan(benchmark: string, lines: readonly string[], out: string | undefined): void {
	console.log(`${benchmark} (0482) dry run: nothing is started, killed or measured.`);
	for (const line of lines) console.log(`  - ${line}`);
	console.log(`  results: ${out === undefined ? "<--out dir>" : out}`);
}
