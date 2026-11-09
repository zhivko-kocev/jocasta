// The subscription windows (0456): `omp usage --json` (omp 18.12.6, `cli/usage-cli.ts`) reports each account's limits
// with `window.id` ("5h", "7d"), `amount` and `window.resetsAt`. The HUD and the rail show the most used of each window
// across his accounts; one toast goes out when a window passes `QUOTA_WARN`.
import { onPath } from "../../shared/pollers/cli.ts";
import { type QuotaWindow, quotaPercent } from "../protocol.ts";

const WINDOWS: readonly QuotaWindow["window"][] = ["5h", "7d"];

/** The part of a `UsageLimit` (pi-ai `usage.ts`) read here. */
interface Limit {
	window?: { id?: unknown; resetsAt?: unknown };
	amount?: { used?: unknown; limit?: unknown; usedFraction?: unknown; remainingFraction?: unknown; unit?: unknown };
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

/** The 5h and 7d windows of `omp usage --json`'s output: the most used of each across reports; none when absent. */
export function readQuota(json: string): QuotaWindow[] {
	const parsed = JSON.parse(json) as { reports?: { limits?: Limit[] }[] };
	const most = new Map<QuotaWindow["window"], QuotaWindow>();
	for (const report of Array.isArray(parsed.reports) ? parsed.reports : []) {
		for (const limit of Array.isArray(report?.limits) ? report.limits : []) {
			const id = limit?.window?.id;
			if (!WINDOWS.includes(id as QuotaWindow["window"])) continue;
			const used = usedFraction(limit);
			if (used === undefined) continue;
			const window = id as QuotaWindow["window"];
			if ((most.get(window)?.used ?? -1) >= used) continue;
			const resetsAt = num(limit.window?.resetsAt);
			most.set(window, { window, used, ...(resetsAt ? { resetsAt: new Date(resetsAt).toISOString() } : {}) });
		}
	}
	return WINDOWS.flatMap((w) => most.get(w) ?? []);
}

/** A window's share as the HUD says it: `5h 42%`. */
export function quotaPart(q: QuotaWindow): string {
	return `${q.window} ${quotaPercent(q)}%`;
}

/** The HUD's quota, `5h 42% · 7d 18%`; empty when omp reported none. */
export function quotaText(quota: readonly QuotaWindow[]): string {
	return quota.map(quotaPart).join(" · ");
}

const hhmm = (at: Date) => `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;

/**
 * The toast when a window passes the threshold: `Subscription quota: 5h at 84%, resets at 16:30`; the weekly window's
 * reset names its day (`resets Thu 09:00`).
 */
export function quotaWarning(q: QuotaWindow): string {
	const at = q.resetsAt ? new Date(q.resetsAt) : undefined;
	const resets = !at
		? ""
		: q.window === "5h"
			? `, resets at ${hhmm(at)}`
			: `, resets ${at.toLocaleDateString("en-GB", { weekday: "short" })} ${hhmm(at)}`;
	return `Subscription quota: ${quotaPart(q).replace(" ", " at ")}${resets}`;
}

/** `omp usage --json`'s output; throws when omp isn't on PATH or fails. Killed after a minute. */
export async function ompUsage(): Promise<string> {
	const omp = onPath("omp");
	if (!omp) throw new Error("omp is not on PATH");
	const child = Bun.spawn([omp, "usage", "--json"], {
		stdout: "pipe",
		stderr: "pipe",
		stdin: "ignore",
		windowsHide: true,
		timeout: 60_000,
	});
	const [out, err, code] = await Promise.all([
		new Response(child.stdout).text(),
		new Response(child.stderr).text(),
		child.exited,
	]);
	if (code !== 0) throw new Error(`omp usage failed: ${err.trim().split("\n")[0] || `exit ${code}`}`);
	return out;
}
