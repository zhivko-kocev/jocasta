import { expect, test } from "bun:test";
import { waitFor } from "../../src/shared/poll.ts";

test("waitFor checks at once, then every interval, and resolves true once the check holds", async () => {
	let calls = 0;
	expect(await waitFor(() => ++calls === 3, { timeoutMs: 5_000, intervalMs: 1 })).toBe(true);
	expect(calls).toBe(3);
	let once = 0;
	expect(await waitFor(() => ++once > 0, { timeoutMs: 0, intervalMs: 1_000 })).toBe(true);
	expect(once).toBe(1);
});

test("waitFor resolves false past its time, and a check that throws rejects it", async () => {
	const started = performance.now();
	expect(await waitFor(async () => false, { timeoutMs: 60, intervalMs: 20 })).toBe(false);
	expect(performance.now() - started).toBeGreaterThanOrEqual(55);
	await expect(
		waitFor(
			() => {
				throw new Error("broken");
			},
			{ timeoutMs: 100, intervalMs: 10 },
		),
	).rejects.toThrow("broken");
});
