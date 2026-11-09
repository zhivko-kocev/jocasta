/**
 * Running a program to its end with its output captured (CO-10, LC-11): one runner, so every run hides its console
 * window (the service has none of its own, 0552), ignores stdin unless given, keeps a bounded copy of its output, and
 * on its time limit or an abort ends the program's whole tree (`killTree`, start-checked), never only the direct
 * child: a `.cmd` shim's cmd.exe or Bun's `omp.exe` shim would leave the real program running and holding the pipes.
 */
import { spawnSync } from "node:child_process";
import { killTree } from "./proctree.ts";

/**
 * Where `command` is on PATH, or null. Windows spells the variable `Path`, and an environment copied into an options
 * object (a Worker's `process.env`, as Electrobun gave the old companion app) is case-sensitive, so `PATH` can read
 * undefined; the variable is found whatever its case.
 */
export function onPath(command: string, env: Record<string, string | undefined> = process.env): string | null {
	const name = Object.keys(env).find((key) => key.toUpperCase() === "PATH");
	return Bun.which(command, { PATH: (name && env[name]) || "" });
}

/** How much of each of stdout and stderr a run keeps by default; the rest is read and dropped. */
const MAX_OUTPUT_BYTES = 64 * 1024 * 1024;

/** After its tree was ended, how long a run still waits for its pipes before it settles with what it has. */
const PIPE_GRACE_MS = 1_000;

export interface RunOptions {
	cwd?: string;
	/** The whole environment (not merged with `process.env`). */
	env?: Record<string, string | undefined>;
	/** What stdin reads; it is ignored otherwise. */
	stdin?: string | Uint8Array;
	/** Ends the program's tree after this long; the result says `timedOut` and has `code: null`. */
	timeoutMs?: number;
	/** Ends the program's tree when it aborts; the run then rejects with the signal's reason at once. */
	signal?: AbortSignal;
	/** How much of each of stdout and stderr is kept (bytes); 64 MiB by default. */
	maxBytes?: number;
	/** cmd.exe's command line as given (a `.cmd` quoted by the caller). */
	windowsVerbatimArguments?: boolean;
}

export interface RunResult {
	/** The exit code; null when it was ended at its time limit or by a signal. */
	code: number | null;
	/** The signal that ended it, when one did. */
	signal: string | null;
	stdout: string;
	stderr: string;
	timedOut: boolean;
}

/** A stream read to its end, keeping its first `max` bytes; `done` settles when the stream ends. */
function capture(stream: ReadableStream<Uint8Array>, max: number): { text(): string; done: Promise<void> } {
	const chunks: Uint8Array[] = [];
	let kept = 0;
	const done = (async () => {
		for await (const chunk of stream) {
			if (kept >= max) continue;
			const part = kept + chunk.length > max ? chunk.subarray(0, max - kept) : chunk;
			chunks.push(part);
			kept += part.length;
		}
	})().catch(() => undefined);
	return { text: () => Buffer.concat(chunks).toString("utf8"), done };
}

/**
 * Runs `argv` to its end and resolves its exit code and output. A program that can't be started (not found) throws
 * as `Bun.spawn` does, so each caller keeps its own text for it.
 */
export async function runProgram(argv: string[], options: RunOptions = {}): Promise<RunResult> {
	const { signal } = options;
	signal?.throwIfAborted();
	const max = options.maxBytes ?? MAX_OUTPUT_BYTES;
	const child = Bun.spawn(argv, {
		...(options.cwd ? { cwd: options.cwd } : {}),
		...(options.env ? { env: options.env } : {}),
		stdin: options.stdin === undefined ? "ignore" : Buffer.from(options.stdin),
		stdout: "pipe",
		stderr: "pipe",
		windowsHide: true,
		...(options.windowsVerbatimArguments ? { windowsVerbatimArguments: true } : {}),
	});
	const stdout = capture(child.stdout, max);
	const stderr = capture(child.stderr, max);
	const finished = Promise.all([child.exited, stdout.done, stderr.done]).then(([code]) => code);
	let ended: Promise<unknown> | undefined;
	const end = () => {
		if (child.exitCode === null && child.signalCode === null) ended ??= killTree(child.pid).catch(() => []);
	};
	let timedOut = false;
	// Ended at its limit: a grandchild that escaped the walk may still hold the pipes, so the run settles once the
	// program is gone and its pipes had a moment, with the output read so far.
	const limit = Promise.withResolvers<null>();
	const timer =
		options.timeoutMs === undefined
			? undefined
			: setTimeout(() => {
					timedOut = true;
					end();
					void (ended ?? Promise.resolve())
						.then(() => child.exited)
						.then(() => Bun.sleep(PIPE_GRACE_MS))
						.then(() => limit.resolve(null));
				}, options.timeoutMs);
	const aborted = Promise.withResolvers<never>();
	const onAbort = () => {
		end();
		aborted.reject(signal?.reason);
	};
	signal?.addEventListener("abort", onAbort, { once: true });
	try {
		const code = await Promise.race([finished, aborted.promise, limit.promise]);
		const exit = { code: timedOut ? null : code, signal: child.signalCode == null ? null : String(child.signalCode) };
		return { ...exit, stdout: stdout.text(), stderr: stderr.text(), timedOut };
	} finally {
		clearTimeout(timer);
		signal?.removeEventListener("abort", onAbort);
	}
}

/**
 * `runProgram` for the callers that must run synchronously (git on the service's sync paths). Node's synchronous
 * spawn can only end the direct child at `timeoutMs`; a program that couldn't start throws.
 */
export function runProgramSync(argv: string[], options: Omit<RunOptions, "signal"> = {}): RunResult {
	const [program = "", ...args] = argv;
	const result = spawnSync(program, args, {
		...(options.cwd ? { cwd: options.cwd } : {}),
		...(options.env ? { env: options.env } : {}),
		...(options.stdin === undefined ? { stdio: ["ignore", "pipe", "pipe"] } : { input: options.stdin }),
		encoding: "utf8",
		windowsHide: true,
		maxBuffer: options.maxBytes ?? MAX_OUTPUT_BYTES,
		...(options.timeoutMs === undefined ? {} : { timeout: options.timeoutMs }),
		...(options.windowsVerbatimArguments ? { windowsVerbatimArguments: true } : {}),
	});
	const timedOut = (result.error as NodeJS.ErrnoException | undefined)?.code === "ETIMEDOUT";
	if (result.error && !timedOut) throw result.error;
	const exit = { code: timedOut ? null : result.status, signal: result.signal };
	return { ...exit, stdout: result.stdout ?? "", stderr: result.stderr ?? "", timedOut };
}
