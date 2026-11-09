import { expect, test } from "bun:test";
import {
	type Account,
	createPacer,
	type PacerDeps,
	type Pause,
	pauseFor,
	RECHECK_MS,
	RESET_SLACK_MS,
	readAccounts,
} from "../../evals/pacer.ts";

const NOW = Date.parse("2026-10-24T10:00:00Z");
const at = (minutes: number) => NOW + minutes * 60_000;

/** `omp usage --json` with one report per account: `[5h used, 7d used, 5h reset minutes, 7d reset minutes]`. */
function usageJson(accounts: [number, number, number?, number?][], extra: Record<string, unknown> = {}): string {
	return JSON.stringify({
		generatedAt: NOW,
		reports: [
			...accounts.map(([h5, d7, r5, r7]) => ({
				provider: "anthropic",
				metadata: { email: "someone@example.org" },
				limits: [
					{ window: { id: "5h", ...(r5 === undefined ? {} : { resetsAt: at(r5) }) }, amount: { usedFraction: h5 } },
					{
						window: { id: "7d", ...(r7 === undefined ? {} : { resetsAt: at(r7) }) },
						amount: { used: d7 * 100, unit: "percent" },
					},
				],
			})),
			{ provider: "github-copilot", limits: [{ window: { id: "5h" }, amount: { usedFraction: 0.1 } }] },
		],
		accountsWithoutUsage: [],
		...extra,
	});
}

test("each Anthropic report is one account with its windows, named by position only; other providers are left out", () => {
	const accounts = readAccounts(
		usageJson([
			[0.9, 0.2, 30],
			[0.1, 0.07],
		]),
	);
	expect(accounts).toEqual([
		{
			id: "anthropic#1",
			windows: [
				{ window: "5h", used: 0.9, resetsAt: at(30) },
				{ window: "7d", used: 0.2 },
			],
		},
		{
			id: "anthropic#2",
			windows: [
				{ window: "5h", used: 0.1 },
				{ window: "7d", used: 0.07 },
			],
		},
	]);
	const unreported = readAccounts(usageJson([[0.9, 0.9]], { accountsWithoutUsage: [{ provider: "anthropic" }] }));
	expect(unreported.map((a) => a.windows.length)).toEqual([2, 0]);
});

test("one full account doesn't pause the run (0543: 2026-11-05's second account at its 7d limit, the first at 7%)", () => {
	const accounts = readAccounts(
		usageJson([
			[0.07, 0.07],
			[0.3, 0.86, 60, 3000],
		]),
	);
	expect(pauseFor(accounts, NOW)).toBeNull();
	expect(pauseFor([], NOW)).toBeNull();
	// An account omp lists without a report isn't known to be full.
	expect(
		pauseFor(readAccounts(usageJson([[0.9, 0.9]], { accountsWithoutUsage: [{ provider: "anthropic" }] })), NOW),
	).toBeNull();
});

test("with every account full the run waits for the first account to free up, each at its latest full window's reset", () => {
	const accounts: Account[] = [
		{
			id: "anthropic#1",
			windows: [
				{ window: "5h", used: 0.9, resetsAt: at(30) },
				{ window: "7d", used: 0.95, resetsAt: at(600) },
			],
		},
		{
			id: "anthropic#2",
			windows: [
				{ window: "5h", used: 0.85, resetsAt: at(120) },
				{ window: "7d", used: 0.4 },
			],
		},
	];
	expect(pauseFor(accounts, NOW)).toEqual({
		until: at(120) + RESET_SLACK_MS,
		accounts: [
			{ account: "anthropic#1", window: "7d", used: 0.95 },
			{ account: "anthropic#2", window: "5h", used: 0.85 },
		],
	});
	expect(pauseFor([{ id: "a", windows: [{ window: "7d", used: 1.2 }] }], NOW)?.until).toBe(NOW + RECHECK_MS);
	expect(pauseFor([{ id: "a", windows: [{ window: "5h", used: 0.9, resetsAt: at(-5) }] }], NOW)?.until).toBe(
		NOW + RECHECK_MS,
	);
});

/** A fake usage source that answers each read from `reads` and a clock that sleeps by moving forward. */
function fake(reads: (string | Error)[]) {
	let now = NOW;
	const slept: number[] = [];
	const lines: string[] = [];
	const deps: PacerDeps = {
		usage: async () => {
			const next = reads.shift();
			if (next === undefined) throw new Error("no more reads");
			if (next instanceof Error) throw next;
			return next;
		},
		sleep: async (ms) => {
			slept.push(ms);
			now += ms;
		},
		now: () => now,
		log: (line) => lines.push(line),
	};
	return { deps, slept, lines };
}

test("the pacer waits while every account is full, reads again, and reports a full account to the infra classifier", async () => {
	const { deps, slept, lines } = fake([
		usageJson([
			[0.9, 0.2, 20],
			[0.95, 0.3, 50],
		]),
		usageJson([
			[0.1, 0.2],
			[0.95, 0.3, 30],
		]),
	]);
	const pauses: Pause[] = [];
	const pacer = createPacer(deps, pauses);
	expect(pacer.accountFull()).toBe(false);
	await pacer.pace();
	expect(slept).toEqual([20 * 60_000 + RESET_SLACK_MS]);
	expect(pauses).toHaveLength(1);
	expect(lines[0]).toContain("quota pacer: every account is at 85% or more (anthropic#1 5h 90%, anthropic#2 5h 95%)");
	expect(lines[0]).not.toContain("example.org");
	// The second read: account 2 is still full, so a rate limit in this trial is an infra error.
	expect(pacer.accountFull()).toBe(true);
});

test("a failed usage read is logged and the trial runs", async () => {
	const { deps, slept, lines } = fake([new Error("omp is not on PATH")]);
	const pacer = createPacer(deps);
	await pacer.pace();
	expect(slept).toEqual([]);
	expect(pacer.accountFull()).toBe(false);
	expect(lines).toEqual(["quota pacer: omp usage failed (omp is not on PATH); running the trial unpaced"]);
});
