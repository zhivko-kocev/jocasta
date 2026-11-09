import { ompUsage, type UsageJson, type UsageWindow, usageReports } from "../src/frontier/server/quota.ts";

/**
 * The quota pacer (0482, amended by 0543): before each trial it reads `omp usage --json` and waits only while every
 * signed-in account of the arms' provider has its 5-hour or 7-day window at or past {@link PAUSE_AT} used; then it
 * holds the run until the first of those accounts frees up (the latest reset among its full windows), and reads again.
 * omp spreads sessions across his accounts, so one full account doesn't stop the run; a trial that hits a full
 * account's rate limit is an infra error and retried (`classifyInfra`'s `accountFull`). An account omp lists without a
 * usage report counts as not full. A window without a known reset is read again after {@link RECHECK_MS}.
 */

/** The share of a window at which an account counts as full (0482). */
export const PAUSE_AT = 0.85;
/** How long to wait before reading again when omp reports no reset time. */
export const RECHECK_MS = 10 * 60_000;
/** Slack after a reset, so the next read sees the new window. */
export const RESET_SLACK_MS = 60_000;
/** The provider whose accounts the arms' trials use (every arm's model is Anthropic's; A7 bills its API key). */
export const PACER_PROVIDER = "anthropic";

export interface Account {
	/** `<provider>#<n>` in omp's order: run records name no account (0482: no user names). */
	id: string;
	windows: UsageWindow[];
}

export interface Pause {
	/** Epoch ms to wait until. */
	until: number;
	/** Each account's fullest window at the pause. */
	accounts: { account: string; window: UsageWindow["window"]; used: number }[];
}

/**
 * The provider's accounts in `omp usage --json`: each report is one account with its 5h and 7d windows; each
 * `accountsWithoutUsage` entry of the provider is an account with none.
 */
export function readAccounts(json: string, provider = PACER_PROVIDER): Account[] {
	const parsed = JSON.parse(json) as UsageJson;
	const accounts: Account[] = [];
	const ofProvider = (value: unknown) => typeof value === "string" && value.toLowerCase() === provider;
	for (const report of usageReports(parsed)) {
		if (!ofProvider(report.provider)) continue;
		accounts.push({ id: `${provider}#${accounts.length + 1}`, windows: report.windows });
	}
	for (const account of Array.isArray(parsed.accountsWithoutUsage) ? parsed.accountsWithoutUsage : [])
		if (ofProvider(account?.provider)) accounts.push({ id: `${provider}#${accounts.length + 1}`, windows: [] });
	return accounts;
}

/** Whether an account has a window at or past {@link PAUSE_AT}. */
export const isFull = (account: Account): boolean => account.windows.some((window) => window.used >= PAUSE_AT);

/**
 * The pause the accounts call for at `now`: none while any account isn't full (or omp lists none); otherwise until the
 * earliest moment one of them frees up, each freeing at the latest reset among its full windows.
 */
export function pauseFor(accounts: readonly Account[], now: number): Pause | null {
	if (accounts.length === 0 || !accounts.every(isFull)) return null;
	let until = Number.POSITIVE_INFINITY;
	for (const account of accounts) {
		let frees = now;
		for (const window of account.windows) {
			if (window.used < PAUSE_AT) continue;
			const reset =
				window.resetsAt !== undefined && window.resetsAt > now ? window.resetsAt + RESET_SLACK_MS : now + RECHECK_MS;
			frees = Math.max(frees, reset);
		}
		until = Math.min(until, frees);
	}
	return {
		until,
		accounts: accounts.map((account) => {
			const fullest = account.windows.reduce((a, b) => (b.used > a.used ? b : a));
			return { account: account.id, window: fullest.window, used: fullest.used };
		}),
	};
}

export interface PacerDeps {
	/** `omp usage --json`'s output. */
	usage(): Promise<string>;
	sleep(ms: number): Promise<void>;
	now(): number;
	log(line: string): void;
}

export const realPacerDeps: PacerDeps = {
	usage: ompUsage,
	sleep: (ms) => Bun.sleep(ms),
	now: () => Date.now(),
	log: (line) => process.stderr.write(`${line}\n`),
};

/** The runner's `pace` hook, and whether the last reading had a full account (for `classifyInfra`). */
export interface Pacer {
	pace(): Promise<void>;
	/** True when the latest reading had at least one full account; false before any reading or after a failed one. */
	accountFull(): boolean;
}

/**
 * The pacer. A failed read is logged and the trial runs: without omp's numbers nothing says to wait, and a run that
 * hits the limit retries its trials as infra errors. Each pause is logged with every account's fullest window and the
 * time it waits until; `pauses` collects them for the run's record.
 */
export function createPacer(deps: PacerDeps = realPacerDeps, pauses: Pause[] = []): Pacer {
	let full = false;
	return {
		accountFull: () => full,
		pace: async () => {
			for (;;) {
				let accounts: Account[];
				try {
					accounts = readAccounts(await deps.usage());
				} catch (error) {
					full = false;
					deps.log(`quota pacer: omp usage failed (${(error as Error).message}); running the trial unpaced`);
					return;
				}
				full = accounts.some(isFull);
				const pause = pauseFor(accounts, deps.now());
				if (!pause) return;
				pauses.push(pause);
				const who = pause.accounts.map((a) => `${a.account} ${a.window} ${Math.round(a.used * 100)}%`).join(", ");
				deps.log(
					`quota pacer: every account is at ${PAUSE_AT * 100}% or more (${who}); waiting until ${new Date(pause.until).toISOString()}`,
				);
				await deps.sleep(Math.max(0, pause.until - deps.now()));
			}
		},
	};
}
