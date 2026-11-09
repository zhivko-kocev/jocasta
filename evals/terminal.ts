import { herdrRequest, paneShellPid } from "../bench/lib/herdr.ts";
import { isAlive, killTree } from "../bench/lib/proctree.ts";
import { connectHerdr } from "../src/frontier/server/herdr.ts";

/**
 * A terminal for a trial turn that needs one (0567: omp's interactive plan review needs a TTY on stdin, `main.ts`).
 * Bun's PTY is POSIX-only, so on Windows the turn runs in a tab of a separate bench herdr session (`--pty-session`,
 * default {@link BENCH_PTY_SESSION}, as H5 and H6 use; never his or a sandbox Jocasta's), in the workspace
 * {@link TRIAL_WORKSPACE}. The turn ends when the tab's process is gone; past the limit its tree is killed. herdr
 * doesn't report the exit code: the caller judges the turn by what the turn itself logged.
 */

export const BENCH_PTY_SESSION = "bench-pty";
export const TRIAL_WORKSPACE = "bench-trials";
const POLL_MS = 1_000;
const HIS_SESSIONS: Record<string, true> = { jocasta: true, jarvis: true };

export type TerminalRunner = (
	command: string,
	args: string[],
	cwd: string,
	env: Record<string, string | undefined>,
	timeoutMs: number | undefined,
) => Promise<{ timedOut: boolean; wallMs: number }>;

/**
 * The variables `env` sets or changes against this process's, for the pane's command (herdr's server supplies the
 * rest); a variable `env` drops becomes empty, so a key removed from the trial's environment stays out of the pane's.
 */
export function envDelta(
	env: Record<string, string | undefined>,
	base: Record<string, string | undefined> = process.env,
): Record<string, string> {
	const delta: Record<string, string> = {};
	for (const [key, value] of Object.entries(env)) if (value !== undefined && value !== base[key]) delta[key] = value;
	for (const [key, value] of Object.entries(base)) if (value !== undefined && env[key] === undefined) delta[key] = "";
	return delta;
}

/** Runs a turn in a new tab of `session`'s {@link TRIAL_WORKSPACE} and waits for its process to end. */
export function herdrTerminal(session: string): TerminalRunner {
	if (HIS_SESSIONS[session.toLowerCase()]) throw new Error(`the trials' terminal session can't be ${session}`);
	return async (command, args, cwd, env, timeoutMs) => {
		const started = performance.now();
		const herdr = await connectHerdr({ session });
		try {
			const workspace = await herdr.ensureWorkspace(TRIAL_WORKSPACE, cwd);
			const pane = await herdr.createTab(workspace, {
				title: "bench trial",
				cwd,
				command: [command, ...args],
				env: envDelta(env),
			});
			let timedOut = false;
			for (;;) {
				const pid = await paneShellPid(session, pane.paneId).catch(() => null);
				if (pid === null || !isAlive(pid)) break;
				if (timeoutMs !== undefined && performance.now() - started > timeoutMs) {
					timedOut = true;
					killTree(pid);
					break;
				}
				await Bun.sleep(POLL_MS);
			}
			await herdrRequest(session, "tab.close", { tab_id: pane.tabId }).catch(() => undefined);
			return { timedOut, wallMs: Math.round(performance.now() - started) };
		} finally {
			herdr.close();
		}
	};
}
