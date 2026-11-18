// Jocasta's one omp extension (0617, an exception to 0386): a project pane's omp loads it with `-e`. It handles
// `session_stop` and nothing else: when a turn ends it runs the `jocasta pane-check` role, whose text, when it has any,
// is the turn's continuation. Every decision (which tree, which check, once per tree) is the service's. It imports only
// Node's built-ins, since omp loads it from the shipped config folder, and answers nothing on any failure of its own.
import { spawn } from "node:child_process";

/** Holds the role's argv as a JSON array (`PANE_CHECK_ENV` in `src/shared/pane_check.rs`). */
const COMMAND_ENV = "JOCASTA_PANE_CHECK";
/** Under omp's 30 s limit for a handler (`PANE_CHECK_ASK_MS` is shorter than this). */
export const LIMIT_MS = 27_000;

/** What this file uses of omp's extension API (omp 18.20.2): `SessionStopEvent.signal` ends when the settle is aborted. */
interface TurnEndApi {
	on(
		event: "session_stop",
		handler: (event: { signal?: AbortSignal }) => Promise<{ continue: true; additionalContext: string } | undefined>,
	): void;
}

/** The role's argv from the environment, or undefined when it holds none. */
function roleCommand(): string[] | undefined {
	try {
		const value: unknown = JSON.parse(process.env[COMMAND_ENV] ?? "");
		return Array.isArray(value) && value.length > 0 && value.every((word) => typeof word === "string")
			? (value as string[])
			: undefined;
	} catch {
		return undefined;
	}
}

/**
 * What the role prints, or "" when it fails, can't start, runs past the limit or `signal` aborts (then it is ended, and
 * the service owes the failure to the next turn end).
 */
function roleText(command: string[], signal: AbortSignal | undefined): Promise<string> {
	const { promise, resolve } = Promise.withResolvers<string>();
	const [program, ...args] = command as [string, ...string[]];
	let out = "";
	const child = spawn(program, args, { stdio: ["ignore", "pipe", "ignore"], windowsHide: true });
	const end = () => child.kill();
	const timer = setTimeout(end, LIMIT_MS);
	signal?.addEventListener("abort", end, { once: true });
	child.stdout.setEncoding("utf8");
	child.stdout.on("data", (chunk: string) => {
		out += chunk;
	});
	child.on("error", () => {
		clearTimeout(timer);
		signal?.removeEventListener("abort", end);
		resolve("");
	});
	child.on("close", (code) => {
		clearTimeout(timer);
		signal?.removeEventListener("abort", end);
		resolve(code === 0 && !signal?.aborted ? out.trim() : "");
	});
	return promise;
}

export default function paneCheckExtension(omp: TurnEndApi): void {
	omp.on("session_stop", async (event) => {
		try {
			const command = roleCommand();
			const text = command && !event.signal?.aborted ? await roleText(command, event.signal) : "";
			return text ? { continue: true, additionalContext: text } : undefined;
		} catch {
			return undefined;
		}
	});
}
