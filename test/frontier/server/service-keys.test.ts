import { expect, test } from "bun:test";
import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { PLUGIN_ACTION_PATH, POPUP_PATH } from "../../../src/frontier/protocol.ts";
import type { Confirmation } from "../../../src/frontier/server/allow.ts";
import { overlayFile, overlayText } from "../../../src/frontier/server/overlay.ts";
import type { Popup } from "../../../src/frontier/server/popups.ts";
import {
	HANDOFF_PROMPT,
	IDLE_CHECK_FRAME,
	REPORT_STEER,
	WHEN_YOU_FINISH,
} from "../../../src/frontier/server/wording.ts";
import { projectDir } from "../../../src/shared/paths.ts";
import { GIT_TEST_TIMEOUT, sh, useTempJocasta } from "../../shared/fixture.ts";
import { fakeHost } from "./fake-hosts.ts";
import {
	BRIEF,
	bossPane,
	call,
	cli,
	commitIn,
	demoProject,
	GUARD,
	Page,
	proposedPlanFile,
	sessionFile,
	startThread,
	type TestServer,
	taskDirOf,
	threadOmp,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"his keys through /api/plugin/action: Stop refuses an idle thread with why and stops a working one; Finish and Review refuse while it works; Quit's Stop everything and Finish's Merge take the popup's code, Cancel doesn't (0544, 0554, 0568)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		const shown: Popup[] = [];
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const page = await Page.open(server);
				// A popup's answer goes with the reader secret its one view handed out (0578): viewed once per question.
				const readers = new Map<string, string>();
				const post = async (route: string, body: unknown, token: string | null = server.token) => {
					const headers: Record<string, string> = token ? { authorization: `Bearer ${token}` } : {};
					if (route.startsWith(POPUP_PATH)) {
						if (!readers.has(route)) {
							const view = await fetch(`http://127.0.0.1:${server.port}${route}`, { headers });
							readers.set(route, view.headers.get("x-jocasta-reader") ?? "");
						}
						headers["x-jocasta-reader"] = readers.get(route) as string;
					}
					return fetch(`http://127.0.0.1:${server.port}${route}`, {
						method: "POST",
						headers,
						body: JSON.stringify(body),
					});
				};
				const key = async (action: string, pane?: string) => {
					expect((await post(PLUGIN_ACTION_PATH, { action, ...(pane ? { paneId: pane } : {}) })).status).toBe(202);
				};
				const said = async (text: string | RegExp) =>
					(await page.waitFor(
						(m) => m.type === "action" && (typeof text === "string" ? m.text === text : text.test(m.text)),
					)) as { text: string; isError: boolean };
				const popup = async (title: string) => {
					await until(() => shown.some((p) => p.view.title === title && p.open));
					return shown.find((p) => p.view.title === title && p.open) as Popup;
				};

				await key("stop", paneId);
				expect((await said("Stop: demo/Readme line isn't working; nothing to stop.")).isError).toBe(true);
				expect(server.herdr.of("notify")).toContainEqual([
					{ title: "Jocasta", body: "Stop: demo/Readme line isn't working; nothing to stop.", sound: "none" },
				]);
				// Logged as the toasts' notifications are (0552).
				await until(() =>
					readFileSync(join(env.data, "notifications.jsonl"), "utf8").includes(
						'"body":"Stop: demo/Readme line isn\'t working; nothing to stop."',
					),
				);
				server.herdr.setState(paneId, "working");
				await until(() => server.service.busyThreads().working === 1);
				await key("stop", paneId);
				expect((await said("Stopped demo/Readme line's run.")).isError).toBe(false);
				expect(server.herdr.of("stop")).toEqual([[paneId]]);
				await key("finish", paneId);
				await said("demo/Readme line isn't idle (working); finish it once it is.");
				await key("review", paneId);
				await said("Review: demo/Readme line isn't idle (working); review it once it is.");
				await key("stop", bossPane(server.herdr));
				await said("Stop: the focused pane isn't a thread's; focus a thread or pick one in today.");

				// Quit with a thread working: the close question; Stop everything without its code is refused.
				await key("quit");
				const close = await popup("Jocasta: Stop everything?");
				expect(close.view.code).toMatch(/^[A-HJ-NP-Z]{3}$/);
				expect((await post(`${POPUP_PATH}${close.code}`, { choice: "Stop everything" })).status).toBe(409);
				expect((await post(`${POPUP_PATH}${close.code}`, { choice: "Cancel" })).status).toBe(204);
				await said("Jocasta keeps running.");

				// Finish once idle with a commit: Merge without its code is refused, with it the thread merges.
				server.herdr.setState(paneId, "idle");
				await until(() => server.service.busyThreads().working === 0);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				await key("finish", paneId);
				const finish = await popup("Jocasta: Finish demo/Readme line");
				expect(finish.view.choices).toEqual(["Merge", "PR", "Cancel"]);
				expect((await post(`${POPUP_PATH}${finish.code}`, { choice: "Merge", typed: "" })).status).toBe(409);
				const typed = finish.view.code?.toLowerCase();
				expect((await post(`${POPUP_PATH}${finish.code}`, { choice: "Merge", typed })).status).toBe(204);
				const merged = await said(/^Merged demo\/Readme line into main/);
				expect(merged.isError).toBe(false);
				expect(existsSync(path)).toBe(false);

				expect((await post(PLUGIN_ACTION_PATH, { action: "merge" })).status).toBe(400);
				expect((await post(PLUGIN_ACTION_PATH, {}, null)).status).toBe(401);
				page.close();
			},
			{ agentWaitMs: 10_000, onQuestion: (p) => void shown.push(p) },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread's working, asking and idle come from its omp host's pipe, matched by its omp's pid and worktree; herdr's state counts only while the host doesn't answer (0547, 0561)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				// The pane's omp: a real process, so its pid and start can be read (0490).
				const omp = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
					stdio: ["ignore", "ignore", "ignore"],
					windowsHide: true,
				});
				const host = await fakeHost(join(env.data, "collab-hosts"), {
					pid: omp.pid,
					cwd: path,
					busy: true,
					inputRequired: false,
				});
				// His own omp in the same worktree, not the pane's: never asked.
				const his = await fakeHost(join(env.data, "collab-hosts"), {
					pid: process.pid,
					cwd: path,
					busy: false,
					inputRequired: true,
				});
				try {
					server.herdr.roots.set(paneId, omp.pid);
					await server.service.refresh();
					// herdr says idle; omp says a turn runs.
					await until(() => server.service.busyThreads().working === 1);
					// herdr's idle pushed meanwhile doesn't end it while the host answers.
					server.herdr.setState(paneId, "idle");
					const asked = host.asked;
					await until(() => host.asked >= asked + 2);
					expect(server.service.busyThreads()).toEqual({ working: 1, awaiting: 0 });
					// omp's `ask` dialog: awaiting him.
					host.state = { ...host.state, inputRequired: true };
					await until(() => server.service.busyThreads().awaiting === 1);
					// The turn ends.
					host.state = { ...host.state, busy: false, inputRequired: false };
					await until(() => server.service.busyThreads().awaiting === 0 && server.service.busyThreads().working === 0);
					// No entry any more: unknown, and herdr's events decide again.
					await host.close();
					// Once a poll finds no entry, herdr's working (pushed again until it counts) is the thread's state.
					await until(() => {
						server.herdr.setState(paneId, "working");
						return server.service.busyThreads().working === 1;
					});
					expect(his.asked).toBe(0);
				} finally {
					await Promise.allSettled([host.close(), his.close()]);
					omp.kill();
					await omp.exited;
				}
			},
			{ agentWaitMs: 10_000, hostPollMs: 50 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a pane whose collab.relayUrl doesn't resolve to loopback isn't started: thread_start refuses before anything is made (0536, 0561)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const reply = await call(server, "thread_start", {
					project: "demo",
					title: "Readme line",
					brief: "Add a README line.",
				});
				expect(reply.isError).toBe(true);
				expect(reply.text).toContain(`omp wasn't started in ${project.mainPath}`);
				expect(reply.text).toContain('"wss://my.omp.sh"');
				expect(reply.text).toContain(overlayFile(project.id));
				expect(server.herdr.of("createWorktree")).toEqual([]);
				expect(server.herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta"]);
			},
			{
				// The boss's overlay resolves to the relay; the project's says the public one.
				relayUrlOf: async (command) =>
					command.includes(overlayFile(project.id)) ? "wss://my.omp.sh" : "ws://127.0.0.1:47499",
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

/**
 * Holds `file` open for reading as omp does while it starts, without letting it be replaced (Windows' FileShare.Read).
 * The release ends the holder: its handle closes with the process at once, within the product's one-second retry
 * however loaded the machine is, where a PowerShell left to read a line and close the file itself can take longer.
 */
async function holdOpen(file: string): Promise<{ release(): Promise<void> }> {
	const hold = Bun.spawn(
		[
			"powershell",
			"-NoProfile",
			"-Command",
			`$f = [System.IO.File]::Open('${file.replaceAll("'", "''")}', 'Open', 'Read', 'Read'); Write-Output held; Start-Sleep -Seconds 600`,
		],
		{ stdin: "ignore", stdout: "pipe", stderr: "ignore", windowsHide: true },
	);
	const reader = hold.stdout.getReader();
	await reader.read();
	reader.releaseLock();
	return {
		release: async () => {
			hold.kill();
			await hold.exited;
		},
	};
}

test.skipIf(process.platform !== "win32")(
	"parallel thread starts in one project don't race on its overlay: one write at a time, none when unchanged, a refused rename tried again (0534)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const overlay = overlayFile(project.id);
				const start = (title: string) =>
					call(server, "thread_start", { project: "demo", title, brief: "Add a README line." });
				// An overlay from an older build, open in an omp starting: the rename over it is refused until it lets go.
				mkdirSync(projectDir(project.id), { recursive: true });
				writeFileSync(overlay, "old: true\n");
				const first = await holdOpen(overlay);
				const replies = ["One", "Two", "Three", "Four"].map(start);
				// Released once the product's write is waiting on the refused rename, so the retry is what lets it through.
				await until(() => readdirSync(projectDir(project.id)).some((f) => f.endsWith(".tmp")));
				await first.release();
				for (const reply of await Promise.all(replies)) expect(reply.text).toStartWith("Started ");
				expect(readFileSync(overlay, "utf8")).toBe(overlayText(project.id, server.relayPort));
				// The overlay as it should be, held open by every omp reading it: never replaced, every start goes on.
				const second = await holdOpen(overlay);
				try {
					const more = await Promise.all(["Five", "Six", "Seven", "Eight"].map(start));
					for (const reply of more) expect(reply.text).toStartWith("Started ");
				} finally {
					await second.release();
				}
				expect(server.herdr.of("createWorktree")).toHaveLength(8);
				expect(readdirSync(projectDir(project.id)).filter((f) => f.endsWith(".tmp"))).toEqual([]);
			},
			{ agentWaitMs: 0 },
		);
	},
	GIT_TEST_TIMEOUT * 2,
);

/** Starts `Readme line` with `done_when` through the boss's tool; its omp comes up idle (0540). */
async function startWithChecks(server: TestServer, doneWhen: string[]): Promise<{ paneId: string; path: string }> {
	const replied = call(server, "thread_start", {
		project: "demo",
		title: "Readme line",
		brief: BRIEF,
		done_when: doneWhen,
	});
	await until(() => server.herdr.of("createWorktree").length === 1);
	const pane = server.herdr.paneList.at(-1);
	if (!pane?.cwd) throw new Error("no thread pane");
	server.herdr.setState(pane.paneId, "idle");
	await replied;
	return { paneId: pane.paneId, path: pane.cwd };
}

test(
	"thread_start's brief and done_when go into the task file and every launch of the thread's omp; the approved plan, session, state and spend follow (0540)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startWithChecks(server, ["bun test test/readme.test.ts"]);
				const dir = taskDirOf(project.id);
				expect(basename(dir)).toMatch(/^t-[0-9a-f]{4}$/);
				const prompt = join(dir, "prompt.md");
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
				// Then how to finish (0577, PRM-7).
				expect(readFileSync(prompt, "utf8")).toBe(
					`# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- \`bun test test/readme.test.ts\` passes\n\n${WHEN_YOU_FINISH.join("\n")}\n`,
				);
				const task = () => readFileSync(join(dir, "task.md"), "utf8");
				const base = sh(project.mainPath, "rev-parse", "--short", "main");
				await until(() => task().includes("- State: idle"));
				expect(task()).toBe(
					[
						"# Readme line (demo)",
						"",
						`- Thread: ${basename(dir)} · session — · parent: — · base ${base}`,
						"- State: idle · spend —",
						"",
						"## Brief",
						BRIEF,
						"",
						"## Done when",
						"- `bun test test/readme.test.ts` passes",
						"",
						"## Plan",
						"Not approved yet.",
						"",
						"## Report",
						"None yet.",
						"",
						"## Reviews",
						"None yet.",
						"",
					].join("\n"),
				);
				expect((await call(server, "thread_status", { thread: "Readme line" })).text).toEndWith(
					`\ntask file: ${join(dir, "task.md")}\nreport: none yet`,
				);

				// The plan, proposed in omp's local:// folder and approved: the task file carries it.
				const file = proposedPlanFile(path);
				const local = join(file.replace(/\.jsonl$/, ""), "local");
				mkdirSync(local, { recursive: true });
				writeFileSync(join(local, "readme-line-plan.md"), "1. Add the line.\n");
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle", file);
				await until(() => task().includes("- State: plan ready"));
				appendFileSync(file, `${JSON.stringify({ type: "mode_change", id: "m3", mode: "none" })}\n`);
				server.herdr.setState(paneId, "working", file);
				await until(() => task().includes("## Plan (approved "));
				expect(task()).toMatch(/## Plan \(approved \d\d:\d\d\)\n1\. Add the line\.\n/);
				expect(task()).toContain(
					`- Thread: ${basename(dir)} · session ${basename(file, ".jsonl")} · parent: — · base ${base}`,
				);
				expect(task()).toMatch(/- State: working · spend \$\d+\.\d\d\n/);

				// herdr restores the pane as a shell: omp starts again with the same appended brief and done-when.
				await server.service.refresh();
				server.herdr.restore();
				const before = server.herdr.calls.length;
				await server.service.refresh();
				const respawned = server.herdr.calls
					.slice(before)
					.filter((c) => c.method === "respawn")
					.map((c) => c.args[1] as { cwd: string; command: string[] });
				expect(respawned.find((o) => resolve(o.cwd) === path)?.command).toEqual(threadOmp(project.id));
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"Approve and execute: the task file records the plan once omp left plan mode, also when herdr says working first and the work goes on in a new session; spend and the ledger cover both sessions (0538, 0540, 0546)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startWithChecks(server, []);
				const task = () => readFileSync(join(taskDirOf(project.id), "task.md"), "utf8");
				const planning = proposedPlanFile(path);
				server.herdr.setState(paneId, "working", planning);
				server.herdr.setState(paneId, "idle", planning);
				await until(() => task().includes("- State: plan ready"));
				// He approves: herdr says working before omp wrote that it left plan mode.
				server.herdr.setState(paneId, "working", planning);
				await until(() => !task().includes("- State: plan ready"));
				expect(task()).toContain("## Plan\nNot approved yet.\n");
				// omp leaves plan mode, then goes on in a new session (`/clear`, no parent) with the plan copied to its folder.
				appendFileSync(planning, `${JSON.stringify({ type: "mode_change", id: "m3", mode: "none" })}\n`);
				const executing = sessionFile(path, [{ text: "Added the line.", cost: 0.5 }]);
				const local = join(executing.replace(/\.jsonl$/, ""), "local");
				mkdirSync(local, { recursive: true });
				writeFileSync(join(local, "readme-line-plan.md"), "1. Add the line.\n");
				server.herdr.setState(paneId, "idle", executing);
				await until(() => task().includes("## Plan (approved "));
				expect(task()).toMatch(/## Plan \(approved \d\d:\d\d\)\n1\. Add the line\.\n/);
				expect(task()).toContain(`session ${basename(executing, ".jsonl")}`);
				// Both sessions' spend: the plan's $0.25 and the work's $0.50.
				await until(() => task().includes("spend $0.75"));
				expect((await call(server, "thread_status", { thread: "Readme line" })).text).toContain("\nspend $0.75");
				// The task's commit ends a turn of the new session: its reviewer gets the plan approved in the old one (0453).
				server.herdr.setState(paneId, "working", executing);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				server.herdr.setState(paneId, "idle", executing);
				const review = () =>
					server.herdr
						.of("prompt")
						.map(([p, t]) => (p === paneId ? String(t) : ""))
						.find((t) => t.startsWith("Check this task's work."));
				await until(() => review() !== undefined);
				expect(review()).toContain("the approved plan (local://readme-line-plan.md), to check the diff against it");
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"report_result: a thread's report is validated, stored in its task file and shown by thread_status; its decisions go to the project's graph and to the boss for lessons (0539)",
	async () => {
		const project = await demoProject(env.root);
		const posts: unknown[] = [];
		await withServer(
			async (server) => {
				const { paneId } = await startThread(server);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				const report = {
					outcome: "done",
					summary: "Readme line added",
					files: ["README.md"],
					tests: [{ command: "bun test", passed: true }],
					decisions: [{ question: "Where the line goes", choice: "The top", why: "Read first" }],
					questions: [],
					followUps: ["Translate it"],
				};
				const notThread = {
					text: "report_result: only a thread's pane hands in a report, and this pane isn't a thread's.",
					isError: true,
				};
				expect(await call(server, "report_result", report)).toEqual(notThread);
				// A pane without a pane key may be one an earlier Jocasta started (0578).
				expect(await call(server, "report_result", report, null)).toEqual({
					...notThread,
					text: "report_result: only a thread's pane hands in a report, and this pane isn't a thread's; a pane Jocasta started before panes had keys gets one when Jocasta starts it again, once it is idle.",
				});
				expect(await call(server, "report_result", { ...report, summary: "x".repeat(201) }, paneId)).toMatchObject({
					isError: true,
				});
				expect(await call(server, "report_result", report, paneId)).toEqual({
					text: "Report stored for Readme line; its review reads it.",
				});
				const task = readFileSync(join(taskDirOf(project.id), "task.md"), "utf8");
				expect(task).toContain(
					'## Report\noutcome done · "Readme line added" · tests 1/1 · files: README.md · decisions: Where the line goes = The top · follow-ups: Translate it\n',
				);
				expect((await call(server, "thread_status", {})).text).toBe(
					"demo/Readme line [feature/readme-line] idle · reported done",
				);
				expect((await call(server, "thread_status", { thread: "Readme line" })).text).toEndWith(
					'\nreport: outcome done · "Readme line added" · tests 1/1 · files: README.md · decisions: Where the line goes = The top · follow-ups: Translate it',
				);
				await until(() => posts.length === 1);
				expect(posts[0]).toEqual([
					project.id,
					{
						name: expect.stringMatching(/^decisions t-[0-9a-f]{4} Readme line$/),
						body: 'Decisions settled by the thread "Readme line" (feature/readme-line) in demo:\n- Where the line goes: The top. Why: Read first',
						source: "report_result",
						at: expect.any(String),
					},
				]);
				const said =
					"[event] demo/Readme line — reported its decisions: «Where the line goes = The top (Read first)» · offer him the lasting ones as lessons, once";
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === boss && t === said));
			},
			{
				agentWaitMs: 10_000,
				graph: {
					search: async () => [],
					post: async (projectId, episode) => {
						posts.push([projectId, episode]);
					},
				},
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a task end without report_result gets one steer; the next end goes to the review, whose done_when Jocasta's runner checks first, giving the failing gate to the turn; its exit code goes in the proof, with no report said (0453, 0539, 0540, 0542, 0546)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const failing = "git rev-parse --verify no-such-ref";
				const { paneId, path } = await startWithChecks(server, [failing]);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				const prompts = () => server.herdr.of("prompt");
				server.herdr.setState(paneId, "working");
				commitIn(path, "demo\nA line.\n", "Add a README line");
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().some(([p, t]) => p === paneId && t === REPORT_STEER));
				expect(server.herdr.of("splitBeside")).toEqual([]);

				// It ends again without the report: the runner checks the done-when on the task's tree, then the review goes
				// on, once, with the brief and the failing gate.
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				const review = () =>
					prompts()
						.filter(([p]) => p === paneId)
						.map(([, t]) => String(t))
						.find((t) => t.startsWith("Check this task's work."));
				await until(() => review() !== undefined);
				const asked = review() as string;
				expect(asked).toMatch(/they failed; give the reviewer these gates too:\n\nchecks: FAIL \(tree [0-9a-f]{6}\)\n/);
				expect(asked).toContain(`  FAIL  done_when  ${failing}\n`);
				expect(asked).toContain(`        rerun: ${failing}`);
				// No project check ran here, so the turn still runs the project's tests itself.
				expect(asked).toContain("the way its AGENTS.md or package scripts say");
				expect(asked).toContain("his brief «Add a README line.», to check the diff against what he asked for");
				expect(asked).toContain("the fact that the thread handed in no report");
				expect(prompts().filter(([, t]) => t === REPORT_STEER)).toHaveLength(1);

				// The review turn: the project's check passes; the done-when's result is the runner's stored one.
				const file = sessionFile(path, [
					{ text: "Ran the check.", cost: 0.1, bash: { command: "bun run check", ok: true } },
					{ text: "Reviewed.\nTESTS: pass — `bun run check`", cost: 0.1 },
				]);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle");
				await until(() => prompts().some(([, t]) => t === HANDOFF_PROMPT));
				expect(
					prompts()
						.filter(([p]) => p === boss)
						.map(([, t]) => t),
				).toContain(
					`[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests pass, done-when \`${failing}\` exit 128, no report`,
				);
				const task = readFileSync(join(taskDirOf(project.id), "task.md"), "utf8");
				expect(task).toMatch(
					/## Reviews\n- \d\d:\d\d not a pass · `bun run check` exit 0 · `git rev-parse --verify no-such-ref` exit 128 · notes: 2 notes, tests pass, no report\n$/,
				);
			},
			{ agentWaitMs: 10_000, askReport: undefined, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"the check tool and the review say when the project's mise check didn't run, and why: never a silent pass (0473, 0559)",
	async () => {
		const project = await demoProject(env.root);
		const saved = { paranoid: process.env.FAKE_MISE_PARANOID, trust: process.env.FAKE_MISE_TRUST };
		// mise lists the project's config, but its paranoid trust is off: Jocasta may not load it.
		process.env.FAKE_MISE_PARANOID = "false\n";
		process.env.FAKE_MISE_TRUST = `${project.mainPath}\\mise.toml: trusted\n`;
		try {
			await withServer(
				async (server) => {
					const { paneId, path } = await startWithChecks(server, ["git --version"]);
					const boss = bossPane(server.herdr) as string;
					server.herdr.setState(boss, "idle");
					const skipped = "the project's check didn't run: mise's paranoid config trust is off";
					expect((await call(server, "check", {}, paneId)).text).toMatch(
						new RegExp(`^checks: PASS \\(tree [0-9a-f]{6}\\)\\nNote: ${skipped}\\.$`),
					);
					const prompts = () => server.herdr.of("prompt");
					server.herdr.setState(paneId, "working");
					commitIn(path, "demo\nA line.\n", "Add a README line");
					server.herdr.setState(paneId, "idle");
					await until(() =>
						prompts().some(([p, t]) => p === paneId && String(t).startsWith("Check this task's work.")),
					);
					const file = sessionFile(path, [
						{ text: "Reviewed.\nTESTS: pass — `bun test`", cost: 0.1, bash: { command: "bun test", ok: true } },
					]);
					server.herdr.setState(paneId, "working", file);
					server.herdr.setState(paneId, "idle");
					await until(() => prompts().some(([, t]) => t === HANDOFF_PROMPT));
					const said = prompts()
						.filter(([p]) => p === boss)
						.map(([, t]) => String(t))
						.find((t) => t.includes("done and reviewed"));
					expect(said).toContain(`, ${skipped}, no report`);
					expect(readFileSync(join(taskDirOf(project.id), "task.md"), "utf8")).toContain(
						`notes: 2 notes, tests pass, ${skipped}, no report\n`,
					);
				},
				{
					agentWaitMs: 10_000,
					...HUNK_OPTIONS,
					mise: [process.execPath, join(import.meta.dir, "fake-mise.ts")],
				},
			);
		} finally {
			for (const [key, value] of [
				["FAKE_MISE_PARANOID", saved.paranoid],
				["FAKE_MISE_TRUST", saved.trust],
			] as const) {
				if (value === undefined) delete process.env[key];
				else process.env[key] = value;
			}
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"the check tool answers a thread with its checks on its tree; a turn ending with failing edits gets the gates once; thread_status says each thread's ledger line (0538, 0542, 0546)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const clean = "git diff --quiet";
				const { paneId, path } = await startWithChecks(server, [clean]);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				// From the thread's pane only; a clean tree passes in one line.
				expect((await call(server, "check", {}, null)).isError).toBe(true);
				expect((await call(server, "check", {}, paneId)).text).toMatch(/^checks: PASS \(tree [0-9a-f]{6}\)$/);

				// A turn ends with an uncommitted edit: the check on idle fails it, and the gates go into the pane under
				// Jocasta's frame (0583, PRM-16).
				writeFileSync(join(path, "README.md"), "demo\nchanged\n");
				const file = sessionFile(path, [{ text: "Edited the README.", cost: 0.25 }]);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle");
				const gates = () =>
					server.herdr
						.of("prompt")
						.filter(([p, t]) => p === paneId && String(t).startsWith(IDLE_CHECK_FRAME))
						.map(([, t]) => String(t));
				await until(() => gates().length === 1);
				const block = (gates()[0] as string).slice(IDLE_CHECK_FRAME.length + 2);
				expect(block).toStartWith("checks: FAIL");
				expect(block).toContain(`  FAIL  done_when  ${clean}\n`);
				expect(block).toContain(`        rerun: ${clean}`);
				// The check tool answers the same tree from the stored result, without the frame.
				expect((await call(server, "check", {}, paneId)).text).toBe(block);

				const status = await call(server, "thread_status", {});
				expect(status.text).toContain("\n  spend $0.25");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"jocasta thread and worktree commands run the service's own functions and rules, worded for him; send, stop, finish and remove ask him first, and Deny does nothing; a done_when the guard denies or asks about is refused (0544, 0580, 0581)",
	async () => {
		const project = await demoProject(env.root);
		const asks: Confirmation[] = [];
		let allow = false;
		await withServer(
			async (server) => {
				const brief = "Add a README line. Keep it short.";
				const replied = cli(server, "thread new", { project: "demo", brief, done_when: ["bun test"] });
				await until(() => server.herdr.of("createWorktree").length === 1);
				const pane = server.herdr.paneList.at(-1);
				if (!pane?.cwd) throw new Error("no thread pane");
				server.herdr.setState(pane.paneId, "idle");
				expect(await replied).toEqual({
					text: "Started demo/Add a README line on feature/add-a-readme-line: it plans first; approve its plan in its pane.",
				});
				// No Allow for a new thread, even outside the project he last focused: it waits at its plan for him.
				expect(asks).toEqual([]);
				expect(readFileSync(join(taskDirOf(project.id, "Add a README line"), "task.md"), "utf8")).toContain(
					`## Brief\n${brief}\n\n## Done when\n- \`bun test\` passes\n`,
				);
				const line = "demo/Add a README line [feature/add-a-readme-line] idle";
				expect(await cli(server, "thread list")).toEqual({ text: line });
				expect(await cli(server, "thread list", { project: "demo" })).toEqual({ text: line });
				expect((await cli(server, "thread status", { thread: "Add a README line" })).text).toEndWith(
					"\nreport: none yet",
				);

				// Deny: nothing stops, nothing is typed.
				expect(await cli(server, "thread stop", { thread: "Add a README line" })).toEqual({
					text: "You chose Deny; nothing was stopped.",
					isError: true,
				});
				expect(server.herdr.of("stop")).toEqual([]);
				expect(asks.at(-1)).toEqual({
					message: "Stop demo/Add a README line's run?",
					detail: "A `jocasta thread stop` command asks to stop the thread's current run (one Escape into its omp).",
				});
				expect(await cli(server, "thread send", { thread: "Add a README line", text: "Go on." })).toEqual({
					text: "You chose Deny; nothing was sent.",
					isError: true,
				});
				expect(server.herdr.of("prompt").some(([, t]) => t === "Go on.")).toBe(false);
				expect(asks.at(-1)?.detail).toBe(
					"A `jocasta thread send` command asks to type it into the thread's pane:\n\nGo on.",
				);

				// The tools' rules hold, and refuse before anything is asked.
				const asked = asks.length;
				expect(await cli(server, "thread finish", { thread: "Add a README line", how: "merge" })).toEqual({
					text: "demo/Add a README line has no commits ahead of main; nothing to finish.",
					isError: true,
				});
				expect(await cli(server, "worktree remove", { path: join(env.root, "nope") })).toMatchObject({ isError: true });
				expect(asks).toHaveLength(asked);
				expect(await cli(server, "worktree list")).toEqual({ text: "No leftover worktrees." });

				// Allow: it runs.
				allow = true;
				expect(await cli(server, "thread stop", { thread: "Add a README line" })).toEqual({
					text: "Stopped demo/Add a README line's run.",
				});
				expect(server.herdr.of("stop")).toEqual([[pane.paneId]]);
				expect(await cli(server, "thread send", { thread: "Add a README line", text: "Go on." })).toEqual({
					text: "Sent to demo/Add a README line.",
				});
				expect(server.herdr.of("prompt")).toContainEqual([pane.paneId, "Go on."]);
				expect(await cli(server, "nope")).toEqual({ text: "no command jocasta nope.", isError: true });
				// done_when runs outside omp's guard, so the guard's own rules decide first (0580): deny refuses, and one
				// it would ask about is refused here, as this door asks nothing. Nothing is made either way.
				const worktrees = server.herdr.of("createWorktree").length;
				expect(
					await cli(server, "thread new", {
						project: "demo",
						brief: "Push it.",
						done_when: ["git push --force origin main"],
					}),
				).toEqual({
					text: "jocasta thread new: done_when `git push --force origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started.",
					isError: true,
				});
				expect(
					(await cli(server, "thread new", { project: "demo", brief: "Ship it.", done_when: ["npm publish"] })).text,
				).toStartWith("jocasta thread new: done_when `npm publish` needs your Allow under Jocasta's guard");
				expect(server.herdr.of("createWorktree")).toHaveLength(worktrees);
				expect(asks).toHaveLength(asked + 2);
				const unauthorized = await fetch(`http://127.0.0.1:${server.port}/api/cli`, { method: "POST", body: "{}" });
				expect(unauthorized.status).toBe(401);
			},
			{
				agentWaitMs: 10_000,
				confirm: async (ask) => {
					asks.push(ask);
					return { allow, how: "answered" };
				},
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread key pressed in today's own pane names today's key for it; any other pane that isn't a thread's gets the plain refusal (0581, UX-16)",
	async () => {
		await withServer(async (server) => {
			const boss = bossPane(server.herdr) as string;
			const today = await server.herdr.splitBeside(boss, {
				command: ["jocasta", "today"],
				cwd: env.root,
				label: "today",
			});
			await expect(server.service.keyStop(today)).rejects.toThrow("Stop: in today, press s on the picked thread.");
			await expect(server.service.keyReview(today)).rejects.toThrow("Review: in today, press r on the picked thread.");
			await expect(server.service.keyFinishable(today)).rejects.toThrow(
				"Finish: in today, press f on the picked thread.",
			);
			await expect(server.service.keyStop(boss)).rejects.toThrow(
				"Stop: the focused pane isn't a thread's; focus a thread or pick one in today.",
			);
		});
	},
	GIT_TEST_TIMEOUT,
);
