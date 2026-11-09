import { spawnSync } from "node:child_process";

/** How long one synchronous git call may take before it is killed (CO-10). */
const GIT_TIMEOUT_MS = 30_000;

/** Runs git through its CLI (0103) in `dir`, throwing when git could not run, was killed or ran past 30 s. */
function run(dir: string, args: readonly string[]) {
	const result = spawnSync("git", ["-C", dir, ...args], {
		encoding: "utf8",
		windowsHide: true,
		timeout: GIT_TIMEOUT_MS,
	});
	const timedOut = (result.error as NodeJS.ErrnoException | undefined)?.code === "ETIMEDOUT";
	if (result.error && !timedOut) throw result.error;
	if (result.status === null) {
		throw new Error(
			`git ${args.join(" ")} was killed (${timedOut ? `after ${GIT_TIMEOUT_MS / 1000} s` : result.signal}) in ${dir}`,
		);
	}
	return result;
}

/**
 * Runs git in `dir` without holding the event loop (hooks, signing and long logs take their time, CO-10); killed after
 * `timeoutMs`, which reads as `code: null`.
 */
export async function gitAsync(
	cwd: string,
	args: string[],
	timeoutMs: number,
): Promise<{ code: number | null; stdout: string; stderr: string }> {
	const proc = Bun.spawn(["git", "-C", cwd, ...args], {
		stdin: "ignore",
		stdout: "pipe",
		stderr: "pipe",
		windowsHide: true,
	});
	let killed = false;
	const timer = setTimeout(() => {
		killed = true;
		proc.kill();
	}, timeoutMs);
	try {
		const [stdout, stderr, code] = await Promise.all([
			new Response(proc.stdout).text(),
			new Response(proc.stderr).text(),
			proc.exited,
		]);
		return { code: killed ? null : code, stdout, stderr };
	} finally {
		clearTimeout(timer);
	}
}

/** Runs git through its CLI (0103) in `dir`. Returns trimmed stdout, or undefined when git exits non-zero. */
export function git(dir: string, args: readonly string[]): string | undefined {
	const result = run(dir, args);
	return result.status === 0 ? result.stdout.trim() : undefined;
}
