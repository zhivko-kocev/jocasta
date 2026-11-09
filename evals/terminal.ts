import { tmpdir } from "node:os";
import { herdrRequest } from "../bench/lib/herdr.ts";
import { launchInPane } from "../bench/lib/launch.ts";
import { refuseHisSession } from "../bench/lib/sandbox.ts";
import { connectHerdr, type Herdr } from "../src/frontier/server/herdr.ts";
import { killTree, pidAlive } from "../src/shared/proctree.ts";

/**
 * A terminal for a trial turn that needs one (0567: omp's interactive plan review needs a TTY on stdin, `main.ts`).
 * Bun's PTY is POSIX-only, so on Windows the turn runs in a tab of a separate bench herdr session (`--pty-session`,
 * default `BENCH_PTY_SESSION`, as H5 and H6 use; never his or a sandbox Jocasta's), in the workspace
 * {@link TRIAL_WORKSPACE}. The turn ends when the tab's process is gone; past the limit its tree is killed. herdr
 * doesn't report the exit code: the caller judges the turn by what the turn itself logged.
 */

export const TRIAL_WORKSPACE = "bench-trials";
const POLL_MS = 1_000;

export type TerminalRunner = (
	command: string,
	args: string[],
	cwd: string,
	env: Record<string, string | undefined>,
	timeoutMs: number | undefined,
) => Promise<{ timedOut: boolean; wallMs: number }>;

/** What {@link herdrTerminal} talks to; tests give a fake. */
export interface TerminalDeps {
	/**
	 * Where the trials' workspace starts its own default shell (EVL-17): the work root, never a trial's repo, which that
	 * shell would hold for as long as the session lives. The OS temp folder when unset.
	 */
	workspaceCwd?: string;
	connect?: (session: string) => Promise<Pick<Herdr, "ensureWorkspace" | "createTab" | "paneRoot" | "close">>;
	closeTab?: (session: string, tabId: string) => Promise<unknown>;
}

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

/**
 * Runs a turn in a new tab of `session`'s {@link TRIAL_WORKSPACE} and waits for its process to end. The tab's process
 * id is read once through the session's own client, asked again until herdr names it (a dropped answer is asked
 * again, 0517); from then on the process is watched locally (EVL-6), so a herdr hiccup never ends a turn early.
 */
export function herdrTerminal(session: string, deps: TerminalDeps = {}): TerminalRunner {
	refuseHisSession(session, "the trials' terminal session");
	const connect = deps.connect ?? ((name: string) => connectHerdr({ session: name }));
	const closeTab =
		deps.closeTab ?? ((name: string, tabId: string) => herdrRequest(name, "tab.close", { tab_id: tabId }));
	return async (command, args, cwd, env, timeoutMs) => {
		const started = performance.now();
		const late = () => timeoutMs !== undefined && performance.now() - started > timeoutMs;
		const herdr = await connect(session);
		try {
			const workspace = await herdr.ensureWorkspace(TRIAL_WORKSPACE, deps.workspaceCwd ?? tmpdir());
			const pane = await launchInPane(herdr, workspace, [command, ...args], cwd, {
				title: "bench trial",
				env: envDelta(env),
			});
			let pid: number | undefined;
			while (pid === undefined && !late()) {
				pid = await herdr.paneRoot(pane.paneId).catch(() => undefined);
				if (pid === undefined) await Bun.sleep(POLL_MS);
			}
			let timedOut = pid === undefined;
			while (pid !== undefined && pidAlive(pid)) {
				if (late()) {
					timedOut = true;
					await killTree(pid).catch(() => []);
					break;
				}
				await Bun.sleep(POLL_MS);
			}
			await closeTab(session, pane.tabId).catch(() => undefined);
			return { timedOut, wallMs: Math.round(performance.now() - started) };
		} finally {
			herdr.close();
		}
	};
}
