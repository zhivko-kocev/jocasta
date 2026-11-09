// The subscription windows (0456): `omp usage --json` (omp 18.12.6, `cli/usage-cli.ts`) reports each account's limits
// with `window.id` ("5h", "7d"), `amount` and `window.resetsAt`. The HUD and the rail show the most used of each window
// across his accounts; one toast goes out when a window passes `QUOTA_WARN`.
import { hhmm, weekday } from "../../shared/day.ts";
import { onPath, runProgram } from "../../shared/process.ts";
import { type QuotaWindow, quotaPercent } from "../protocol.ts";

const WINDOWS: readonly QuotaWindow["window"][] = ["5h", "7d"];

/** The part of a `UsageLimit` (pi-ai `usage.ts`) read here. */
interface Limit {
	window?: { id?: unknown; resetsAt?: unknown };
	amount?: { used?: unknown; limit?: unknown; usedFraction?: unknown; remainingFraction?: unknown; unit?: unknown };
}

/** `omp usage --json`'s output as read here (`accountsWithoutUsage`: the accounts it has no report for). */
export interface UsageJson {
	reports?: { provider?: unknown; limits?: Limit[] }[];
	accountsWithoutUsage?: { provider?: unknown }[];
}

/** One report's 5h or 7d window: its used fraction (above 1 past the limit) and its reset in epoch ms when known. */
export interface UsageWindow {
	window: QuotaWindow["window"];
	used: number;
	resetsAt?: number;
}

const num = (value: unknown): number | undefined =>
	typeof value === "number" && Number.isFinite(value) ? value : undefined;

/** A limit's used fraction, as omp's `resolveUsedFraction` resolves it: fraction, used/limit, percent, remaining. */
function usedFraction(limit: Limit): number | undefined {
	const amount = limit.amount ?? {};
	const fraction = num(amount.usedFraction);
	if (fraction !== undefined) return fraction;
	const used = num(amount.used);
	const max = num(amount.limit);
	if (used !== undefined && max !== undefined && max > 0) return used / max;
	if (amount.unit === "percent" && used !== undefined) return used / 100;
	const remaining = num(amount.remainingFraction);
	return remaining === undefined ? undefined : Math.max(0, 1 - remaining);
}

/**
 * Each report of `omp usage --json` (one per account), in omp's order: its provider and its 5h and 7d windows with a
 * used fraction. The HUD's {@link readQuota} and the benchmark's quota pacer (`evals/pacer.ts`, 0543) both read it.
 */
export function usageReports(parsed: UsageJson): { provider: unknown; windows: UsageWindow[] }[] {
	return (Array.isArray(parsed.reports) ? parsed.reports : []).map((report) => {
		const windows: UsageWindow[] = [];
		for (const limit of Array.isArray(report?.limits) ? report.limits : []) {
			const id = limit?.window?.id;
			if (!WINDOWS.includes(id as QuotaWindow["window"])) continue;
			const used = usedFraction(limit);
			if (used === undefined) continue;
			const resetsAt = num(limit.window?.resetsAt);
			windows.push({ window: id as QuotaWindow["window"], used, ...(resetsAt === undefined ? {} : { resetsAt }) });
		}
		return { provider: report?.provider, windows };
	});
}

/** The 5h and 7d windows of `omp usage --json`'s output: the most used of each across reports; none when absent. */
export function readQuota(json: string): QuotaWindow[] {
	const most = new Map<QuotaWindow["window"], QuotaWindow>();
	for (const report of usageReports(JSON.parse(json) as UsageJson)) {
		for (const { window, used, resetsAt } of report.windows) {
			if ((most.get(window)?.used ?? -1) >= used) continue;
			most.set(window, { window, used, ...(resetsAt ? { resetsAt: new Date(resetsAt).toISOString() } : {}) });
		}
	}
	return WINDOWS.flatMap((w) => most.get(w) ?? []);
}

/** A window's share as the HUD says it: `5h 42%`. */
function quotaPart(q: QuotaWindow): string {
	return `${q.window} ${quotaPercent(q)}%`;
}

/**
 * The toast when a window passes the threshold: `Subscription quota: 5h at 84%, resets at 16:30`; the weekly window's
 * reset names its day (`resets Thu 09:00`).
 */
export function quotaWarning(q: QuotaWindow): string {
	const at = q.resetsAt ? new Date(q.resetsAt) : undefined;
	const resets = !at ? "" : q.window === "5h" ? `, resets at ${hhmm(at)}` : `, resets ${weekday(at)} ${hhmm(at)}`;
	return `Subscription quota: ${quotaPart(q).replace(" ", " at ")}${resets}`;
}

/** `omp usage --json`'s output; throws when omp isn't on PATH or fails. Ended after a minute. */
export async function ompUsage(): Promise<string> {
	const omp = onPath("omp");
	if (!omp) throw new Error("omp is not on PATH");
	const run = await runProgram([omp, "usage", "--json"], { timeoutMs: 60_000 });
	if (run.code !== 0) {
		const why = run.timedOut ? "ran past a minute" : `exit ${run.code ?? run.signal}`;
		throw new Error(`omp usage failed: ${run.stderr.trim().split("\n")[0] || why}`);
	}
	return run.stdout;
}
