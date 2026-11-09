import { runProgram, runProgramSync } from "./process.ts";

/** How long one synchronous git call may take before it is killed (CO-10). */
const GIT_TIMEOUT_MS = 30_000;

/** What a git call may add: the whole environment it runs with (`GIT_INDEX_FILE`) and what it reads on stdin. */
export interface GitOptions {
	env?: Record<string, string | undefined>;
	stdin?: string | Uint8Array;
}

/**
 * Runs git through its CLI (0103) in `dir` without holding the event loop (hooks, signing and long logs take their
 * time, CO-10). Past `timeoutMs` git and everything it started are ended, which reads as `code: null`.
 */
export async function gitAsync(
	cwd: string,
	args: string[],
	timeoutMs: number,
	options: GitOptions = {},
): Promise<{ code: number | null; stdout: string; stderr: string }> {
	const { code, stdout, stderr } = await runProgram(["git", "-C", cwd, ...args], { ...options, timeoutMs });
	return { code, stdout, stderr };
}

/**
 * Runs git through its CLI (0103) in `dir` synchronously. Returns trimmed stdout, or undefined when git exits
 * non-zero; throws when git could not run, was killed or ran past 30 s.
 */
export function git(dir: string, args: readonly string[]): string | undefined {
	const result = runProgramSync(["git", "-C", dir, ...args], { timeoutMs: GIT_TIMEOUT_MS });
	if (result.code === null) {
		const how = result.timedOut ? `after ${GIT_TIMEOUT_MS / 1000} s` : result.signal;
		throw new Error(`git ${args.join(" ")} was killed (${how}) in ${dir}`);
	}
	return result.code === 0 ? result.stdout.trim() : undefined;
}
