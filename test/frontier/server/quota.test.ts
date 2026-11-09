import { expect, test } from "bun:test";
import { quotaText, quotaWarning, readQuota } from "../../../src/frontier/server/quota.ts";

/** `omp usage --json` as omp 18.12.6 prints it (`cli/usage-cli.ts`), trimmed to what matters here. */
const usage = {
	generatedAt: 1,
	reports: [
		{
			provider: "anthropic",
			fetchedAt: 1,
			limits: [
				{
					id: "anthropic:5h",
					label: "5 Hour",
					scope: { provider: "anthropic", windowId: "5h" },
					window: { id: "5h", label: "5 Hour", resetsAt: new Date(2026, 9, 23, 16, 30).getTime() },
					amount: { usedFraction: 0.42, unit: "percent" },
					status: "ok",
				},
				{
					id: "anthropic:7d",
					label: "7 Day",
					scope: { provider: "anthropic", windowId: "7d" },
					window: { id: "7d", label: "7 Day" },
					amount: { used: 18, unit: "percent" },
				},
				{ id: "anthropic:extra", label: "Extra", scope: {}, window: { id: "extra" }, amount: { usedFraction: 0.99 } },
			],
		},
		{
			provider: "openai-codex",
			fetchedAt: 1,
			limits: [
				// Another account's 5-hour window, more used: the HUD shows the closest to its limit.
				{ id: "codex:5h", window: { id: "5h", label: "5 hours" }, amount: { used: 3, limit: 4, unit: "requests" } },
				{ id: "codex:7d", window: { id: "7d", label: "7 days" }, amount: { remainingFraction: 0.9, unit: "percent" } },
			],
		},
	],
	accountsWithoutUsage: [],
	disabledCredentials: [],
	capacity: {},
};

test("the 5h and 7d windows are the most used of each across accounts, read as omp resolves a used fraction (0456)", () => {
	const quota = readQuota(JSON.stringify(usage));
	expect(quota).toEqual([
		{ window: "5h", used: 0.75 },
		{ window: "7d", used: 0.18 },
	]);
	expect(quotaText(quota)).toBe("5h 75% · 7d 18%");
});

test("no reports, or windows without amounts, give no windows and no HUD text", () => {
	expect(readQuota(JSON.stringify({ reports: [] }))).toEqual([]);
	expect(readQuota(JSON.stringify({ reports: [{ limits: [{ window: { id: "5h" }, amount: {} }] }] }))).toEqual([]);
	expect(quotaText([])).toBe("");
	expect(() => readQuota("not json")).toThrow();
});

test("the toast names the window, its share and when it resets", () => {
	const at = new Date(2026, 9, 23, 16, 30).toISOString();
	expect(quotaWarning({ window: "5h", used: 0.84, resetsAt: at })).toBe(
		"Subscription quota: 5h at 84%, resets at 16:30",
	);
	expect(quotaWarning({ window: "7d", used: 0.91, resetsAt: at })).toBe(
		"Subscription quota: 7d at 91%, resets Fri 16:30",
	);
	expect(quotaWarning({ window: "7d", used: 0.91 })).toBe("Subscription quota: 7d at 91%");
});
