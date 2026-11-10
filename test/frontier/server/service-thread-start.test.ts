import { expect, test } from "bun:test";
import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { hunkDiffArgs, hunkEnv, reviewPrompt } from "../../../src/frontier/server/hunk.ts";
import { overlayFile, overlayText } from "../../../src/frontier/server/overlay.ts";
import { HANDOFF_PROMPT, shortModel, WHEN_YOU_FINISH } from "../../../src/frontier/server/wording.ts";
import { reviewedFile } from "../../../src/frontier/server/worktrees.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
import { lessonsDir, projectDir } from "../../../src/shared/paths.ts";
import { withoutLeadingComment } from "../../../src/shared/prompt.ts";
import { GIT_TEST_TIMEOUT, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import {
	bossPane,
	call,
	cli,
	commitIn,
	demoProject,
	GUARD,
	HUNK,
	keyed,
	Page,
	proposedPlanFile,
	review,
	sessionFile,
	startThread,
	taskDirOf,
	threadOmp,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"thread_start: the project workspace with its pane, the omp overlay, a worktree workspace on feature/<slug> from trunk running a fresh omp with the thread's env, then the brief with the lessons line (0388, 0399, 0400, 0412, 0414, 0454, 0455, 0457, 0459)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path, reply } = await startThread(server, { late: true });
				expect(reply).toBe(
					"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane.",
				);
				const overlay = overlayFile(project.id);
				expect(Bun.YAML.parse(readFileSync(overlay, "utf8"))).toEqual({
					memory: { backend: "mnemopi" },
					mnemopi: {
						scoping: "global",
						bank: project.id,
						llmMode: "smol",
						dbPath: join(projectDir(project.id), "mnemopi.db"),
					},
					plan: { defaultOnStartup: true },
					compaction: {
						methodOrder: ["handoff", "remote", "soft", "shake", "snapcompact"],
						thresholdTokens: 300000,
						idleEnabled: true,
						idleThresholdTokens: 100000,
					},
					contextFiles: { extra: ["AGENTS.local.md"] },
					skills: { customDirectories: [projectDir(project.id)] },
					// The privacy settings ride in every pane (0478).
					dev: { autoqa: false },
					disabledProviders: ["claude-plugins"],
					modelRoles: { web: "web/duckduckgo" },
					// Every session hosts in Jocasta's relay, and `/share` stays on this machine (0536, 0561).
					collab: { autoStart: "control", relayUrl: `ws://127.0.0.1:${server.relayPort}` },
					share: { serverUrl: `http://127.0.0.1:${server.relayPort}/s` },
				});
				// The project's lessons: one skill `lessons`, its index made from the shipped template (0459).
				expect(readFileSync(join(lessonsDir(project.id), "SKILL.md"), "utf8")).toBe(
					readFileSync(join(shippedConfigDir(), "lessons", "SKILL.md"), "utf8"),
				);
				const command = ["omp", "--continue", ...GUARD, "--config", overlay];
				// The project workspace: label = project name, cwd = main checkout, the project pane running omp.
				expect(server.herdr.of("ensureWorkspace")[1]).toEqual(["demo", project.mainPath, command, keyed(hunkEnv())]);
				const projectWs = server.herdr.workspaceList.find((w) => w.label === "demo")?.id;
				// Thread panes get Jocasta's hunk daemon port, as the review's hunk commands run in them (F-02, F-03), and
				// their own variables (0457); a new thread starts a fresh session, never `--continue` (0455).
				expect(server.herdr.of("createWorktree")).toEqual([
					[
						projectWs,
						{
							repo: project.mainPath,
							branch: "feature/readme-line",
							base: "main",
							path: join(env.root, "demo.wt", "readme-line"),
							title: "Readme line",
							// Every thread warns at $50 (0541) and appends its task's brief and done-when (0540).
							command: [
								"omp",
								"--append-system-prompt",
								join(taskDirOf(project.id), "prompt.md"),
								...GUARD,
								"--config",
								overlayFile(project.id),
								"--warn-cost",
								"50",
							],
							env: keyed({
								...hunkEnv(),
								JOCASTA_THREAD: "readme-line",
								COMPOSE_PROJECT_NAME: "demo-readme-line",
								JOCASTA_PORT_OFFSET: "10",
							}),
						},
					],
				]);
				// Not heavy: its record has no `heavy` and its omp no heavy.yml (0590).
				const record = JSON.parse(readFileSync(join(taskDirOf(project.id), "thread.json"), "utf8"));
				expect(record).not.toHaveProperty("heavy");
				expect(path).toBe(join(env.root, "demo.wt", "readme-line"));
				expect(sh(path, "rev-parse", "--abbrev-ref", "HEAD")).toBe("feature/readme-line");
				// The brief ends with the lessons line (0459); the commit step is in prompt.md's When you finish (0577).
				expect(server.herdr.of("prompt")).toContainEqual([
					paneId,
					"Add a README line.\n\nBefore you plan, read skill://lessons and cite each lesson your plan relies on as (lesson: <file>); a plan that goes against one says why.",
				]);

				const status = await call(server, "thread_status", {});
				expect(status.text).toBe("demo/Readme line [feature/readme-line] idle");
				expect((await call(server, "thread_send", { thread: "feature/readme-line", text: "Go on." })).text).toBe(
					"Sent to demo/Readme line.",
				);
				expect((await call(server, "thread_stop", { thread: "Readme line" })).text).toBe(
					"Stopped demo/Readme line's run.",
				);
				expect(server.herdr.of("stop")).toEqual([[paneId]]);
				expect(await call(server, "thread_send", { thread: "nope", text: "x" })).toEqual({
					text: "no thread nope.",
					isError: true,
				});
				// Boss tools take the boss key (SEC-1, 0484): a thread's pane, no pane, or the boss pane's id without the key
				// or with a wrong one is refused plainly, revealing nothing about the boss pane, whose omp has this run's
				// key; graph_search isn't refused, and searches the thread's own project.
				for (const [from, key] of [
					[paneId, null],
					[null, null],
					[bossPane(server.herdr) ?? null, null],
					[bossPane(server.herdr) ?? null, "wrong-key"],
					[paneId, "wrong-key"],
				] as const) {
					expect(await call(server, "thread_finish", { thread: "Readme line", how: "merge" }, from, key)).toEqual({
						text: "thread_finish is the boss's tool; only the boss pane Jocasta started in workspace Jocasta can use it.",
						isError: true,
					});
				}
				expect(await call(server, "graph_search", { query: "x" }, paneId)).toEqual({
					text: "the knowledge graph isn't available in this run.",
					isError: true,
				});
				// A second thread reuses the project workspace and takes the next free number (0457).
				await startThread(server, { title: "Second", brief: "More." });
				expect(server.herdr.of("ensureWorkspace")).toHaveLength(2);
				expect(server.herdr.of("createWorktree")[1]?.[1]).toMatchObject({
					env: { JOCASTA_THREAD: "second", COMPOSE_PROJECT_NAME: "demo-second", JOCASTA_PORT_OFFSET: "20" },
				});
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a heavy thread_start (0590) is recorded on its task and loads heavy.yml after the overlay, at its start and when started again; a heavy that isn't true or false is refused at both doors",
	async () => {
		const project = await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const args = { project: "demo", title: "Readme line", brief: "Migrate it." };
				expect(await call(server, "thread_start", { ...args, heavy: "yes" })).toEqual({
					text: "thread_start: heavy is true or false.",
					isError: true,
				});
				expect(await cli(server, "thread new", { ...args, heavy: 1 })).toEqual({
					text: "jocasta thread new: heavy is true or false.",
					isError: true,
				});
				expect(herdr.of("createWorktree")).toEqual([]);

				const { path, reply } = await startThread(server, { heavy: true });
				expect(reply).toBe(
					"Started demo/Readme line on branch feature/readme-line. It opens in plan mode and waits for his approval of its plan in its pane. Heavy: it runs on his slow role.",
				);
				const heavy = ["--config", join(shippedConfigDir(), "heavy.yml")];
				expect(JSON.parse(readFileSync(join(taskDirOf(project.id), "thread.json"), "utf8")).heavy).toBe(true);
				expect(herdr.of("createWorktree")[0]?.[1]).toMatchObject({
					command: [
						"omp",
						"--append-system-prompt",
						join(taskDirOf(project.id), "prompt.md"),
						...GUARD,
						"--config",
						overlayFile(project.id),
						...heavy,
						"--warn-cost",
						"50",
					],
				});

				// herdr restores its pane as a shell: omp starts again, still heavy, heavy.yml after the overlay.
				await server.service.refresh();
				herdr.restore();
				const before = herdr.calls.length;
				await server.service.refresh();
				const respawned = herdr.calls
					.slice(before)
					.filter((c) => c.method === "respawn")
					.map((c) => c.args[1] as { cwd: string; command: string[] });
				expect(respawned.find((o) => resolve(o.cwd) === path)?.command).toEqual([...threadOmp(project.id), ...heavy]);
				// The project pane is never heavy.
				expect(respawned.find((o) => resolve(o.cwd) === project.mainPath)?.command).not.toContain(heavy[1]);
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_start puts the worktree beside the project in <project>.wt/<slug>; a taken branch or folder makes both -2, -3… (0445, 0455)",
	async () => {
		const project = await demoProject(env.root);
		const folder = join(env.root, "demo.wt");
		await withServer(
			async (server) => {
				const first = await startThread(server);
				expect(first.path).toBe(join(folder, "readme-line"));
				// The first thread moved off its branch: a second one with the same title never picks up its branch.
				sh(first.path, "checkout", "--detach");
				const again = await startThread(server, { brief: "Again." });
				expect(again.reply).toStartWith("Started demo/Readme line on branch feature/readme-line-2");
				const pane = server.herdr.paneList.find((p) => p.paneId === again.paneId);
				if (!pane) throw new Error("no thread pane");
				expect(server.herdr.of("createWorktree")[1]?.[1]).toMatchObject({
					repo: project.mainPath,
					branch: "feature/readme-line-2",
					path: join(folder, "readme-line-2"),
				});
				expect(pane.cwd).toBe(join(folder, "readme-line-2"));
				expect(sh(join(folder, "readme-line-2"), "rev-parse", "--abbrev-ref", "HEAD")).toBe("feature/readme-line-2");

				// The first one's folder is gone, its branch stays (a PR finish): the next takes -3 for both.
				sh(project.mainPath, "worktree", "remove", "--force", first.path);
				await startThread(server, { brief: "Third." });
				expect(server.herdr.of("createWorktree")[2]?.[1]).toMatchObject({
					branch: "feature/readme-line-3",
					path: join(folder, "readme-line-3"),
				});
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"project panes append the shipped Ponytail prompt; thread panes append it with their task's brief and done-when, in one file (0421, 0427, 0540)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				await startThread(server);
				// Project panes append Ponytail without its license comment (0577): a copy in <data> (PRM-6).
				const ponytail = join(shippedConfigDir(), "ponytail.md");
				const shipped = readFileSync(ponytail, "utf8");
				expect(shipped).toStartWith("<!--");
				const stripped = join(env.data, "ponytail.md");
				expect(readFileSync(stripped, "utf8")).toBe(withoutLeadingComment(shipped).trimStart());
				const command = [
					"omp",
					"--continue",
					"--append-system-prompt",
					stripped,
					...GUARD,
					"--config",
					overlayFile(project.id),
				];
				expect(server.herdr.of("ensureWorkspace")[1]).toEqual(["demo", project.mainPath, command, keyed(hunkEnv())]);
				// omp 18.18 takes one --append-system-prompt (the last), so Ponytail and the task share `prompt.md`.
				const prompt = join(taskDirOf(project.id), "prompt.md");
				expect(server.herdr.of("createWorktree")[0]?.[1]).toMatchObject({
					command: [
						"omp",
						"--append-system-prompt",
						prompt,
						...GUARD,
						"--config",
						overlayFile(project.id),
						"--warn-cost",
						"50",
					],
				});
				// Then the task's sections, ending with how to finish (0577, PRM-7).
				expect(readFileSync(prompt, "utf8")).toBe(
					`${withoutLeadingComment(shipped).trim()}\n\n# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- No commands given; the review's project check decides.\n\n${WHEN_YOU_FINISH.join("\n")}\n`,
				);
			},
			{ agentWaitMs: 10_000, panePrompt: undefined },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a brief left unsubmitted in omp's input gets one Enter; one omp takes at once gets none",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				// The pane stays idle after the brief's paste: one Enter, only after the brief, never twice.
				const { paneId } = await startThread(server);
				expect(server.herdr.of("submit")).toEqual([[paneId]]);
				const order = server.herdr.calls.map((c) => c.method).filter((m) => m === "prompt" || m === "submit");
				expect(order).toEqual(["prompt", "submit"]);

				// A second thread starts working on its brief: no Enter.
				const replied = call(server, "thread_start", { project: "demo", title: "Second", brief: "More." });
				await until(() => server.herdr.of("createWorktree").length === 2);
				const second = server.herdr.paneList.at(-1)?.paneId as string;
				server.herdr.setState(second, "idle");
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === second && String(t).startsWith("More.")));
				server.herdr.setState(second, "working");
				expect((await replied).text).toStartWith("Started demo/Second on branch");
				expect(server.herdr.of("submit")).toEqual([[paneId]]);
			},
			{ agentWaitMs: 10_000, submitWaitMs: 3_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"events: a blocked thread is an urgent [event] line at once, speech and a toast; a turn's end waits for the idle boss and is spoken too (0405, 0407, 0413)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				await page.waitFor((m) => m.type === "snapshot");
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				const file = sessionFile(path, [{ text: "Which format?", cost: 0.5, ask: "CSV or JSON?" }]);

				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "blocked", file);
				await until(() => server.herdr.of("prompt").some(([p]) => p === boss));
				expect(server.herdr.of("prompt")).toContainEqual([
					boss,
					"[event] demo/Readme line — awaiting you: «CSV or JSON?»",
				]);
				await page.waitFor(
					(m) => m.type === "toast" && m.reason === "blocked" && m.paneId === paneId && m.text.includes("CSV or JSON?"),
				);

				// A turn that ends without commits: a normal line, held while the boss works, sent when it is idle.
				server.herdr.setState(boss, "working");
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await until(() => server.service.bossState() === "working");
				expect(server.herdr.of("prompt").filter(([p]) => p === boss)).toHaveLength(1);
				server.herdr.setState(boss, "idle");
				await until(() => server.herdr.of("prompt").filter(([p]) => p === boss).length === 2);
				expect(server.herdr.of("prompt").at(-1)).toEqual([
					boss,
					"[event] demo/Readme line — finished its turn: «Which format?»",
				]);
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a plan waiting in omp's Plan Review (herdr says idle) waits on him like a blocked thread; herdr's raw done isn't done",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				await page.waitFor((m) => m.type === "snapshot");
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				const status = async () => (await call(server, "thread_status", {})).text;

				// herdr's `done` means idle and not yet looked at: not a finished task.
				server.herdr.setState(paneId, "done");
				await until(() => server.service.hud().running === 0);
				expect(await status()).toBe("demo/Readme line [feature/readme-line] idle");

				const file = proposedPlanFile(path);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle", file);
				await until(() => server.herdr.of("prompt").some(([p]) => p === boss));
				expect(server.herdr.of("prompt").filter(([p]) => p === boss)).toEqual([
					[boss, "[event] demo/Readme line — plan ready, awaiting you"],
				]);
				await page.waitFor((m) => m.type === "toast" && m.reason === "blocked" && m.paneId === paneId);
				expect(server.service.hud().blocked).toBe(1);
				expect(await status()).toBe("demo/Readme line [feature/readme-line] plan ready\n  spend $0.25");
				expect((await call(server, "thread_send", { thread: "Readme line", text: "Go." })).isError).toBe(true);
				await page.waitFor(
					(m) => m.type === "today" && m.today.needsYou.some((n) => n.kind === "blocked" && n.paneId === paneId),
				);

				// He approves: omp leaves plan mode and works; nothing waits on him any more.
				appendFileSync(file, `${JSON.stringify({ type: "mode_change", id: "m3", mode: "none" })}\n`);
				server.herdr.setState(paneId, "working", file);
				await until(() => server.service.hud().blocked === 0);
				expect(await status()).toStartWith("demo/Readme line [feature/readme-line] working");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"task end: hunk opens beside the thread, the thread tests and reviews into it, the task is announced once when its review is ready, then /handoff; neither turn retriggers, and a handoff herdr never reports doesn't swallow the next task (0394, 0400, 0406, 0425, UX-21, F-01)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				const range = "main...feature/readme-line";
				const prompts = () => server.herdr.of("prompt");
				server.herdr.setState(boss, "idle");
				server.herdr.setState(paneId, "working");
				writeFileSync(join(path, "README.md"), "demo\nA line.\n");
				sh(path, "commit", "-qam", "Add a README line");
				server.herdr.setState(paneId, "idle");

				// 1. hunk to the right at half width on trunk...branch, watching, named "review", on Jocasta's hunk daemon;
				// 2. the review prompt. Nothing is announced yet: the review follows (UX-21).
				await until(() => prompts().some(([p, text]) => p === paneId && text === review(range)));
				expect(server.herdr.of("splitBeside")).toEqual([
					[
						paneId,
						{
							command: hunkDiffArgs(HUNK, range),
							cwd: path,
							env: hunkEnv(HUNK_OPTIONS.hunkPort),
							label: "review",
							direction: "right",
							ratio: 0.5,
						},
					],
				]);
				expect(hunkDiffArgs(HUNK, range)).toEqual([...HUNK, "diff", "--theme", "tokyo-night", range, "--watch"]);
				expect(hunkEnv(1)).toEqual({ HUNK_MCP_PORT: "1", HUNK_DISABLE_UPDATE_NOTICE: "1" });
				expect(prompts().some(([p]) => p === boss)).toBe(false);
				expect(prompts().some(([, text]) => text === HANDOFF_PROMPT)).toBe(false);
				expect(reviewPrompt(HUNK, range)).toContain("session comment apply --repo . --stdin");
				expect(reviewPrompt(HUNK, range)).toContain("First run the project's tests");
				expect(reviewPrompt(HUNK, range)).toContain("`TESTS: pass — <the test command you ran>`");

				// The review turn ends on its test line: 3. hunk's notes counted, 4. the boss hears it (and it is spoken
				// and toasted, once), 5. /handoff.
				const file = sessionFile(path, [
					{
						text: "Ran bun test, then the review: two notes.\n\n**TESTS: pass — `bun test`**",
						cost: 0.1,
						bash: { command: "bun test", ok: true },
					},
				]);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().some(([, text]) => text === HANDOFF_PROMPT));
				// What the review covered comes first, so the next task's line differs (N-2).
				const reviewReady =
					"[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests pass, no report";
				const order = [
					server.herdr.calls.findIndex((c) => c.method === "splitBeside"),
					server.herdr.calls.findIndex((c) => c.method === "prompt" && c.args[1] === review(range)),
					server.herdr.calls.findIndex((c) => c.method === "prompt" && c.args[1] === reviewReady),
					server.herdr.calls.findIndex((c) => c.method === "prompt" && c.args[1] === HANDOFF_PROMPT),
				];
				expect(order.every((i) => i >= 0)).toBe(true);
				expect([...order].sort((a, b) => a - b)).toEqual(order);
				expect(prompts().filter(([p]) => p === boss)).toEqual([[boss, reviewReady]]);
				await page.waitFor((m) => m.type === "toast" && m.paneId === paneId && m.text.includes("done and reviewed"));
				expect(page.received.filter((m) => m.type === "toast" && m.paneId === paneId)).toHaveLength(1);
				// Reviewed and unfinished, it waits on him in "Awaiting you" (0456).
				await until(() =>
					server.service.day
						.today()
						.needsYou.some(
							(row) => row.kind === "reviewed" && row.text === "Readme line — done and reviewed: 2 notes, tests pass",
						),
				);
				// The reviewed commit is kept for a restart (0394).
				const reviewed = JSON.parse(readFileSync(reviewedFile(), "utf8")) as Record<string, string>;
				expect(Object.values(reviewed)).toEqual([sh(path, "rev-parse", "HEAD")]);
				expect(server.herdr.of("reportPaneMetadata").at(-1)).toEqual([
					paneId,
					expect.objectContaining({ stateLabels: { blocked: "awaiting you", done: "review ready" } }),
				]);

				// The handoff runs and ends: no second review, no second /handoff, no line.
				const before = server.herdr.calls.length;
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await server.service.refresh();
				const after = server.herdr.calls
					.slice(before)
					.filter((c) => c.method === "prompt" || c.method === "splitBeside");
				expect(after).toEqual([]);

				// The next task reviews in the same hunk pane (it watches the range).
				server.herdr.setState(paneId, "working");
				commitIn(path, "demo\nA line.\nAnother.\n", "Another line");
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().filter(([, text]) => text === review(range)).length === 2);
				expect(server.herdr.of("splitBeside")).toHaveLength(1);
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().filter(([, text]) => text === HANDOFF_PROMPT).length === 2);

				// F-01: this handoff lands in the session without herdr ever reporting it as a turn; the next run is a
				// task, and its commit gets its review.
				appendFileSync(
					file,
					`${JSON.stringify({ type: "compaction", id: "h1", method: "handoff", summary: "Handed off.", timestamp: new Date().toISOString() })}\n`,
				);
				server.herdr.setState(paneId, "working");
				// The run counts as the next task once its HEAD is noted (async git): only then may it commit.
				await until(async () =>
					(await call(server, "thread_status", {})).text.startsWith("demo/Readme line [feature/readme-line] working"),
				);
				commitIn(path, "demo\nA line.\nAnother.\nA third.\n", "A third line");
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().filter(([, text]) => text === review(range)).length === 3);
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"task end with no trunk to diff against: no review pane, /handoff at once (0400)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				// No trunk to diff against: no origin/HEAD, main, master or trunk, and the main checkout on a detached HEAD
				// (from the start: a project's trunk is kept with its places, 10 minutes).
				const main = join(env.root, "demo");
				sh(main, "branch", "-m", "main", "work");
				sh(main, "checkout", "-q", "--detach");
				const { paneId, path } = await startThread(server);
				server.herdr.setState(paneId, "working");
				writeFileSync(join(path, "README.md"), "demo\nA line.\n");
				sh(path, "commit", "-qam", "Add a README line");
				server.herdr.setState(paneId, "idle");
				await until(() => server.herdr.of("prompt").some(([, text]) => text === HANDOFF_PROMPT));
				expect(server.herdr.of("splitBeside")).toEqual([]);
				// A finished task without a review count reads "done" once, not "done — done".
				expect((await call(server, "thread_status", {})).text).toBe("demo/Readme line [feature/readme-line] done");
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"sidebar tokens per thread pane and project workspace, <data>/hud.txt, and both cleared when the service stops (0402, 0403, UX-04, UX-13)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		const hud = join(env.data, "hud.txt");
		let paneId = "";
		// Every omp session of the day counts (the HUD matches omp's own stats), not only the threads'. The sessions
		// folder is listed every 30 s (PERF-3): this one is there from the start.
		sessionFile(env.root, [{ text: "elsewhere", cost: 9 }]);
		await withServer(
			async (server) => {
				const started = await startThread(server);
				paneId = started.paneId;
				const file = sessionFile(started.path, [
					{ text: "Planning.", cost: 1.25 },
					{ text: "Done planning.", cost: 0.5 },
				]);
				server.herdr.setState(paneId, "working", file);
				await server.service.refresh();
				await until(() => JSON.stringify(server.herdr.of("reportPaneMetadata")).includes("$1.75"));
				expect(server.herdr.of("reportPaneMetadata")).toContainEqual([
					paneId,
					{
						tokens: { model: "opus-5-5", cost: "$1.75" },
						// "done" is labelled "review ready" only once a review's notes are counted (0394).
						stateLabels: { blocked: "awaiting you" },
						ttlMs: expect.any(Number),
					},
				]);
				const projectWs = server.herdr.workspaceList.find((w) => w.label === "demo")?.id;
				expect(server.herdr.of("reportWorkspaceMetadata")).toContainEqual([projectWs, { running: "1 at work" }]);
				// The HUD says NEEDS YOU's rows and the spend; the quota is in `today` (0554, 0581).
				await until(() => existsSync(hud) && readFileSync(hud, "utf8") === "0 awaiting you · $10.75 today\n");

				// A thread awaiting him counts once.
				server.herdr.setState(paneId, "blocked");
				await until(() => readFileSync(hud, "utf8") === "1 awaiting you · $10.75 today\n");
				expect(server.herdr.of("reportWorkspaceMetadata")).toContainEqual([projectWs, { running: "1 awaiting you" }]);
			},
			{ agentWaitMs: 10_000, herdr },
		);
		// Stopped: the HUD says so, and the tokens are cleared (UX-13).
		expect(readFileSync(hud, "utf8")).toBe("Jocasta isn't running; start it with jocasta.\n");
		expect(herdr.of("reportPaneMetadata").at(-1)).toEqual([paneId, { tokens: { model: null, cost: null } }]);
		expect(herdr.of("reportWorkspaceMetadata").at(-1)?.[1]).toEqual({ running: null });
		expect(shortModel("anthropic/claude-opus-5-5")).toBe("opus-5-5");
		expect(shortModel("openai/gpt-5.5")).toBe("gpt-5.5");
	},
	GIT_TEST_TIMEOUT,
);

test("the compaction and privacy settings agree wherever they are written: the project overlay, the boss's boss.yml and A1's arm (0399, 0448, 0478, SVC-16)", () => {
	const overlay = Bun.YAML.parse(overlayText("p-0123456789abcdef")) as Record<string, unknown>;
	const boss = Bun.YAML.parse(readFileSync(join(shippedConfigDir(), "boss.yml"), "utf8")) as Record<string, unknown>;
	const a1 = (
		Bun.YAML.parse(readFileSync(join(import.meta.dir, "../../../evals/variants/a1.yaml"), "utf8")) as {
			settings: Record<string, unknown>;
		}
	).settings;
	const shared = (config: Record<string, unknown>) => {
		const roles = config.modelRoles;
		return {
			compaction: config.compaction,
			dev: config.dev,
			disabledProviders: config.disabledProviders,
			web: roles && typeof roles === "object" && "web" in roles ? roles.web : undefined,
		};
	};
	expect(shared(boss)).toEqual(shared(overlay));
	expect(shared(a1)).toEqual(shared(overlay));
});
