/**
 * The harness-OS layer for the benchmark's A6 arm (0543, 0567), built from Jocasta's own parts so a trial gets what a
 * thread gets, without a Jocasta service, herdr or Graphiti:
 *
 * - the task file's brief and Done-when appended to the system prompt (`taskPrompt`, 0540), after the variant's own
 *   prompt file (omp keeps only the last `--append-system-prompt`), titled as New thread titles a brief, with the
 *   checkout's folder as the project (0577);
 * - `jocasta mcp` as the trial's MCP server "jocasta", through an omp extension root (`-e <root>/layer/omp`, whose
 *   `.mcp.json` omp reads), answering from this process: `report_result` (0539), `check` (0542, 0546) and
 *   `graph_search` (0562): its code locations for relationship questions, with no facts (the bench has no Graphiti);
 * - the check runner on the trial's checkout, the project's own mise check as the product decides it;
 * - the code index on the trial's checkout as its main checkout, built during setup, before the trial's clock (0575).
 *
 * `switches.index` off answers `graph_search` with facts only, here none (`f-index-off`); `check` is always on (0571).
 * The ledger is the bench's own, from the trial's saved sessions (`src/shared/ledger.ts`). The layer's data (checks,
 * code index, `jocasta-service.json`) lives in a per-trial folder under the OS temp folder, inside the user's profile (EVL-1:
 * codebase-memory-mcp refuses a folder other users may write to, such as the VM's short `--work` root), and goes on
 * `close()`.
 */
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import type { BenchLayer, LayerSetup, LayerTrial } from "../../../evals/layer.ts";
import type { LayerSwitches } from "../../../evals/variants.ts";
import { removeTree, sameToken } from "../../shared/fsx.ts";
import { writeServiceFile } from "../../shared/service-file.ts";
import { CheckRunner, checksDir, gateReply, shortTree } from "../server/checks.ts";
import { CodeIndex, codeAnswer, relationshipOf } from "../server/code-index.ts";
import { miseCheck, skippedSaid } from "../server/mise.ts";
import { parseReport, REPORT_RESULT, reportLine } from "../server/report.ts";
import { taskPrompt } from "../server/task-file.ts";
import { branchSlug, briefTitle } from "../server/wording.ts";

/** The repo's root: `jocasta mcp` runs from its source here. */
const REPO_ROOT = resolve(import.meta.dir, "..", "..", "..");

/** The project id the layer's parts know every trial by. */
const PROJECT_ID = "bench";

/**
 * omp's time limit for the layer's MCP calls (0577): `check` runs up to 30 minutes, plus one, as setup writes on his
 * global `jocasta` server.
 */
const MCP_TIMEOUT_MS = 1_860_000;

/** A new folder for one trial's layer data, under the OS temp folder (inside the user's profile on Windows). */
const layerDataDir = (): string => mkdtempSync(join(tmpdir(), "jl"));

/** What one trial's layer keeps while it runs. */
interface Trial {
	repo: string;
	/** The checkout's folder name: the project's name in replies, as a thread's project is named. */
	project: string;
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
			const project = await miseCheck(PROJECT_ID, trial.repo, trial.env);
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
					? await trial.index.locations({ id: PROJECT_ID, mainPath: trial.repo }, query)
					: undefined;
			return locations && relationship
				? codeAnswer(trial.project, relationship, locations)
				: `Nothing in ${trial.project}'s graph for that.`;
		}
		default:
			throw new Error(`no tool ${tool}.`);
	}
}

async function setUp(trial: LayerTrial): Promise<LayerSetup> {
	const layer = join(trial.root, "layer");
	const plugin = join(layer, "omp");
	mkdirSync(plugin, { recursive: true });
	const data = layerDataDir();
	let index: CodeIndex | undefined;
	let indexBuildMs: number | undefined;
	let checks: CheckRunner | undefined;
	let server: Bun.Server<undefined> | undefined;
	const close = async () => {
		server?.stop(true);
		await checks?.stop();
		await index?.stop();
		removeTree(data);
	};
	try {
		if (trial.switches.index) {
			index = new CodeIndex({ dataDir: data });
			const refused = await index.unavailable();
			if (refused) throw new Error(`A6's code index can't run for this trial: ${refused}`);
			// 0575: built now, before the trial's clock: in daily use his projects' index is already built.
			const started = performance.now();
			await index.build({ id: PROJECT_ID, mainPath: trial.repo });
			indexBuildMs = Math.round(performance.now() - started);
		}
		checks = new CheckRunner({ dir: checksDir(data) });
		const state: Trial = {
			repo: trial.repo,
			project: basename(trial.repo),
			env: trial.env,
			checks,
			index,
			reports: join(layer, "reports.jsonl"),
		};
		const token = randomBytes(24).toString("hex");
		server = Bun.serve({
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
		// `jocasta mcp` finds this endpoint as it finds the service: `<data>/jocasta-service.json`, its process alive.
		writeServiceFile({ port: server.port as number, token, pid: process.pid }, data);
		const title = briefTitle(trial.prompt);
		// `JOCASTA_THREAD` as a thread pane has it, so `jocasta mcp` lists report_result and check (0577, C6).
		const env = { JOCASTA_DATA_DIR: data, JOCASTA_THREAD: branchSlug(title) || PROJECT_ID };
		writeFileSync(
			join(plugin, ".mcp.json"),
			`${JSON.stringify(
				{
					mcpServers: {
						jocasta: {
							type: "stdio",
							command: process.execPath,
							args: [join(REPO_ROOT, "src", "entry.ts"), "mcp"],
							env,
							timeout: MCP_TIMEOUT_MS,
						},
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
				{ title, project: state.project, brief: trial.prompt, doneWhen: [] },
				trial.appendSystemPrompt ?? null,
			),
		);
		return {
			args: ["--append-system-prompt", prompt, "-e", plugin],
			settings: {},
			env,
			...(indexBuildMs === undefined ? {} : { indexBuildMs }),
			close,
		};
	} catch (error) {
		await close();
		throw error;
	}
}

/** EVL-1: whether a trial's code index could keep its data where the layer puts it and find codebase-memory-mcp. */
async function preflight(switches: LayerSwitches): Promise<string | undefined> {
	if (!switches.index) return undefined;
	const data = layerDataDir();
	try {
		const refused = await new CodeIndex({ dataDir: data }).unavailable();
		return refused === undefined ? undefined : `A6's code index can't run: ${refused}`;
	} finally {
		removeTree(data);
	}
}

export const benchLayer: BenchLayer = { setUp, preflight };
