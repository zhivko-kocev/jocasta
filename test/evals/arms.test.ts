import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative } from "node:path";
import { DEFAULT_ARMS, mcpProblem } from "../../evals/bench.ts";
import { FOLLOWUP_ARMS, FOLLOWUP_REPS, FOLLOWUP_TASKS, pickFollowupTasks } from "../../evals/followup.ts";
import {
	A7_API_KEY_ENV,
	claudeSessions,
	opencodeSessions,
	readOpencodeResponse,
	saveSessions,
} from "../../evals/harness.ts";
import { LAYER_MODULE, type LayerTrial, LayerUnavailableError, layerBuilt, loadBenchLayer } from "../../evals/layer.ts";
import { trialArgs, trialEnv } from "../../evals/runner.ts";
import { type BenchTask, planTrials, TASK_PROMPT_PREFIX, type TaskLanguage } from "../../evals/tasks.ts";
import { loadVariants } from "../../evals/variants.ts";
import { benchLayer } from "../../src/frontier/layer/bench.ts";
import { briefTitle } from "../../src/frontier/server/wording.ts";
import { useTempDirs } from "../shared/fixture.ts";

const VARIANTS = join(import.meta.dir, "..", "..", "evals", "variants");
const REPO = join(import.meta.dir, "..", "..");
const tempDir = useTempDirs("jocasta-arms-");
const inside = (folder: string, path: string) => {
	const rel = relative(folder, path);
	return !rel.startsWith("..") && !isAbsolute(rel);
};

const task = (id: string, language: TaskLanguage, set: "main" | "spare" = "main") =>
	({ instance_id: id, language, set }) as BenchTask;
/** The main set's mix: 12 Python, 12 TypeScript/JavaScript, 6 Go. */
const mainSet = [
	...Array.from({ length: 12 }, (_, i) => task(`py-${i}`, "Python")),
	...Array.from({ length: 8 }, (_, i) => task(`ts-${i}`, "TypeScript")),
	...Array.from({ length: 4 }, (_, i) => task(`js-${i}`, "JavaScript")),
	...Array.from({ length: 6 }, (_, i) => task(`go-${i}`, "Go")),
	task("spare-0", "Go", "spare"),
];

test("the main run is 30 tasks × 3 reps × 7 arms = 630 trials (0543)", () => {
	expect(DEFAULT_ARMS).toEqual(["a0", "a1", "a2", "a3", "a4", "a6", "a7"]);
	expect(
		planTrials(
			mainSet.filter((t) => t.set === "main"),
			DEFAULT_ARMS,
			3,
			482,
		),
	).toHaveLength(630);
	for (const arm of DEFAULT_ARMS) expect(() => loadVariants(VARIANTS, [arm])).not.toThrow();
});

test("A6 is A3 plus the layer, plan first through omp's plan review answered 'execute'; every omp arm warms only while streaming (0567)", () => {
	const a3 = loadVariants(VARIANTS, ["a3"]);
	const a6 = loadVariants(VARIANTS, ["a6"]);
	expect(a6.layer).toEqual({ index: true });
	expect(a6.followups).toEqual(a3.followups);
	expect(a6.planFirst).toBe(false);
	expect(a6.planReview).toBe("execute");
	expect(a3.planFirst).toBe(true);
	expect(a3.planReview).toBeUndefined();
	expect(a6.settings).toMatchObject({
		modelRoles: { default: "anthropic/claude-opus-5-5:high", plan: "anthropic/claude-opus-5-5:high" },
	});
	for (const arm of ["a0", "a1", "a2", "a3", "a6", "f-index-off", "f-prewalk", "f-keep-context", "f-ponytail-if"])
		expect(loadVariants(VARIANTS, [arm]).settings).toMatchObject({ providers: { cacheWarming: "streaming" } });
	// 0574: the two arms whose harness updates itself have it off.
	expect(loadVariants(VARIANTS, ["a4"]).env).toEqual({ DISABLE_AUTOUPDATER: "1" });
	expect(loadVariants(VARIANTS, ["a7"]).env).toEqual({ OPENCODE_DISABLE_AUTOUPDATE: "1" });
	// A6's first turn is interactive: no -p, the extension, the prompt last; its review turns are A3's print turns.
	const first = trialArgs(a6, {
		prompt: "Fix it",
		followUp: false,
		overlay: "o.yml",
		sessionDir: "s",
		extension: "x.js",
	});
	expect(first).not.toContain("-p");
	expect(first).not.toContain("--plan-yolo");
	expect(first.slice(-5)).toEqual(["--config", "o.yml", "-e", "x.js", "Fix it"]);
	expect(trialArgs(a6, { prompt: "review", followUp: true, overlay: "o.yml", sessionDir: "s" }).slice(-4)).toEqual([
		"-p",
		"review",
		"--mode",
		"json",
	]);
	const dir = tempDir();
	writeFileSync(join(dir, "both.yaml"), "model: m\nplan_first: true\nplan_review: execute\n");
	expect(() => loadVariants(dir, ["both"])).toThrow(/two ways to plan first/);
	const a7 = loadVariants(VARIANTS, ["a7"]);
	expect(a7).toMatchObject({
		harness: "opencode",
		model: "anthropic/claude-opus-5-5",
		args: ["--variant", "high", "--auto"],
	});
	expect(a7.layer).toBeUndefined();
	expect(trialArgs(a7, { prompt: "Fix it", followUp: false, overlay: "x.yml" })).toEqual([
		"run",
		"--model",
		"anthropic/claude-opus-5-5",
		"--variant",
		"high",
		"--auto",
		"--format",
		"json",
		"Fix it",
	]);
	expect(trialArgs(a7, { prompt: "again", followUp: true, overlay: "x.yml", session: "ses_abc" }).slice(0, 3)).toEqual([
		"run",
		"--session",
		"ses_abc",
	]);
	expect(() => trialArgs(a7, { prompt: "again", followUp: true, overlay: "x.yml" })).toThrow(/first turn's session/);
});

test("omp-only keys are refused on opencode", () => {
	const dir = tempDir();
	writeFileSync(join(dir, "oc.yaml"), "harness: opencode\nlayer: {}\nsettings: { a: 1 }\n");
	expect(() => loadVariants(dir, ["oc"])).toThrow(/settings, layer apply to omp only, not opencode/);
});

test("the follow-up study: 10 main tasks in the main set's language proportions × 2 reps × A6 and its four arms (0571, 0575)", () => {
	expect(FOLLOWUP_ARMS).toEqual(["a6", "f-index-off", "f-prewalk", "f-keep-context", "f-ponytail-if"]);
	const picked = pickFollowupTasks(mainSet, 482);
	expect(picked).toHaveLength(FOLLOWUP_TASKS);
	const count = (languages: TaskLanguage[]) => picked.filter((t) => languages.includes(t.language)).length;
	expect([count(["Python"]), count(["TypeScript", "JavaScript"]), count(["Go"])]).toEqual([4, 4, 2]);
	expect(picked.some((t) => t.set === "spare")).toBe(false);
	expect(pickFollowupTasks(mainSet, 482)).toEqual(picked);
	expect(planTrials(picked, FOLLOWUP_ARMS, FOLLOWUP_REPS, 482)).toHaveLength(100);
});

test("each follow-up arm differs from A6 in its one part only", () => {
	const a6 = loadVariants(VARIANTS, ["a6"]);
	const layer = a6.layer;
	if (layer === undefined) throw new Error("a6 has no layer");
	const arm = (name: string) => loadVariants(VARIANTS, [name]);
	expect(arm("f-index-off").layer).toEqual({ ...layer, index: false });
	expect(arm("f-keep-context").layer).toEqual(layer);
	expect(arm("f-keep-context").planReview).toBe("keep");
	expect(arm("f-prewalk").args).toEqual([...a6.args, "--prewalk"]);
	const ponytail = arm("f-ponytail-if").appendSystemPrompt as string;
	expect(ponytail).toEndWith("ponytail-important-if.md");
	const plain = readFileSync(a6.appendSystemPrompt as string, "utf8");
	const wrapped = readFileSync(ponytail, "utf8");
	// Same Ponytail body; only the Running tests rules sit inside the conditional block.
	const body = (text: string) => text.slice(text.indexOf("# Ponytail, lazy senior dev mode"));
	expect(
		body(wrapped).replace('<important if="you are running or writing tests">\n', "").replace("</important>\n", ""),
	).toBe(body(plain));
	expect(wrapped).toContain('<important if="you are running or writing tests">\n- Save test output');
});

test("A6 fails loud without Jocasta's layer build, before any model call", async () => {
	const root = tempDir();
	expect(layerBuilt(root)).toBe(false);
	await expect(loadBenchLayer(root)).rejects.toBeInstanceOf(LayerUnavailableError);
	await expect(loadBenchLayer(root)).rejects.toThrow(/A6 needs Jocasta's layer build/);
	// With the module in place, its benchLayer is what a trial gets.
	mkdirSync(join(root, LAYER_MODULE, ".."), { recursive: true });
	writeFileSync(
		join(root, LAYER_MODULE),
		"export const benchLayer = { setUp: async () => ({ args: [], settings: {}, env: {}, close: async () => {} }), preflight: async () => undefined };\n",
	);
	expect(typeof (await loadBenchLayer(root)).setUp).toBe("function");
});

test("A6's layer keeps its data under the OS temp folder, not in a trial root outside the profile, and removes it (EVL-1)", async () => {
	// Beside the checkout: on Windows outside the user's profile (D:\ on his PC and in CI), as the VM's C:\t is.
	const root = tempDir(dirname(REPO));
	const repo = join(root, "repo");
	mkdirSync(repo);
	const ponytail = join(root, "system-prompt.md");
	writeFileSync(ponytail, "# Ponytail, lazy senior dev mode\n");
	const prompt = `${TASK_PROMPT_PREFIX}The retry test is flaky.`;
	const trial: LayerTrial = {
		root,
		repo,
		baseCommit: "0".repeat(40),
		prompt,
		appendSystemPrompt: ponytail,
		switches: { index: false },
		env: {},
	};
	const layer = await benchLayer.setUp(trial);
	const data = layer.env.JOCASTA_DATA_DIR as string;
	try {
		expect(inside(tmpdir(), data)).toBe(true);
		expect(inside(root, data)).toBe(false);
		expect(existsSync(join(data, "jocasta-service.json"))).toBe(true);
		const plugin = layer.args[layer.args.indexOf("-e") + 1] as string;
		// 0577: the check's 30 minutes fit omp's limit; JOCASTA_THREAD lists report_result and check.
		expect(JSON.parse(readFileSync(join(plugin, ".mcp.json"), "utf8")).mcpServers.jocasta).toMatchObject({
			timeout: 1_860_000,
			env: { JOCASTA_DATA_DIR: data, JOCASTA_THREAD: expect.stringMatching(/^[a-z0-9-]+$/) },
		});
		// PRM-5: titled as New thread titles the brief, the checkout's folder as the project.
		const text = readFileSync(layer.args[layer.args.indexOf("--append-system-prompt") + 1] as string, "utf8");
		expect(text).toStartWith("# Ponytail, lazy senior dev mode\n");
		expect(text).toContain(`# Your task: ${briefTitle(prompt)} (repo)`);
		expect(text).not.toContain("benchmark");
		expect(layer.indexBuildMs).toBeUndefined();
	} finally {
		await layer.close();
	}
	expect(existsSync(data)).toBe(false);
	// With the index on, the root never decides whether it runs; codebase-memory-mcp may be missing here.
	const indexed = await benchLayer.setUp({ ...trial, switches: { index: true } }).then(
		async (setup) => {
			await setup.close();
			return "set up";
		},
		(error: Error) => error.message,
	);
	expect(indexed).not.toMatch(/can't keep its data/);
	expect((await benchLayer.preflight({ index: true })) ?? "").not.toMatch(/can't keep its data/);
	expect(await benchLayer.preflight({ index: false })).toBeUndefined();
}, 120_000);

test("A7's key reaches opencode only: every trial's environment drops it and any ANTHROPIC_API_KEY", () => {
	process.env[A7_API_KEY_ENV] = "sk-test";
	process.env.ANTHROPIC_API_KEY = "sk-other";
	try {
		const env = trialEnv({ id: "c", dir: ".", prompt: "", tags: [], checks: [], hiddenChecks: [] }, tempDir());
		expect(env[A7_API_KEY_ENV]).toBeUndefined();
		expect(env.ANTHROPIC_API_KEY).toBeUndefined();
	} finally {
		delete process.env[A7_API_KEY_ENV];
		delete process.env.ANTHROPIC_API_KEY;
	}
});

test("session ids come from opencode's and Claude Code's JSON output", () => {
	const oc = `${JSON.stringify({ type: "step_start", sessionID: "ses_24debf3a8ffeNIh02hZdU8Yx4V" })}\n${JSON.stringify({ type: "text", sessionID: "ses_24debf3a8ffeNIh02hZdU8Yx4V" })}\n`;
	expect(opencodeSessions(oc)).toEqual(["ses_24debf3a8ffeNIh02hZdU8Yx4V"]);
	const cc = `${JSON.stringify({ type: "system", subtype: "init", session_id: "1001133b-1a93-458d-a92a-ce512911db5c" })}\n`;
	expect(claudeSessions(cc)).toEqual(["1001133b-1a93-458d-a92a-ce512911db5c"]);
});

test("saved sessions: omp's folder is copied whole; Claude Code's transcript and subagents are found by session id", () => {
	const omp = join(tempDir(), "sessions");
	mkdirSync(join(omp, "sub"), { recursive: true });
	writeFileSync(join(omp, "main.jsonl"), "{}\n");
	writeFileSync(join(omp, "sub", "agent.jsonl"), "{}\n");
	const saved = join(tempDir(), "saved");
	mkdirSync(saved, { recursive: true });
	writeFileSync(join(saved, "stale.jsonl"), "{}\n");
	saveSessions("omp", saved, { transcript: "", sessionDir: omp, launcher: ["omp"], env: {}, cwd: "." });
	expect(existsSync(join(saved, "main.jsonl")) && existsSync(join(saved, "sub", "agent.jsonl"))).toBe(true);
	expect(existsSync(join(saved, "stale.jsonl"))).toBe(false);

	const claudeHome = tempDir();
	const id = "1001133b-1a93-458d-a92a-ce512911db5c";
	const project = join(claudeHome, "projects", "C--t-repo");
	mkdirSync(join(project, id, "subagents"), { recursive: true });
	writeFileSync(join(project, `${id}.jsonl`), "{}\n");
	writeFileSync(join(project, id, "subagents", "agent-a1.jsonl"), "{}\n");
	const savedClaude = join(tempDir(), "claude");
	saveSessions("claude", savedClaude, {
		transcript: JSON.stringify({ type: "system", subtype: "init", session_id: id }),
		launcher: ["claude"],
		env: { CLAUDE_CONFIG_DIR: claudeHome },
		cwd: ".",
	});
	expect(existsSync(join(savedClaude, `${id}.jsonl`))).toBe(true);
	expect(existsSync(join(savedClaude, id, "subagents", "agent-a1.jsonl"))).toBe(true);
});

test("opencode's usage comes from its exports: steps as requests, reasoning as output, its own cost", () => {
	const dir = tempDir();
	writeFileSync(
		join(dir, "ses_1.json"),
		JSON.stringify({
			info: { id: "ses_1" },
			messages: [
				{ info: { role: "user" }, parts: [] },
				{
					info: { role: "assistant", modelID: "claude-opus-5-5" },
					parts: [
						{ type: "tool", tool: "read", state: { status: "completed" } },
						{
							type: "step-finish",
							tokens: { input: 10, output: 5, reasoning: 3, cache: { read: 100, write: 20 } },
							cost: 0.01,
						},
						{
							type: "step-finish",
							tokens: { input: 1, output: 1, reasoning: 0, cache: { read: 0, write: 0 } },
							cost: 0.002,
						},
					],
				},
			],
		}),
	);
	const response = readOpencodeResponse(dir);
	expect(response).toMatchObject({ model: "claude-opus-5-5", requests: 2, turns: 1, toolCalls: 1 });
	expect(response.usage).toMatchObject({ input: 11, output: 9, cacheRead: 100, cacheWrite: 20, totalTokens: 140 });
	expect(response.usage.cost).toBeCloseTo(0.012, 12);
});

test("0587: the omp arms refuse an omp folder whose MCP files give them a server, or hide a layered arm's own", () => {
	const dir = tempDir();
	const write = (name: string, config: unknown) => writeFileSync(join(dir, name), JSON.stringify(config));
	const loads = (where: string) =>
		`the omp arms would load MCP servers from ${where}. Remove them (jocasta uninstall removes Jocasta's) or use a --profile without them; no trial ran.`;
	expect(mcpProblem(dir, ["a6"])).toBeUndefined();
	write("mcp.json", {
		mcpServers: { jocasta: { command: "jocasta" }, whiteboard: { command: "node", enabled: "false" } },
	});
	write(".mcp.json", { mcpServers: { acme: { command: "acme" } } });
	expect(mcpProblem(dir, [])).toBe(loads(`${join(dir, "mcp.json")}: jocasta; ${join(dir, ".mcp.json")}: acme`));
	// Turned off, a server reaches no arm, yet a "jocasta" entry still hides the layer's own server from A6.
	write("mcp.json", { mcpServers: { jocasta: { command: "jocasta", enabled: false } } });
	writeFileSync(join(dir, ".mcp.json"), "not json");
	expect(mcpProblem(dir, [])).toBeUndefined();
	expect(mcpProblem(dir, ["a6", "f-keep-context"])).toBe(
		`a6, f-keep-context would lose the layer's own MCP server "jocasta": ${join(dir, "mcp.json")} has a "jocasta" entry, which hides the layer's even turned off. Remove it or use a --profile without it; no trial ran.`,
	);
	// enabledServers turns a server back on; disabledServers keeps one off, and hides the layer's of the same name.
	write("mcp.json", { mcpServers: { acme: { command: "acme", enabled: "0" } }, enabledServers: ["acme"] });
	expect(mcpProblem(dir, [])).toBe(loads(`${join(dir, "mcp.json")}: acme`));
	write("mcp.json", { mcpServers: { acme: { command: "acme" } }, disabledServers: ["acme", "jocasta"] });
	expect(mcpProblem(dir, [])).toBeUndefined();
	expect(mcpProblem(dir, ["a6"])).toContain(`${join(dir, "mcp.json")} names "jocasta" in disabledServers`);
});
