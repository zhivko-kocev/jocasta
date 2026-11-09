/**
 * The harness-OS layer for the benchmark's A6 arm (0543, 0567), built from Jocasta's own parts so a trial gets what a
 * thread gets, without a Jocasta service, herdr or Graphiti:
 *
 * - the task file's brief and Done-when appended to the system prompt (`taskPrompt`, 0540), after the variant's own
 *   prompt file (omp keeps only the last `--append-system-prompt`);
 * - `jocasta mcp` as the trial's MCP server "jocasta", through an omp extension root (`-e <layer>/omp`, whose
 *   `.mcp.json` omp reads), answering from this process: `report_result` (0539), `check` (0542, 0546) and
 *   `graph_search` (0562): its code locations for relationship questions, with no facts (the bench has no Graphiti);
 * - the check runner on the trial's checkout (`<layer>/data/checks`), the project's own mise check as the product
 *   decides it;
 * - the code index on the trial's checkout as its main checkout (`<layer>/data/code-index`).
 *
 * `switches.index` off answers `graph_search` with facts only, here none (`f-index-off`); `check` is always on (0571).
 * The ledger is the bench's
 * own, from the trial's saved sessions (`src/shared/ledger.ts`). Everything lives under the trial's root and goes with it.
 */
import { randomBytes } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { BenchLayer, LayerSetup, LayerTrial } from "../../../evals/layer.ts";
import { sameToken, writeAtomically } from "../../shared/fsx.ts";
import { CheckRunner, checksDir, gateReply, shortTree } from "../server/checks.ts";
import { CodeIndex, codeAnswer, relationshipOf } from "../server/code-index.ts";
import { miseCheck, skippedSaid } from "../server/mise.ts";
import { parseReport, REPORT_RESULT, reportLine } from "../server/report.ts";
import { taskPrompt } from "../server/task-file.ts";

/** The repo's root: `jocasta mcp` runs from its source here. */
const REPO_ROOT = resolve(import.meta.dir, "..", "..", "..");

/** The project id and name the layer's parts know the trial by. */
const PROJECT = { id: "bench", name: "this repo" };

/** What one trial's layer keeps while it runs. */
interface Trial {
	repo: string;
	env: Record<string, string | undefined>;
	checks: CheckRunner;
	index: CodeIndex | undefined;
	reports: string;
}

async function answer(trial: Trial, tool: string, args: Record<string, unknown>): Promise<string> {
	switch (tool) {
		case REPORT_RESULT: {
			const parsed = parseReport(args);
			if ("problem" in parsed) throw new Error(parsed.problem);
			writeFileSync(trial.reports, `${JSON.stringify({ at: Date.now(), value: parsed.report })}\n`, { flag: "a" });
			return `Report stored: ${reportLine(parsed.report)}`;
		}
		case "check": {
			const project = await miseCheck(PROJECT.id, trial.repo, trial.env);
			if (!project || "skipped" in project) {
				return project
					? `No checks: ${skippedSaid(project.skipped)}, and the task has no done_when.`
					: "No checks: the project has no trusted mise verify or test task, and the task has no done_when.";
			}
			const { task } = project;
			const run = await trial.checks.run(trial.repo, [{ name: task, command: `mise run -c ${task}`, mise: true }], {
				...trial.env,
			});
			if (!run) throw new Error("check: the checks couldn't run here.");
			return gateReply(run) || `checks: PASS (tree ${shortTree(run.tree)})`;
		}
		case "graph_search": {
			const query = typeof args.query === "string" ? args.query : "";
			if (!query.trim()) throw new Error("graph_search: query is required.");
			const relationship = relationshipOf(query);
			const locations =
				trial.index && relationship
					? await trial.index.locations({ id: PROJECT.id, mainPath: trial.repo }, query)
					: undefined;
			return locations && relationship
				? codeAnswer(PROJECT.name, relationship, locations)
				: `Nothing in ${PROJECT.name}'s graph for that.`;
		}
		default:
			throw new Error(`no tool ${tool}.`);
	}
}

async function setUp(trial: LayerTrial): Promise<LayerSetup> {
	const layer = join(trial.root, "layer");
	const data = join(layer, "data");
	const plugin = join(layer, "omp");
	mkdirSync(data, { recursive: true });
	mkdirSync(plugin, { recursive: true });
	const index = trial.switches.index ? new CodeIndex({ dataDir: data }) : undefined;
	const refused = index?.unavailable();
	if (refused) throw new Error(`A6's code index can't run for this trial: ${refused}`);
	const state: Trial = {
		repo: trial.repo,
		env: trial.env,
		checks: new CheckRunner({ dir: checksDir(data) }),
		index,
		reports: join(layer, "reports.jsonl"),
	};
	const token = randomBytes(24).toString("hex");
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		idleTimeout: 0,
		async fetch(request) {
			const url = new URL(request.url);
			const bearer = request.headers.get("authorization")?.replace(/^Bearer /, "");
			if (!sameToken(bearer, token)) return Response.json({ text: "invalid token", isError: true }, { status: 401 });
			if (request.method !== "POST" || !url.pathname.startsWith("/api/mcp/")) {
				return Response.json({ text: "not found", isError: true }, { status: 404 });
			}
			const tool = decodeURIComponent(url.pathname.slice("/api/mcp/".length));
			try {
				const body = (await request.json()) as { args?: unknown };
				const args =
					body.args !== null && typeof body.args === "object" && !Array.isArray(body.args)
						? (body.args as Record<string, unknown>)
						: {};
				return Response.json({ text: await answer(state, tool, args) });
			} catch (error) {
				return Response.json({ text: (error as Error).message, isError: true });
			}
		},
	});
	// `jocasta mcp` finds this endpoint as it finds the service: `<data>/frontier.json`, its process alive.
	writeAtomically(join(data, "frontier.json"), `${JSON.stringify({ port: server.port, token, pid: process.pid })}\n`);
	const env = { JOCASTA_DATA_DIR: data };
	writeFileSync(
		join(plugin, ".mcp.json"),
		`${JSON.stringify(
			{
				mcpServers: {
					jocasta: { type: "stdio", command: process.execPath, args: [join(REPO_ROOT, "src", "entry.ts"), "mcp"], env },
				},
			},
			null,
			2,
		)}\n`,
	);
	const prompt = join(layer, "prompt.md");
	writeFileSync(
		prompt,
		taskPrompt(
			{ title: "the benchmark task", project: PROJECT.name, brief: trial.prompt, doneWhen: [] },
			trial.appendSystemPrompt ?? null,
		),
	);
	return {
		args: ["--append-system-prompt", prompt, "-e", plugin],
		settings: {},
		env,
		async close() {
			server.stop(true);
			await state.checks.stop();
			await index?.stop();
		},
	};
}

export const benchLayer: BenchLayer = { setUp };
