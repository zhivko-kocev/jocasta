/** Timing for the harness benchmarks: polling with a limit, and JSON lines from logs and session files. */

/**
 * Polls `check` every `intervalMs` until it returns true; resolves the ms from the call to the first true, or null
 * past `timeoutMs`.
 */
export async function waitFor(
	check: () => boolean | Promise<boolean>,
	{ timeoutMs, intervalMs = 100 }: { timeoutMs: number; intervalMs?: number },
): Promise<number | null> {
	const started = performance.now();
	for (;;) {
		if (await check()) return performance.now() - started;
		if (performance.now() - started >= timeoutMs) return null;
		await Bun.sleep(intervalMs);
	}
}

/** Parses JSON lines, skipping blank and non-JSON lines (a log may interleave plain text). */
export function jsonLines<T = Record<string, unknown>>(text: string): T[] {
	const rows: T[] = [];
	for (const line of text.split(/\r?\n/)) {
		const trimmed = line.trim();
		if (!trimmed.startsWith("{")) continue;
		try {
			rows.push(JSON.parse(trimmed) as T);
		} catch {
			// A partly written last line.
		}
	}
	return rows;
}
