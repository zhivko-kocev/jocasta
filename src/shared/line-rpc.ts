/**
 * Newline-delimited JSON (DUP-8): the one line splitter, and the one child process spoken to by JSON lines over its
 * stdio, as the code index's CBM server (0542, 0562) and omp's auth gateway (0463) are: each request carries a
 * numeric `id`, each answer echoes it, and answers may come in any order.
 */

/** Cuts text into lines: `push` takes the next chunk and returns the lines it completed, trimmed (maybe empty). */
export class LineSplitter {
	private buffered = "";

	push(chunk: string): string[] {
		this.buffered += chunk;
		const lines: string[] = [];
		for (let end = this.buffered.indexOf("\n"); end >= 0; end = this.buffered.indexOf("\n")) {
			lines.push(this.buffered.slice(0, end).trim());
			this.buffered = this.buffered.slice(end + 1);
		}
		return lines;
	}
}

/** How much of the child's stderr is kept for its exit error. */
const STDERR_TAIL = 500;

export interface LineRpcOptions<T> {
	cwd?: string;
	/** The whole environment; the service's own when not given. */
	env?: Record<string, string | undefined>;
	/** The error the waiting calls, and every later one, fail with once the child exited; `stderr` is its last 500 chars. */
	exitError: (code: number, stderr: string) => Error;
	/** A JSON line that is no answer (no numeric `id`), such as the gateway's `{ready, version}`. */
	onOther?: (message: T) => void;
	/** Runs once the child exited and its waiting calls failed. */
	onExit?: () => void;
}

export interface RequestOptions {
	/** The call fails with `timeoutMessage` after this long. */
	timeoutMs?: number;
	timeoutMessage?: string;
	/** The call stops waiting when it aborts, failing with `abortMessage`; the protocol has no cancel line. */
	signal?: AbortSignal;
	abortMessage?: string;
}

/** One running child answering JSON lines by `id`; a line that isn't JSON is skipped. */
export class LineRpc<T extends { id?: unknown }> {
	/** Settles with the exit code once every line the child wrote was read and the calls still waiting failed. */
	readonly exited: Promise<number>;
	private readonly child: Bun.Subprocess<"pipe", "pipe", "pipe">;
	private readonly pending = new Map<number, PromiseWithResolvers<T>>();
	private nextId = 1;
	private gone: Error | undefined;

	/** Starts `argv`; throws when it can't be started, as `Bun.spawn` does. */
	constructor(argv: string[], options: LineRpcOptions<T>) {
		const child = Bun.spawn(argv, {
			...(options.cwd ? { cwd: options.cwd } : {}),
			...(options.env ? { env: options.env } : {}),
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
			windowsHide: true,
		});
		this.child = child;
		let stderrTail = "";
		const answers = (async () => {
			const decoder = new TextDecoder();
			const lines = new LineSplitter();
			for await (const chunk of child.stdout) {
				for (const text of lines.push(decoder.decode(chunk, { stream: true }))) {
					let message: T | null;
					try {
						message = JSON.parse(text) as T | null;
					} catch {
						continue;
					}
					const id = message?.id;
					if (typeof id === "number") this.pending.get(id)?.resolve(message as T);
					else if (message) options.onOther?.(message);
				}
			}
		})().catch(() => undefined);
		const errors = (async () => {
			const decoder = new TextDecoder();
			for await (const chunk of child.stderr) stderrTail = (stderrTail + decoder.decode(chunk)).slice(-STDERR_TAIL);
		})().catch(() => undefined);
		// Every line it wrote is read before the calls still waiting fail.
		this.exited = Promise.all([child.exited, answers, errors]).then(([code]) => {
			this.gone = options.exitError(code, stderrTail);
			for (const waiter of this.pending.values()) waiter.reject(this.gone);
			this.pending.clear();
			options.onExit?.();
			return code;
		});
	}

	/** Sends `message(id)` as one line and resolves the answer with that `id`; a call after the exit throws its error. */
	async request(message: (id: number) => unknown, options: RequestOptions = {}): Promise<T> {
		if (this.gone) throw this.gone;
		const { timeoutMs, signal } = options;
		const aborted = () => new Error(options.abortMessage ?? "aborted");
		if (signal?.aborted) throw aborted();
		const id = this.nextId++;
		const waiter = Promise.withResolvers<T>();
		this.pending.set(id, waiter);
		const timer =
			timeoutMs === undefined
				? undefined
				: setTimeout(
						() => waiter.reject(new Error(options.timeoutMessage ?? `no answer within ${timeoutMs / 1000} s`)),
						timeoutMs,
					);
		// An aborted call stops waiting; its late answer is dropped.
		const abort = () => waiter.reject(aborted());
		signal?.addEventListener("abort", abort, { once: true });
		try {
			this.send(message(id));
			return await waiter.promise;
		} finally {
			clearTimeout(timer);
			this.pending.delete(id);
			signal?.removeEventListener("abort", abort);
		}
	}

	/** Sends one line that gets no answer (a JSON-RPC notification). */
	send(message: unknown): void {
		this.child.stdin.write(`${JSON.stringify(message)}\n`);
		void this.child.stdin.flush();
	}

	/** Lets the service exit while the child still runs. */
	unref(): void {
		this.child.unref();
	}

	/** Closes the child's stdin; a stdio server leaves once it has written its answers. */
	close(): void {
		try {
			void this.child.stdin.end();
		} catch {
			// Already closed.
		}
	}

	kill(): void {
		this.child.kill();
	}
}
