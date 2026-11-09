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

/** Rejects once `signal` aborts. */
function aborted(signal: AbortSignal): Promise<never> {
	const { promise, reject } = Promise.withResolvers<never>();
	if (signal.aborted) reject(signal.reason);
	else signal.addEventListener("abort", () => reject(signal.reason), { once: true });
	return promise;
}

/**
 * Where `command` is on PATH, or null. Windows spells the variable `Path`, and an environment copied into an options
 * object (a Worker's `process.env`, as Electrobun gave the old companion app) is case-sensitive, so `PATH` can read
 * undefined; the variable is found whatever its case.
 */
export function onPath(command: string, env: Record<string, string | undefined> = process.env): string | null {
	const name = Object.keys(env).find((key) => key.toUpperCase() === "PATH");
	return Bun.which(command, { PATH: (name && env[name]) || "" });
}

/** `gh`/`az` from PATH, with no prompts, killed after a minute or when `signal` aborts. */
export const runCli: RunCli = async (command, args, okCodes = [], signal) => {
	if (args.some((a) => /["%]/.test(a))) throw new Error(`${command}: an argument holds " or %`);
	const exe = onPath(command);
	if (!exe) throw new Error(`${command} is not on PATH`);
	const { argv, verbatim } = commandLine(exe, args);
	const child = Bun.spawn(argv, {
		stdout: "pipe",
		stderr: "pipe",
		stdin: "ignore",
		windowsHide: true,
		windowsVerbatimArguments: verbatim,
		env: { ...process.env, GH_PROMPT_DISABLED: "1", AZURE_CORE_NO_COLOR: "1" },
		timeout: 60_000,
		...(signal ? { signal } : {}),
	});
	const finished = Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
	// On Windows `az` is a `.cmd`: killing cmd.exe leaves its child holding the pipes, so the abort doesn't wait for them.
	const [out, err, code] = await (signal ? Promise.race([finished, aborted(signal)]) : finished);
	if (code !== 0 && !okCodes.includes(code)) {
		throw new Error(`${command} ${args.slice(0, 2).join(" ")} failed: ${err.trim().split("\n")[0] || `exit ${code}`}`);
	}
	return out;
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
