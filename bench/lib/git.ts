import { type RunResult, runProgramSync } from "../../src/shared/process.ts";

/**
 * git for the task benchmark and the eval runner: stdout of `git <args>` in `cwd` (with `env`, default this
 * process's), on the product's hidden runner; a failure throws `git <args> failed in <cwd>: <stderr>`. A large
 * change's `diff --numstat` passes the 1 MiB default buffer (EVL-4), hence the 256 MiB one. No time limit: the
 * product's 30 s (CO-10) would cut a task repo's `git fetch`.
 */
export function git(cwd: string, args: string[], env?: Record<string, string | undefined>): string {
	const failed = (why: string) => new Error(`git ${args.join(" ")} failed in ${cwd}: ${why}`);
	let result: RunResult;
	try {
		result = runProgramSync(["git", ...args], { cwd, ...(env ? { env } : {}), maxBytes: 1 << 28 });
	} catch (error) {
		throw failed((error as Error).message);
	}
	if (result.code !== 0) throw failed(result.stderr);
	return result.stdout;
}
