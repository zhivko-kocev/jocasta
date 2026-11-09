import { isAbsolute, relative, resolve } from "node:path";
import { UserError } from "../../src/shared/errors.ts";

/**
 * The flags every benchmark script takes (0482): `--out <dir>` outside the repo (raw results live in
 * `D:/thesis/data/`), `--runs <n>` and `--dry-run`, plus the script's own `--name value` flags.
 */

/** The repo root: `bench/lib/` is two levels below it. */
export const REPO_ROOT = resolve(import.meta.dir, "..", "..");

/**
 * The harness's one usage-error convention (DUP-18): a `UserError` (exit 2), which {@link runScript} prints after
 * `bench: `; `evals/run.ts` prints the same class after `jocasta: `.
 */
export class BenchUsageError extends UserError {
	override name = "BenchUsageError";
}

export interface BenchArgs {
	/** Absolute; undefined only on a dry run without `--out`. */
	out?: string;
	runs: number;
	dryRun: boolean;
	/** The script's own flags: the last value of each `--name value`, and `true` for a listed boolean flag. */
	flags: Map<string, string | true>;
}

export interface ArgSpec {
	defaultRuns: number;
	/** The script's own flags that take a value. */
	valued?: readonly string[];
	/** The script's own flags without a value. */
	booleans?: readonly string[];
	/** Repo-relative or absolute paths that may lie inside the repo are fine; `--out` never may. */
	repoRoot?: string;
}

/** True when `path` is `root` or lies under it. */
export function isInside(root: string, path: string): boolean {
	const rel = relative(resolve(root), resolve(path));
	return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

export function parseBenchArgs(argv: readonly string[], spec: ArgSpec): BenchArgs {
	const valued = new Set(["--out", "--runs", ...(spec.valued ?? [])]);
	const booleans = new Set(["--dry-run", ...(spec.booleans ?? [])]);
	const flags = new Map<string, string | true>();
	for (let i = 0; i < argv.length; i++) {
		const flag = argv[i] ?? "";
		if (booleans.has(flag)) {
			flags.set(flag.slice(2), true);
			continue;
		}
		if (!valued.has(flag)) throw new BenchUsageError(`unknown option: ${flag}`);
		const value = argv[++i];
		if (value === undefined || value.startsWith("--")) throw new BenchUsageError(`${flag} needs a value`);
		flags.set(flag.slice(2), value);
	}
	const dryRun = flags.get("dry-run") === true;
	flags.delete("dry-run");
	let runs = spec.defaultRuns;
	const runsText = flags.get("runs");
	flags.delete("runs");
	if (typeof runsText === "string") {
		if (!/^\d+$/.test(runsText) || Number(runsText) < 1)
			throw new BenchUsageError(`--runs needs a whole number of 1 or more, not "${runsText}"`);
		runs = Number(runsText);
	}
	const outText = flags.get("out");
	flags.delete("out");
	const out = typeof outText === "string" ? resolve(outText) : undefined;
	if (out !== undefined && isInside(spec.repoRoot ?? REPO_ROOT, out))
		throw new BenchUsageError(`--out must be outside the repo (raw results live in D:/thesis/data/), not ${out}`);
	if (out === undefined && !dryRun) throw new BenchUsageError("--out <dir> is required (outside the repo)");
	return { out, runs, dryRun, flags };
}

/** A script's own valued flag, or `fallback`. */
export function flag(args: BenchArgs, name: string, fallback: string): string;
export function flag(args: BenchArgs, name: string): string | undefined;
export function flag(args: BenchArgs, name: string, fallback?: string): string | undefined {
	const value = args.flags.get(name);
	return typeof value === "string" ? value : fallback;
}

/** A comma-separated list flag. */
export function listFlag(args: BenchArgs, name: string, fallback: readonly string[]): string[] {
	const value = flag(args, name);
	return value === undefined
		? [...fallback]
		: value
				.split(",")
				.map((item) => item.trim())
				.filter((item) => item !== "");
}

/** A whole-number list flag such as `--threads 2,4,8`. */
export function numberListFlag(args: BenchArgs, name: string, fallback: readonly number[]): number[] {
	return listFlag(args, name, fallback.map(String)).map((item) => {
		if (!/^\d+$/.test(item)) throw new BenchUsageError(`--${name} takes whole numbers, not "${item}"`);
		return Number(item);
	});
}

/**
 * Runs a script's main: a usage error (any `UserError`, a {@link BenchUsageError} or a refusal from the product's
 * modules such as an unknown variant) prints one line and exits with its code (2); a dry run prints the plan and
 * measures nothing.
 */
export async function runScript(main: () => Promise<void> | void): Promise<void> {
	try {
		await main();
	} catch (error) {
		if (error instanceof UserError) {
			console.error(`bench: ${error.message}`);
			process.exit(error.exitCode);
		}
		throw error;
	}
}
