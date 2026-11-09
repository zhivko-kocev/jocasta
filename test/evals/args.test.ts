import { afterEach, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseRunArgs, takeVariantFlags } from "../../evals/args.ts";

const temps: string[] = [];
afterEach(() => {
	for (const dir of temps.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("eval run flags (0318)", () => {
	test("runs default to 3 reps and 10 minutes", () => {
		expect(parseRunArgs([])).toEqual({ reps: 3, timeoutMinutes: 10 });
	});

	test("--reps and --timeout override the defaults", () => {
		expect(parseRunArgs(["--reps", "5", "--timeout", "2.5"])).toEqual({ reps: 5, timeoutMinutes: 2.5 });
		expect(parseRunArgs(["--timeout", "1"])).toEqual({ reps: 3, timeoutMinutes: 1 });
		expect(parseRunArgs(["--reps", "4"])).toEqual({ reps: 4, timeoutMinutes: 10 });
	});

	test.each([
		[["--reps", "0"], "--reps needs a whole number"],
		[["--reps", "2.5"], "--reps needs a whole number"],
		[["--reps", "-1"], "--reps needs a whole number"],
		[["--reps", "x"], "--reps needs a whole number"],
		[["--timeout", "0"], "--timeout needs a number of minutes"],
		[["--timeout", "-3"], "--timeout needs a number of minutes"],
		[["--timeout", ""], "--timeout needs a number of minutes"],
		[["--timeout", "Infinity"], "--timeout needs a number of minutes"],
		[["--reps"], "--reps needs a value"],
		[["--fast"], "unknown eval option: --fast"],
	])("%p is refused", (argv, message) => {
		expect(() => parseRunArgs(argv)).toThrow(message);
	});

	test.each([
		[["--reps", "0"], 'jocasta: --reps needs a whole number of 1 or more, not "0"'],
		[["--variant", "nope"], "jocasta: unknown variant: nope"],
		[["--variant"], "jocasta: --variant needs a name"],
	])("%p exits 2 with a jocasta: line before any trial runs", (argv, message) => {
		const data = mkdtempSync(join(tmpdir(), "jocasta-evals-args-"));
		temps.push(data);
		const env: Record<string, string | undefined> = { ...process.env, JOCASTA_DATA_DIR: data };
		delete env.JOCASTA_EVALS_REAL;
		const run = spawnSync(process.execPath, [join(import.meta.dir, "..", "..", "evals", "run.ts"), ...argv], {
			env,
			encoding: "utf8",
		});
		expect(run.status).toBe(2);
		expect(run.stderr.trim()).toBe(message);
		expect(run.stdout).toBe("");
	});
});

test("--variant is taken off in order, repeats included (0375)", () => {
	expect(takeVariantFlags(["--variant", "copilot", "--reps", "2", "--variant", "copilot"])).toEqual({
		variants: ["copilot", "copilot"],
		rest: ["--reps", "2"],
	});
});
