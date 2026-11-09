// The day's stop ends the poller rounds under way and awaits them before the databases close (F7): a slow `gh` round
// used to finish after Stop everything and log "Poller github failed: Cannot use a closed database".
import { expect, test } from "bun:test";
import { Day } from "../../../src/frontier/server/day.ts";
import type { RunCli } from "../../../src/shared/pollers/cli.ts";
import { openCatalog } from "../../../src/shared/schema.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

useTempJocasta();

/** A Day whose `gh` is `run`, the pollers off so the test runs the rounds; its notifications are collected. */
function stand(run: RunCli): { day: Day; notes: string[]; close: () => Promise<void> } {
	const catalog = openCatalog();
	const notes: string[] = [];
	const day = new Day(
		catalog,
		{
			send: () => {},
			alert: () => {},
			notify: (text) => notes.push(text),
			projects: () => [],
			threads: () => [],
			sessionsByProject: () => new Map(),
			prepareSessions: async () => {},
			services: async () => [],
			projectName: () => null,
			leftovers: async () => [],
			weeklyReview: async () => null,
		},
		{ pollers: false, run },
	);
	// The service's order: the day stops, then the catalog closes.
	const close = async () => {
		await day.stop();
		catalog.close();
	};
	return { day, notes, close };
}

/** A pull request of his whose checks failed, so a finished round has something to write; keyed by `gh api`'s route. */
const answers: Record<string, string> = {
	notifications: "[[]]",
	graphql: JSON.stringify({
		data: {
			open: { nodes: [{ url: "https://github.com/acme/api/pull/7" }] },
			recent: {
				nodes: [
					{
						number: 7,
						title: "Fix",
						url: "https://github.com/acme/api/pull/7",
						repository: { nameWithOwner: "acme/api" },
						commits: {
							nodes: [
								{
									commit: {
										statusCheckRollup: {
											contexts: {
												nodes: [
													{
														name: "ci",
														status: "COMPLETED",
														conclusion: "FAILURE",
														detailsUrl: "https://ci.example.org/7",
													},
												],
											},
										},
									},
								},
							],
						},
					},
				],
			},
		},
	}),
};

test("a slow round that ignores the stop is awaited, and writes nothing once the day stopped", async () => {
	const asked = Promise.withResolvers<void>();
	const slow = Promise.withResolvers<void>();
	const calls: string[] = [];
	const { day, notes, close } = stand(async (_command, args) => {
		calls.push(args[1] ?? "");
		asked.resolve();
		await slow.promise;
		return answers[args[1] ?? ""] ?? "";
	});
	const round = day.poll("github");
	await asked.promise;
	let closed = false;
	const closing = close().then(() => {
		closed = true;
	});
	// The stop waits for the round's `gh` while it runs.
	await Promise.resolve();
	expect(closed).toBe(false);
	slow.resolve();
	await closing;
	// Stop resolved only after the round's last `gh` call returned, before the catalog closed.
	expect(calls).toEqual(["notifications", "graphql"]);
	await round;
	expect(notes).toEqual([]);
});

test("a round under way is ended by the stop: `gh` gets the abort and nothing is reported", async () => {
	const asked = Promise.withResolvers<AbortSignal | undefined>();
	const { day, notes, close } = stand((_command, _args, _ok, signal) => {
		const { promise, reject } = Promise.withResolvers<string>();
		signal?.addEventListener("abort", () => reject(new Error("gh api notifications failed: exit 143")));
		asked.resolve(signal);
		return promise;
	});
	void day.poll("github");
	const signal = await asked.promise;
	expect(signal?.aborted).toBe(false);
	// Without the abort this `gh` never answers, and the stop would never resolve.
	await close();
	expect(signal?.aborted).toBe(true);
	expect(notes).toEqual([]);
});

test("no round starts once the day stopped", async () => {
	const calls: string[] = [];
	const { day, notes, close } = stand(async (_command, args) => {
		calls.push(args[0] ?? "");
		return "[]";
	});
	await close();
	await day.poll("github");
	expect(calls).toEqual([]);
	expect(notes).toEqual([]);
});
