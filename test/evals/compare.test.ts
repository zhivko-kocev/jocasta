import { afterEach, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compareRows } from "../../evals/compare.ts";
import type { ResultRow } from "../../evals/runner.ts";
import { loadVariants, variantArgs } from "../../evals/variants.ts";

const temps: string[] = [];
afterEach(() => {
	for (const dir of temps.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function tempDir(): string {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-evals-compare-"));
	temps.push(dir);
	return dir;
}

function row(
	id: string,
	passed: boolean,
	metrics: { tokens?: number; wallMs?: number; lines?: number; cost?: number } = {},
): ResultRow {
	return {
		case: id,
		rep: 0,
		variant: "default",
		harness: "omp",
		harnessVersion: "omp/0.0.0-test",
		startedAt: "2026-10-24T10:00:00.000Z",
		model: "faux-1",
		usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 },
		totalUsage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: metrics.tokens ?? 100,
			cost: metrics.cost ?? 0,
		},
		requests: 1,
		turns: 1,
		toolCalls: 1,
		prompts: 1,
		wallMs: metrics.wallMs ?? 1000,
		linesAdded: metrics.lines ?? 10,
		linesRemoved: 0,
		grade: {},
		hiddenPassed: null,
		findings: null,
		passed,
	};
}

const trials = (id: string, passes: number, total: number, metrics?: Parameters<typeof row>[2]) =>
	Array.from({ length: total }, (_, i) => row(id, i < passes, metrics));

describe("the keep rule (0375)", () => {
	test("a mean ratio of exactly 0.90 improves; 0.91 doesn't", () => {
		const a = trials("c", 4, 4, { tokens: 1000, wallMs: 1000, lines: 10 });
		const atBoundary = compareRows(a, trials("c", 4, 4, { tokens: 900, wallMs: 1000, lines: 10 }));
		expect(atBoundary.meanRatios.tokens).toBeCloseTo(0.9, 12);
		expect(atBoundary.improved).toEqual(["tokens"]);
		expect(atBoundary.verdict).toBe("keep");
		const above = compareRows(a, trials("c", 4, 4, { tokens: 910, wallMs: 1000, lines: 10 }));
		expect(above.improved).toEqual([]);
		expect(above.verdict).toBe("revert");
	});

	test("the metric is the mean over cases of each case's B/A ratio", () => {
		// 0.8 and 1.0 average to 0.90: improves, though B's total is 1.8/2.
		const a = [...trials("x", 1, 1, { lines: 10 }), ...trials("y", 1, 1, { lines: 30 })];
		const b = [...trials("x", 1, 1, { lines: 8 }), ...trials("y", 1, 1, { lines: 30 })];
		const result = compareRows(a, b);
		expect(result.cases.map((c) => c.ratios.lines)).toEqual([0.8, 1]);
		expect(result.improved).toEqual(["lines"]);
	});

	test("a case whose A mean is 0 is skipped for that metric", () => {
		const a = [...trials("explain", 1, 1, { lines: 0 }), ...trials("fix", 1, 1, { lines: 10 })];
		const b = [...trials("explain", 1, 1, { lines: 50 }), ...trials("fix", 1, 1, { lines: 5 })];
		const result = compareRows(a, b);
		expect(result.cases.find((c) => c.case === "explain")?.ratios.lines).toBeUndefined();
		expect(result.meanRatios.lines).toBe(0.5);
	});

	test("the pass rate holds down to √(100²/nA + 100²/nB) points below A's, not past it", () => {
		const better = { tokens: 50 };
		// nA = nB = 8: the tolerance is exactly 50 points.
		const atEdge = compareRows(trials("c", 8, 8), trials("c", 4, 8, better));
		expect(atEdge.tolerance).toBe(50);
		expect([atEdge.rateA, atEdge.rateB]).toEqual([100, 50]);
		expect(atEdge.passRateHolds).toBe(true);
		expect(atEdge.verdict).toBe("keep");
		const pastEdge = compareRows(trials("c", 8, 8), trials("c", 3, 8, better));
		expect(pastEdge.passRateHolds).toBe(false);
		expect(pastEdge.verdict).toBe("revert");
	});

	test("bun run evals compare prints the table and verdict and writes compare.md into B", () => {
		const dirA = join(tempDir(), "run-a");
		const dirB = join(tempDir(), "run-b");
		for (const [dir, rows] of [
			[dirA, trials("bug-paginate", 2, 2, { tokens: 1000 })],
			[dirB, trials("bug-paginate", 2, 2, { tokens: 500 })],
		] as const) {
			mkdirSync(dir, { recursive: true });
			writeFileSync(join(dir, "results.jsonl"), rows.map((r) => `${JSON.stringify(r)}\n`).join(""));
		}
		const run = spawnSync(
			process.execPath,
			[join(import.meta.dir, "..", "..", "evals", "run.ts"), "compare", dirA, dirB],
			{
				encoding: "utf8",
			},
		);
		expect(run.status).toBe(0);
		expect(run.stdout).toContain("| bug-paginate | 2/2 | 2/2 | 0.50 | 1.00 | 1.00 | 1.00 |");
		expect(run.stdout).toContain("**Verdict: keep.**");
		expect(run.stdout).toContain("## Paired statistics (0482), 1 tasks in both runs");
		expect(readFileSync(join(dirB, "compare.md"), "utf8")).toBe(`${run.stdout.trimEnd()}\n`);
		const missing = spawnSync(
			process.execPath,
			[join(import.meta.dir, "..", "..", "evals", "run.ts"), "compare", dirA],
			{
				encoding: "utf8",
			},
		);
		expect(missing.status).toBe(2);
	});
});

describe("variants (0375)", () => {
	test("later variants layer over earlier ones", () => {
		const dir = tempDir();
		writeFileSync(join(dir, "prompt-a.md"), "a");
		writeFileSync(join(dir, "prompt-b.md"), "b");
		writeFileSync(join(dir, "ext.ts"), "");
		writeFileSync(
			join(dir, "base.yaml"),
			"append_system_prompt: prompt-a.md\nextensions: [ext.ts]\ntools: [read, bash]\nsettings:\n  defaultProvider: github-copilot\n  roles:\n    default: [a]\n    cheap: [b]\nenv:\n  ACME_A: one\n  ACME_B: one\n",
		);
		writeFileSync(
			join(dir, "lean.yaml"),
			"append_system_prompt: prompt-b.md\nextensions: [ext.ts]\ntools: [read]\nsettings:\n  roles:\n    cheap: [c]\nenv:\n  ACME_B: two\n",
		);
		const variant = loadVariants(dir, ["base", "lean"]);
		expect(variant).toEqual({
			label: "base+lean",
			harness: "omp",
			planFirst: false,
			followups: [],
			configs: [],
			appendSystemPrompt: join(dir, "prompt-b.md"),
			extensions: [join(dir, "ext.ts"), join(dir, "ext.ts")],
			tools: ["read"],
			args: [],
			settings: { defaultProvider: "github-copilot", roles: { default: ["a"], cheap: ["c"] } },
			env: { ACME_A: "one", ACME_B: "two" },
		});
		expect(variantArgs(variant)).toEqual([
			"--append-system-prompt",
			join(dir, "prompt-b.md"),
			"-e",
			join(dir, "ext.ts"),
			"-e",
			join(dir, "ext.ts"),
			"--tools",
			"read",
		]);
		expect(loadVariants(dir, [])).toEqual({
			label: "default",
			harness: "omp",
			planFirst: false,
			followups: [],
			configs: [],
			extensions: [],
			args: [],
			env: {},
		});
	});

	test.each([
		["an unknown name", "nope", "unknown variant: nope"],
		["a path for a name", "../base", "unknown variant: ../base"],
		["an unknown key", "bad", "bad.yaml:1: unknown key `modell`"],
		["plan first without a model", "plan", "plan: plan_first needs a model"],
		["omp keys on claude", "both", "both: configs, tools apply to omp only, not claude"],
		["a loop of extends", "loop", "variant loop extends itself: loop → loop"],
	])("%s is refused", (_name, variant, message) => {
		const dir = tempDir();
		writeFileSync(join(dir, "base.yaml"), "tools: [read]\n");
		writeFileSync(join(dir, "bad.yaml"), "modell: x\n");
		writeFileSync(join(dir, "plan.yaml"), "plan_first: true\n");
		writeFileSync(join(dir, "both.yaml"), "extends: [base]\nharness: claude\nconfigs: [base.yaml]\n");
		writeFileSync(join(dir, "loop.yaml"), "extends: [loop]\n");
		expect(() => loadVariants(dir, [variant])).toThrow(message);
	});

	test("the shipped copilot and anthropic variants load", () => {
		const dir = join(import.meta.dir, "..", "..", "evals", "variants");
		expect(loadVariants(dir, ["copilot"]).settings).toEqual({
			modelRoles: { default: "github-copilot/claude-opus-5.5" },
		});
		expect(loadVariants(dir, ["anthropic"]).settings).toEqual({ modelRoles: { default: "anthropic/claude-opus-5-5" } });
	});

	test("the arms A0–A4 build on each other and give each harness its flags (0482)", () => {
		const dir = join(import.meta.dir, "..", "..", "evals", "variants");
		const config = join(import.meta.dir, "..", "..", "config");
		const a0 = loadVariants(dir, ["a0"]);
		expect(variantArgs(a0)).toEqual(["--model", "anthropic/claude-opus-5-5", "--thinking", "high"]);
		const a1 = loadVariants(dir, ["a1"]);
		expect(variantArgs(a1)).toEqual([
			"--model",
			"anthropic/claude-opus-5-5",
			"--config",
			join(config, "guard.yml"),
			"--append-system-prompt",
			join(config, "ponytail.md"),
			"--thinking",
			"high",
		]);
		expect(a1.settings).toMatchObject({ compaction: { thresholdTokens: 300000 }, dev: { autoqa: false } });
		const a2 = loadVariants(dir, ["a2"]);
		expect(variantArgs(a2).slice(0, 5)).toEqual([
			"--model",
			"anthropic/claude-opus-5-5",
			"--plan-yolo",
			"--plan-yolo-into",
			"anthropic/claude-opus-5-5",
		]);
		const a3 = loadVariants(dir, ["a3"]);
		expect(a3.planFirst).toBe(true);
		expect(a3.followups).toHaveLength(2);
		expect(a3.followups[0]).toContain("git diff {base}");
		expect(a3.settings).toMatchObject({
			task: { agentModelOverrides: { reviewer: ["anthropic/claude-opus-5-5"] } },
		});
		const a4 = loadVariants(dir, ["a4"]);
		expect(a4.harness).toBe("claude");
		expect(variantArgs(a4)).toEqual([
			"--model",
			"claude-opus-5-5",
			"--permission-mode",
			"bypassPermissions",
			"--effort",
			"high",
		]);
	});
});
