import { onPath, runProgram } from "../process.ts";

/**
 * Runs `gh` or `az` and returns stdout; a missing command, or an exit code other than 0 and `okCodes`, throws with its
 * stderr's first line. `signal` ends the command (the service stopping).
 */
export type RunCli = (
	command: "gh" | "az",
	args: string[],
	okCodes?: readonly number[],
	signal?: AbortSignal,
) => Promise<string>;

/**
 * The argv that runs `exe`. On Windows `az` is `az.cmd` under `C:\Program Files`; Bun mangles a `.cmd` at a path with
 * spaces when an argument has spaces too, so cmd.exe gets the line quoted here. The pollers' arguments never hold `"`
 * or `%` (checked below).
 */
function commandLine(exe: string, args: string[]): { argv: string[]; verbatim: boolean } {
	if (process.platform !== "win32" || !/\.(cmd|bat)$/i.test(exe)) return { argv: [exe, ...args], verbatim: false };
	const quoted = [exe, ...args].map((a) => (a === "" || /[\s&|<>^()!,;=']/.test(a) ? `"${a}"` : a)).join(" ");
	return { argv: [process.env.ComSpec ?? "cmd.exe", "/d", "/s", "/c", `"${quoted}"`], verbatim: true };
}

/** `gh`/`az` from PATH, with no prompts, ended after a minute or when `signal` aborts. */
export const runCli: RunCli = async (command, args, okCodes = [], signal) => {
	if (args.some((a) => /["%]/.test(a))) throw new Error(`${command}: an argument holds " or %`);
	const exe = onPath(command);
	if (!exe) throw new Error(`${command} is not on PATH`);
	const { argv, verbatim } = commandLine(exe, args);
	const run = await runProgram(argv, {
		env: { ...process.env, GH_PROMPT_DISABLED: "1", AZURE_CORE_NO_COLOR: "1" },
		timeoutMs: 60_000,
		windowsVerbatimArguments: verbatim,
		...(signal ? { signal } : {}),
	});
	if (run.code === null || (run.code !== 0 && !okCodes.includes(run.code))) {
		const why = run.timedOut ? "ran past a minute" : `exit ${run.code ?? run.signal}`;
		throw new Error(`${command} ${args.slice(0, 2).join(" ")} failed: ${run.stderr.trim().split("\n")[0] || why}`);
	}
	return run.stdout;
};

/** stdout parsed as JSON; empty output is `fallback`. */
export async function runJson<T>(
	run: RunCli,
	command: "gh" | "az",
	args: string[],
	fallback: T,
	okCodes?: readonly number[],
): Promise<T> {
	const out = (await run(command, args, okCodes)).trim();
	return out === "" ? fallback : (JSON.parse(out) as T);
}
