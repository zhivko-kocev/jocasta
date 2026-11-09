import { UserError } from "../src/shared/errors.ts";

/** Reps, per-trial limit, omp profile and dry run for one eval run (0318, 0482). */
export interface RunSettings {
	reps: number;
	/** Undefined: no limit. */
	timeoutMinutes?: number;
	/** omp's `--profile` for every trial (0482 E8); undefined: his default profile. */
	profile?: string;
	/** Print the plan (cases, reps, each turn's arguments) and run nothing. */
	dryRun?: true;
}

/** Defaults (0318): 3 reps, 10 minutes per trial. */
export const RUN_DEFAULTS: RunSettings = { reps: 3, timeoutMinutes: 10 };

/**
 * Parses `bun evals/run.ts [--reps <n>] [--timeout <minutes>] [--profile <name>] [--dry-run]` (0318, 0482). Bad values
 * throw a `UserError` (exit 2). `bun run evals --reps 5` passes the flags through (0317).
 */
export function parseRunArgs(argv: string[]): RunSettings {
	const settings = { ...RUN_DEFAULTS };
	for (let i = 0; i < argv.length; i++) {
		const flag = argv[i];
		if (flag === "--dry-run") {
			settings.dryRun = true;
			continue;
		}
		if (flag !== "--reps" && flag !== "--timeout" && flag !== "--profile")
			throw new UserError(`unknown eval option: ${flag}`);
		const value = argv[++i];
		if (value === undefined) throw new UserError(`${flag} needs a value`);
		const number = Number(value);
		if (flag === "--profile") {
			// omp's profile names (pi-utils `dirs.ts`).
			if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(value) || value.endsWith("."))
				throw new UserError(`--profile needs an omp profile name (a-z, 0-9, ".", "_", "-"), not "${value}"`);
			settings.profile = value;
		} else if (flag === "--reps") {
			if (!/^\d+$/.test(value) || number < 1)
				throw new UserError(`--reps needs a whole number of 1 or more, not "${value}"`);
			settings.reps = number;
		} else {
			if (value.trim() === "" || !Number.isFinite(number) || number <= 0)
				throw new UserError(`--timeout needs a number of minutes above 0, not "${value}"`);
			settings.timeoutMinutes = number;
		}
	}
	return settings;
}

/**
 * Takes every `--variant <name>` off `bun run evals`'s arguments, in order (0375); the rest goes to
 * {@link parseRunArgs}. A missing name throws a `UserError` (exit 2).
 */
export function takeVariantFlags(argv: readonly string[]): { variants: string[]; rest: string[] } {
	const variants: string[] = [];
	const rest: string[] = [];
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i] ?? "";
		if (arg !== "--variant") {
			rest.push(arg);
			continue;
		}
		const name = argv[++i];
		if (name === undefined || name.startsWith("-")) throw new UserError("--variant needs a name");
		variants.push(name);
	}
	return { variants, rest };
}
