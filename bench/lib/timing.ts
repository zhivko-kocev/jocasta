/** Timing for the harness benchmarks: polling with a limit. */

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
