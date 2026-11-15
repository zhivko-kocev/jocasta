/**
 * The harness-OS layer for the benchmark's A6 arm (0543, 0567), built from Jocasta's own parts so a trial gets what a
 * thread gets, without a Jocasta service or herdr:
 *
 * - the task file's brief and Done-when appended to the system prompt (`taskPrompt`, 0540), after the variant's own
 *   prompt file (omp keeps only the last `--append-system-prompt`), titled as New thread titles a brief, with the
 *   checkout's folder as the project (0577);
 * - `jocasta mcp` as the trial's MCP server "jocasta", through an omp extension root (`-e <root>/layer/omp`, whose
 *   `.mcp.json` omp reads), answering from this process: `report_result` (0539), `check` (0542, 0546, 0571) and
 *   `history_search` (0604), which has no past sessions to search in a trial and answers as for an empty history;
 * - the check runner on the trial's checkout, the project's own mise check as the product decides it.
 *
 * The ledger is the bench's own, from the trial's saved sessions (`src/shared/ledger.ts`). The layer's data (checks,
 * `jocasta-service.json`) lives in a per-trial folder under the OS temp folder and goes on `close()`.
 */
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import type { BenchLayer, LayerSetup, LayerTrial } from "../../../evals/layer.ts";
import { removeTree, sameToken } from "../../shared/fsx.ts";
import { writeServiceFile } from "../../shared/service-file.ts";
import { CheckRunner, checksDir, gateReply, shortTree } from "../server/checks.ts";
import { HISTORY_SEARCH, historyAnswer } from "../server/history.ts";
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
		case HISTORY_SEARCH:
			// A trial has no past sessions: the tool answers as it does for a project with nothing in its history (0604).
			return historyAnswer(trial.project, []);
		default:
			throw new Error(`no tool ${tool}.`);
	}
}

async function setUp(trial: LayerTrial): Promise<LayerSetup> {
	const layer = join(trial.root, "layer");
	const plugin = join(layer, "omp");
	mkdirSync(plugin, { recursive: true });
	const data = layerDataDir();
	let checks: CheckRunner | undefined;
	let server: Bun.Server<undefined> | undefined;
	const close = async () => {
		server?.stop(true);
		await checks?.stop();
		removeTree(data);
	};
	try {
		checks = new CheckRunner({ dir: checksDir(data) });
		const state: Trial = {
			repo: trial.repo,
			project: basename(trial.repo),
			env: trial.env,
			checks,
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
			close,
		};
	} catch (error) {
		await close();
		throw error;
	}
}

export const benchLayer: BenchLayer = { setUp };
