// The project's scorecard in the weekly review (0616): its mise task `scorecard` runs before the drift agent, the
// output is kept per week, the agent's prompt carries this week's and last week's, and the report names what changed.
import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { SCORECARD_MAX_CHARS } from "../../../src/frontier/server/weekly-review.ts";
import { projectDir } from "../../../src/shared/paths.ts";
import type { Project } from "../../../src/shared/projects.ts";
import { GIT_TEST_TIMEOUT, useTempJocasta } from "../../shared/fixture.ts";
import { commitIn, demoProject, withServer } from "./helpers.ts";

const env = useTempJocasta();

const OMP = [process.execPath, join(import.meta.dir, "fake-omp.ts")];
const MISE = [process.execPath, join(import.meta.dir, "fake-mise.ts")];
const TASKS = JSON.stringify([{ name: "scorecard" }]);
const SET: string[] = [];

/** Sets the fake mise's answers (a trusted folder with the `scorecard` task) with `extra` over them, until the test ends. */
function mise(extra: Record<string, string> = {}): void {
	const answers = {
		FAKE_MISE_PARANOID: '["config"]\n',
		FAKE_MISE_TRUST: "",
		FAKE_MISE_ENV_JSON: "{}",
		FAKE_MISE_TASKS: TASKS,
		...extra,
	};
	for (const [key, value] of Object.entries(answers)) {
		process.env[key] = value;
		SET.push(key);
	}
}
afterEach(() => {
	for (const key of SET.splice(0)) delete process.env[key];
});

/** A demo project with two commits this week, so it has a week to review. */
async function weekOfWork(): Promise<Project> {
	const project = await demoProject(env.root);
	commitIn(project.mainPath, "demo\nA second change.\n", "Second change");
	return project;
}

/** One review of `project` through the service; its result line and the prompt the agent was given. */
async function review(
	project: Project,
	options: { scorecardMs?: number } = {},
): Promise<{ line: string; prompt: string }> {
	let line = "";
	await withServer(
		async (server) => {
			line = (await server.service.weeklyReview(project)) ?? "";
		},
		{ omp: OMP, mise: MISE, ...options },
	);
	const argv = JSON.parse(readFileSync(join(projectDir(project.id), "weekly", "argv.json"), "utf8")) as string[];
	return { line, prompt: argv.at(-1) as string };
}

const kept = (project: Project) => join(projectDir(project.id), "scorecard");

test(
	"a project's scorecard task runs before the agent, its output is kept for the week, and the agent's prompt carries it; with an earlier week both, and the report names the lines that changed (0616)",
	async () => {
		const project = await weekOfWork();
		mise({ FAKE_MISE_SCORECARD: "functions over 80 lines: 13\nduplicated: 0.18%\n" });
		const first = await review(project);
		expect(first.prompt).toContain("functions over 80 lines: 13 | duplicated: 0.18%");
		expect(first.prompt).toContain("measurements to read as questions, not targets");
		expect(first.prompt).toContain("There is no earlier week to compare.");
		expect(first.line).toEndWith("no drift found · scorecard: no earlier week to compare");
		const [file] = readdirSync(kept(project));
		expect(readFileSync(join(kept(project), file as string), "utf8")).toBe(
			"functions over 80 lines: 13\nduplicated: 0.18%",
		);

		writeFileSync(join(kept(project), "2000-W01.txt"), "functions over 80 lines: 15\nduplicated: 0.18%\nold: 1");
		mise({ FAKE_MISE_SCORECARD: "functions over 80 lines: 13\nduplicated: 0.20%\nnew: 2\n" });
		const second = await review(project);
		expect(second.prompt).toContain("This week: functions over 80 lines: 13 | duplicated: 0.20% | new: 2");
		expect(second.prompt).toContain("Last week (2000-W01): functions over 80 lines: 15 | duplicated: 0.18% | old: 1");
		expect(second.line).toEndWith(
			"no drift found · scorecard: changed since last week: functions over 80 lines: 15 → 13; duplicated: 0.18% → 0.20%; new: 2",
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a project without the task: no run, no line, the review as it is (0616)",
	async () => {
		const project = await weekOfWork();
		const log = join(env.root, "mise.log");
		mise({ FAKE_MISE_TASKS: "[]", FAKE_MISE_LOG: log });
		const { line, prompt } = await review(project);
		expect(line).toEndWith("(2 commits): no drift found");
		expect(prompt).not.toContain("scorecard");
		expect(readFileSync(log, "utf8")).not.toContain("run scorecard");
		expect(existsSync(kept(project))).toBe(false);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a scorecard task that fails, runs past its limit or sits in an untrusted config: the review still runs and its report says why there is no scorecard (0616)",
	async () => {
		const project = await weekOfWork();
		mise({ FAKE_MISE_SCORECARD: "partial: 1\n", FAKE_MISE_SCORECARD_EXIT: "2" });
		const failed = await review(project);
		expect(failed.line).toEndWith("no drift found · no scorecard: the scorecard task exited 2: scorecard: boom");
		expect(failed.prompt).not.toContain("partial");

		mise({ FAKE_MISE_SCORECARD: "late: 1\n", FAKE_MISE_SCORECARD_SLEEP_MS: "10000" });
		const slow = await review(project, { scorecardMs: 1000 });
		expect(slow.line).toEndWith("no drift found · no scorecard: the scorecard task ran past its 1 s limit");
		expect(slow.prompt).not.toContain("late");

		mise({ FAKE_MISE_TRUST: `${project.mainPath}\\mise.toml: untrusted\n`, FAKE_MISE_SCORECARD: "never: 1\n" });
		const untrusted = await review(project);
		expect(untrusted.line).toEndWith(
			`no drift found · no scorecard: ${project.mainPath}\\mise.toml isn't trusted in mise`,
		);
		expect(existsSync(kept(project))).toBe(false);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"an oversized scorecard output is cut to the cap, in what is kept and in the prompt (0616)",
	async () => {
		const project = await weekOfWork();
		mkdirSync(projectDir(project.id), { recursive: true });
		mise({ FAKE_MISE_SCORECARD: `${"x".repeat(SCORECARD_MAX_CHARS * 3)}\n` });
		const { prompt } = await review(project);
		const [file] = readdirSync(kept(project));
		const stored = readFileSync(join(kept(project), file as string), "utf8");
		expect(stored).toBe(`${"x".repeat(SCORECARD_MAX_CHARS)}\n[cut at ${SCORECARD_MAX_CHARS} characters]`);
		expect(prompt).toContain(stored.split("\n")[0] as string);
		expect(prompt.length).toBeLessThan(SCORECARD_MAX_CHARS + 3000);
	},
	GIT_TEST_TIMEOUT,
);
