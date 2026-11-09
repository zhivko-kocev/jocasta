/** Polling with a limit: "wait until this holds, at most so long". Real retries (0517's `attempt`) are kept apart. */

/**
 * Asks `check` at once and then every `intervalMs` until it holds or `timeoutMs` pass (on the monotonic clock, so a
 * clock change neither cuts nor stretches the wait); resolves whether it held. A check that throws rejects the wait.
 */
export async function waitFor(
	check: () => boolean | Promise<boolean>,
	{ timeoutMs, intervalMs }: { timeoutMs: number; intervalMs: number },
): Promise<boolean> {
	const end = performance.now() + timeoutMs;
	for (;;) {
		if (await check()) return true;
		const left = end - performance.now();
		if (left <= 0) return false;
		await Bun.sleep(Math.min(intervalMs, left));
	}
}
