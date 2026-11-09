import { expect, setSystemTime, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Confirmation } from "../../../src/frontier/server/allow.ts";
import type { CodeIndex } from "../../../src/frontier/server/code-index.ts";
import {
	COMMIT_LINE,
	fallbackSlug,
	HANDOFF_PROMPT,
	LESSONS_LINE,
	REPORT_STEER,
	WHEN_YOU_FINISH,
} from "../../../src/frontier/server/wording.ts";
import { HOSTS_ENV, LAYER_ENV } from "../../../src/shared/layer.ts";
import { GIT_TEST_TIMEOUT, readJsonl, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { fakeHost } from "./fake-hosts.ts";
import {
	BRIEF,
	bossIdle,
	bossLines,
	bossPane,
	call,
	commitIn,
	demoProject,
	Page,
	relaunchIdle,
	review,
	sessionFile,
	startTestServer,
	startThread,
	TEST_BOSS_KEY,
	type TestServer,
	taskDirOf,
	threadPane,
	until,
	useHunkDaemon,
	withServer,
	writeSettings,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

// ---- the full review's regressions (0573) ----

/** A tool call as `jocasta mcp` sends it from a pane whose `JOCASTA_PANE_KEY` is `paneKey`, without the boss key. */
async function callWithKey(
	server: TestServer,
	tool: string,
	args: Record<string, unknown>,
	paneKey: string,
): Promise<{ text: string; isError?: boolean }> {
	const response = await fetch(`http://127.0.0.1:${server.port}/api/mcp/${tool}`, {
		method: "POST",
		headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
		body: JSON.stringify({ args, paneKey }),
	});
	return (await response.json()) as { text: string; isError?: boolean };
}

/** A herdr whose prompts into the boss pane wait while `gate` is set, as herdr's agent.prompt round trip takes its time. */
class GatedHerdr extends FakeHerdr {
	gate: Promise<void> | undefined;
	waiting = false;
	override async prompt(paneId: string, text: string): Promise<void> {
		if (this.gate && paneId === bossPane(this)) {
			this.waiting = true;
			await this.gate;
		}
		return super.prompt(paneId, text);
	}
}

test(
	"SVC-2: a thread started while the tick tells the boss of a closed one is kept, and never said closed",
	async () => {
		await demoProject(env.root);
		const herdr = new GatedHerdr();
		await withServer(
			async (server) => {
				herdr.setState(bossPane(herdr) as string, "idle");
				const a = await startThread(server, { title: "Task A" });
				await herdr.closeWorkspace(herdr.paneList.find((p) => p.paneId === a.paneId)?.workspaceId as string);
				// The tick that sees A gone tells the boss, and that prompt takes its time; B starts meanwhile.
				const open = Promise.withResolvers<void>();
				herdr.gate = open.promise;
				const tick = server.service.refresh();
				await until(() => herdr.waiting);
				const b = await startThread(server, { title: "Task B" });
				herdr.gate = undefined;
				open.resolve();
				await tick;
				bossIdle(server);
				await server.service.refresh();
				bossIdle(server);
				await server.service.refresh();
				expect(herdr.paneList.some((p) => p.paneId === b.paneId)).toBe(true);
				expect((await call(server, "thread_status", {})).text).toBe("demo/Task B [feature/task-b] idle");
				expect(bossLines(server).filter((l) => l.includes("closed;"))).toEqual([
					"[event] demo/Task A — closed; its worktree and branch stay",
				]);
			},
			{ herdr },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-3: a project pane whose omp can't start again doesn't end the tick: the thread pane runs omp again and the HUD is written",
	async () => {
		const project = await demoProject(env.root);
		const herdr = new FakeHerdr();
		let server = await startTestServer({ herdr });
		let path = "";
		try {
			({ path } = await startThread(server));
			await server.service.refresh();
		} finally {
			await server.stop();
		}
		const hud = join(env.data, "hud.txt");
		expect(readFileSync(hud, "utf8")).toBe("Jocasta isn't running; start it with jocasta.\n");
		herdr.restore();
		// The project's own omp config names a public relay: its pane is refused (0536, 0561); the thread's isn't.
		const main = realpathSync(project.mainPath);
		const relayUrlOf = async (_command: string[], cwd: string) =>
			realpathSync(cwd) === main ? "wss://relay.example.org" : "ws://127.0.0.1:47499";
		server = await startTestServer({ herdr, relayUrlOf });
		try {
			await server.service.refresh();
			expect(herdr.of("respawn").some(([, opts]) => resolve((opts as { cwd: string }).cwd) === path)).toBe(true);
			expect(readFileSync(hud, "utf8")).toMatch(/^0 awaiting you · /);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-5: thread_finish looks again after his Allow: a thread at work meanwhile isn't merged (0544)",
	async () => {
		const project = await demoProject(env.root);
		const allow = Promise.withResolvers<boolean>();
		let asked = false;
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				const trunkHead = sh(project.mainPath, "rev-parse", "HEAD");
				const finishing = call(server, "thread_finish", { thread: "Readme line", how: "merge" });
				await until(() => asked);
				// While his Allow waits, the thread is at work again.
				server.herdr.setState(paneId, "working");
				await until(() => server.service.busyThreads().working === 1);
				allow.resolve(true);
				expect(await finishing).toEqual({
					text: "demo/Readme line isn't idle (working); finish it once it is.",
					isError: true,
				});
				expect(sh(project.mainPath, "rev-parse", "HEAD")).toBe(trunkHead);
				expect(server.herdr.of("closeWorkspace")).toEqual([]);
			},
			{
				confirm: async (ask) => {
					if (ask.message.startsWith("Start the thread")) return { allow: true, how: "answered" };
					asked = true;
					return { allow: await allow.promise, how: "answered" };
				},
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-6: an omp started again mid-review runs the review again, and mid-/handoff runs /handoff again (0582)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			const { herdr } = server;
			const { paneId, path } = await startThread(server);
			const range = "main...feature/readme-line";
			const typedInto = (pane: string, text: string) =>
				herdr.of("prompt").filter(([p, t]) => p === pane && t === text).length;
			herdr.setState(paneId, "working");
			commitIn(path, "demo\nA line.\n", "Add a README line");
			herdr.setState(paneId, "idle");
			await until(() => typedInto(paneId, review(range)) === 1);
			// Mid-review, herdr restores the pane as a shell: omp starts again, idle, its review turn gone.
			herdr.setState(paneId, "working");
			await relaunchIdle(server, herdr, path);
			const reviewing = threadPane(herdr, path);
			expect(reviewing).not.toBe(paneId);
			await until(() => typedInto(reviewing, review(range)) === 1);
			// The review turn ends on its test line: /handoff. omp starts again mid-handoff: /handoff goes in again.
			const file = sessionFile(path, [
				{ text: "Reviewed.\n\n**TESTS: pass — bun test**", cost: 0.1, bash: { command: "bun test", ok: true } },
			]);
			herdr.setState(reviewing, "working", file);
			herdr.setState(reviewing, "idle");
			await until(() => typedInto(reviewing, HANDOFF_PROMPT) === 1);
			herdr.setState(reviewing, "working");
			await relaunchIdle(server, herdr, path, file);
			const handing = threadPane(herdr, path);
			expect(handing).not.toBe(reviewing);
			await until(() => typedInto(handing, HANDOFF_PROMPT) === 1);
		}, HUNK_OPTIONS);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-6: an omp started again mid-PR goes back to its task, and the boss hears to finish it again (0582)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "remote", "add", "origin", "https://github.com/test/demo.git");
		const ghState = join(env.root, "gh-state.json");
		writeFileSync(ghState, "{}");
		await withServer(
			async (server) => {
				const { herdr } = server;
				const { paneId, path } = await startThread(server);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				const pr = () => call(server, "thread_finish", { thread: "Readme line", how: "pr" });
				expect((await pr()).text).toStartWith("Asked demo/Readme line to push feature/readme-line");
				herdr.setState(paneId, "working");
				await relaunchIdle(server, herdr, path);
				const line =
					"[event] demo/Readme line — no PR: its omp started again mid-turn; everything stays, finish it again";
				await until(() => {
					bossIdle(server);
					return bossLines(server).includes(line);
				});
				expect(existsSync(path)).toBe(true);
				// Back at its task: finishing it asks for the PR again.
				expect((await pr()).text).toStartWith("Asked demo/Readme line to push feature/readme-line");
			},
			{ gh: [process.execPath, join(import.meta.dir, "fake-gh.ts"), ghState] },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-7: a start that fails once herdr made its worktree takes back the workspace, worktree and branch; the next start gets the same name",
	async () => {
		const project = await demoProject(env.root);
		await withServer(async (server) => {
			server.herdr.createWorktreeFails = new Error("herdr's layout failed");
			const reply = await call(server, "thread_start", { project: "demo", title: "Readme line", brief: BRIEF });
			expect(reply.isError).toBe(true);
			expect(reply.text).toContain("herdr's layout failed");
			const folder = join(env.root, "demo.wt", "readme-line");
			expect(existsSync(folder)).toBe(false);
			expect(sh(project.mainPath, "branch", "--list", "feature/readme-line")).toBe("");
			expect(server.herdr.workspaceList.map((w) => w.label)).not.toContain("Readme line");
			expect((await call(server, "thread_status", {})).text).toBe("No threads.");
			server.herdr.createWorktreeFails = undefined;
			expect((await startThread(server)).path).toBe(folder);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-9: a thread closed in quiet hours is in today's activity at once and reaches the boss once they end (0298)",
	async () => {
		await demoProject(env.root);
		writeSettings(env.config, { service: { quietHours: "22:00-08:00" } });
		let at = new Date(2026, 9, 24, 17, 0);
		await withServer(
			async (server) => {
				server.herdr.setState(bossPane(server.herdr) as string, "idle");
				const { paneId } = await startThread(server);
				const { day } = server.service;
				at = new Date(2026, 9, 24, 23, 0);
				day.tick();
				const what = "demo/Readme line — closed; its worktree and branch stay";
				const toBoss = () => bossLines(server).filter((l) => l === `[event] ${what}`).length;
				await server.herdr.closeWorkspace(
					server.herdr.paneList.find((p) => p.paneId === paneId)?.workspaceId as string,
				);
				await server.service.refresh();
				await until(() => day.dashboard().activity.events.some((e) => e.text === what));
				bossIdle(server);
				expect(toBoss()).toBe(0);
				// Quiet hours over: the line that waited goes in, once.
				at = new Date(2026, 9, 25, 8, 1);
				day.tick();
				await until(() => toBoss() === 1);
				bossIdle(server);
				await server.service.refresh();
				expect(toBoss()).toBe(1);
			},
			{ day: { pollers: false, now: () => at } },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-13: Jocasta closing refuses new calls and waits for a merge in flight, which lands and removes its thread (0549)",
	async () => {
		const project = await demoProject(env.root);
		const main = project.mainPath;
		sh(main, "config", "user.name", "Test");
		sh(main, "config", "user.email", "test@example.com");
		const server = await startTestServer();
		let stopping: Promise<void> | undefined;
		try {
			const { path } = await startThread(server);
			commitIn(path, "demo\nA line.\n", "Add a README line");
			// The repo's pre-commit hook takes a while (CO-10); Jocasta is closed meanwhile.
			const hooks = join(env.root, "hooks");
			const marker = join(env.root, "hook-ran");
			mkdirSync(hooks, { recursive: true });
			writeFileSync(join(hooks, "pre-commit"), `#!/bin/sh\ntouch "${marker.replaceAll("\\", "/")}"\nsleep 1\n`, {
				mode: 0o755,
			});
			sh(main, "config", "core.hooksPath", hooks);
			const boss = { bossKey: TEST_BOSS_KEY };
			const finishing = server.service.callTool("thread_finish", { thread: "Readme line", how: "merge" }, boss);
			await until(() => existsSync(marker));
			stopping = server.stop();
			// A call once the close began is refused, nothing done.
			let refused = "";
			const ask = (): void =>
				void server.service.callTool("thread_status", {}, boss).then((reply) => {
					if (reply.isError) refused = reply.text;
					else ask();
				});
			ask();
			await until(() => refused !== "");
			expect(refused).toBe("Jocasta is closing; nothing was changed.");
			expect((await finishing).text).toMatch(
				/^Merged demo\/Readme line into main as [0-9a-f]+ \(1 commit squashed\); its worktree, workspace and branch are removed\.$/,
			);
			expect(existsSync(path)).toBe(false);
			await stopping;
		} finally {
			await (stopping ?? server.stop());
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SVC-14: a project's running count goes to its own workspace, not a later one with the same label",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			const { paneId } = await startThread(server);
			const projectWs = server.herdr.workspaceList.find((w) => w.label === "demo")?.id as string;
			server.herdr.workspaceList.push({ id: "w_late", label: "demo", cwd: env.root });
			server.herdr.setState(paneId, "working");
			await until(() => server.service.busyThreads().working === 1);
			await server.service.refresh();
			const reported = server.herdr.of("reportWorkspaceMetadata").map(([id]) => id);
			expect(reported).toContain(projectWs);
			expect(reported).not.toContain("w_late");
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SEC-LOC-6, SVC-14: worktree_remove keeps a folder whose worktrees git can't list, says why, and the folder keeps its number",
	async () => {
		const project = await demoProject(env.root);
		const config = join(project.mainPath, ".git", "config");
		let saved = "";
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				await server.herdr.closeWorkspace(
					server.herdr.paneList.find((p) => p.paneId === paneId)?.workspaceId as string,
				);
				await server.service.refresh();
				// His Allow comes; git then fails on the repo's config: the folder may hold his work, so it stays.
				expect(await call(server, "worktree_remove", { path })).toEqual({
					text: `${path} couldn't be removed (git couldn't list demo's worktrees, so the folder stays); it stays.`,
					isError: true,
				});
				writeFileSync(config, saved);
				expect(existsSync(join(path, "README.md"))).toBe(true);
				const numbers = JSON.parse(readFileSync(join(env.data, "thread-numbers.json"), "utf8")) as Record<
					string,
					number
				>;
				expect(Object.values(numbers)).toEqual([1]);
			},
			{
				// The thread's start asks too (no project pane focused yet); only the removal's Allow breaks git.
				confirm: async (ask) => {
					if (!ask.message.startsWith("Remove")) return { allow: true, how: "answered" };
					saved = readFileSync(config, "utf8");
					writeFileSync(config, `${saved}\n[broken\n`);
					return { allow: true, how: "answered" };
				},
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SEC-LOC-7: Finish refuses a thread whose worktree was switched to another branch; nothing is merged",
	async () => {
		const project = await demoProject(env.root);
		await withServer(async (server) => {
			const { path } = await startThread(server);
			commitIn(path, "demo\nA line.\n", "Add a README line");
			sh(path, "checkout", "-q", "-b", "elsewhere");
			const trunkHead = sh(project.mainPath, "rev-parse", "HEAD");
			expect(await call(server, "thread_finish", { thread: "Readme line", how: "merge" })).toEqual({
				text: "demo/Readme line's worktree is on elsewhere, not feature/readme-line; finish it on its branch.",
				isError: true,
			});
			expect(sh(project.mainPath, "rev-parse", "HEAD")).toBe(trunkHead);
			expect(sh(project.mainPath, "branch", "--list", "feature/readme-line")).toContain("feature/readme-line");
			expect(server.herdr.of("closeWorkspace")).toEqual([]);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"SEC-LOC-5: at the boss's door a done_when the guard denies is refused unasked; one it asks about waits for his Allow, which names it (0580)",
	async () => {
		await demoProject(env.root);
		const asks: Confirmation[] = [];
		let allow = false;
		await withServer(
			async (server) => {
				server.herdr.ompUpOnCreate = "idle";
				const start = (doneWhen: string) =>
					call(server, "thread_start", { project: "demo", title: "Ship", brief: "Ship it.", done_when: [doneWhen] });
				expect(await start("git push --force origin main")).toEqual({
					text: "thread_start: done_when `git push --force origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started.",
					isError: true,
				});
				// Quotes don't launder a denied command: omp's own matcher sees it without them.
				expect(await start('git push --for"ce" origin main')).toEqual({
					text: 'thread_start: done_when `git push --for"ce" origin main` is denied by Jocasta\'s guard (`*git*push* --force *`); no thread was started.',
					isError: true,
				});
				expect(asks).toEqual([]);
				expect(await start("npm publish")).toEqual({ text: "He chose Deny; no thread was started.", isError: true });
				expect(asks).toHaveLength(1);
				expect(asks[0]?.message).toBe('Start the thread "Ship" in demo?');
				expect(asks[0]?.detail).toContain("It runs when the work is reviewed:  `npm publish`");
				expect(server.herdr.of("createWorktree")).toEqual([]);
				allow = true;
				expect((await start("npm publish")).text).toStartWith("Started demo/Ship on branch feature/ship.");
			},
			{
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
	"SEC-NET-6: a pane key names only its own pane: a forged one gets no thread's tools, and a thread's key reports for that thread (0578)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			await startThread(server);
			const second = await startThread(server, { title: "Second", brief: "More." });
			const report = {
				outcome: "done",
				summary: "Second done",
				files: ["README.md"],
				tests: [{ command: "bun test", passed: true }],
				decisions: [],
				questions: [],
				followUps: [],
			};
			const forged = "f".repeat(64);
			expect(await callWithKey(server, "report_result", report, forged)).toEqual({
				text: "report_result: only a thread's pane hands in a report, and this pane isn't a thread's.",
				isError: true,
			});
			expect(await callWithKey(server, "check", {}, forged)).toEqual({
				text: "check: only a thread's pane runs its checks, and this pane isn't a thread's.",
				isError: true,
			});
			const key = server.herdr.envOf(second.paneId)?.JOCASTA_PANE_KEY as string;
			expect(key).toMatch(/^[0-9a-f]{64}$/);
			expect(await callWithKey(server, "report_result", report, key)).toEqual({
				text: "Report stored for Second; its review reads it.",
			});
			const status = (await call(server, "thread_status", {})).text.split("\n");
			expect(status).toContain("demo/Second [feature/second] idle · reported done");
			expect(status).toContain("demo/Readme line [feature/readme-line] idle");
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"UX-11, C11: his New thread key starts a Macedonian title on a transliterated branch and another script's on thread-<date>-<n>; each answer is {text, isError}, a half-done start an error (0581)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			server.herdr.ompUpOnCreate = "idle";
			const mk = await server.service.keyStart("demo", "Поправи го најавувањето кога сесијата е истечена.");
			expect(mk.isError).toBe(false);
			expect(mk.text).toMatch(
				/^Started demo\/Поправи го најавувањето.* on feature\/popravi-go-najavuvanjeto[a-z-]*: it plans first; approve its plan in its pane\.$/,
			);
			const zh = await server.service.keyStart("demo", "修复登录问题");
			expect(zh).toEqual({
				text: `Started demo/修复登录问题 on feature/${fallbackSlug(new Date(), 1)}: it plans first; approve its plan in its pane.`,
				isError: false,
			});
			// omp never comes up in the pane: started, its brief not sent; half done.
			server.herdr.ompUpOnCreate = undefined;
			const half = await server.service.keyStart("demo", "Add a README line.");
			expect(half.isError).toBe(true);
			expect(half.text).toEndWith(", but omp didn't come up in its pane within 0 s; the brief wasn't sent.");
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"C11: his Finish key's merge that landed but left its workspace is half done: {text, isError: true} (0581)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(async (server) => {
			const { paneId, path } = await startThread(server);
			commitIn(path, "demo\nA line.\n", "Add a README line");
			server.herdr.closeWorkspace = async () => {
				throw new Error("herdr is gone");
			};
			const outcome = await server.service.keyFinish(paneId, "merge");
			expect(outcome.isError).toBe(true);
			expect(outcome.text).toEndWith(
				", but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay.",
			);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"UX-13, FE-4: hud.txt says what awaits him and today's spend; the page gets the HUD when only the quota changed (0581)",
	async () => {
		await demoProject(env.root);
		const usage = JSON.stringify({ reports: [{ limits: [{ window: { id: "5h" }, amount: { usedFraction: 0.5 } }] }] });
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				try {
					const hud = join(env.data, "hud.txt");
					await server.service.refresh();
					const before = readFileSync(hud, "utf8");
					expect(before).toMatch(/^0 awaiting you · (\$\d+\.\d\d|…) today\n$/);
					await server.service.day.readQuota();
					await server.service.refresh();
					await page.waitFor((m) => m.type === "hud" && m.hud.quota.some((q) => q.window === "5h" && q.used === 0.5));
					expect(readFileSync(hud, "utf8")).toBe(before);
				} finally {
					page.close();
				}
			},
			{ day: { pollers: false, usage: async () => usage } },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"PRM-11: a thread's spend passing $50 reaches the boss once (0541)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			server.herdr.setState(bossPane(server.herdr) as string, "idle");
			const { paneId, path } = await startThread(server);
			const file = sessionFile(path, [{ text: "Spent a lot.", cost: 51.2 }]);
			server.herdr.setState(paneId, "working", file);
			server.herdr.setState(paneId, "idle");
			const line = "[event] demo/Readme line — spend passed $50.00 ($51.20); omp warned in its pane";
			await until(() => {
				bossIdle(server);
				void server.service.refresh();
				return bossLines(server).includes(line);
			});
			bossIdle(server);
			await server.service.refresh();
			expect(bossLines(server).filter((l) => l === line)).toHaveLength(1);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"C6: with the layer off, a thread pane gets JOCASTA_LAYER=off, its prompt no Done when or When you finish, its brief the commit line, no report steer, and graph_search no code index (0576)",
	async () => {
		const project = await demoProject(env.root);
		const located: unknown[] = [];
		const saved = process.env[LAYER_ENV];
		process.env[LAYER_ENV] = "off";
		try {
			await withServer(
				async (server) => {
					server.herdr.ompUpOnCreate = "idle";
					await call(server, "thread_start", {
						project: "demo",
						title: "Readme line",
						brief: BRIEF,
						done_when: ["git diff --quiet"],
					});
					const pane = server.herdr.paneList.at(-1);
					if (!pane?.cwd) throw new Error("no thread pane");
					const { paneId, cwd: path } = pane;
					expect(server.herdr.envOf(paneId)?.[LAYER_ENV]).toBe("off");
					const prompt = readFileSync(join(taskDirOf(project.id), "prompt.md"), "utf8");
					expect(prompt).not.toContain("## Done when");
					expect(prompt).not.toContain(WHEN_YOU_FINISH[0]);
					expect(server.herdr.of("prompt")).toContainEqual([paneId, `${BRIEF}\n\n${LESSONS_LINE}\n${COMMIT_LINE}`]);
					// A task's end without a report goes to its review: no steer asks for one.
					server.herdr.setState(paneId, "working");
					commitIn(path, "demo\nA line.\n", "Add a README line");
					server.herdr.setState(paneId, "idle");
					const reviewStart = review("main...feature/readme-line").split("\n")[0] as string;
					await until(() =>
						server.herdr.of("prompt").some(([p, t]) => p === paneId && String(t).startsWith(reviewStart)),
					);
					expect(server.herdr.of("prompt").some(([, t]) => t === REPORT_STEER)).toBe(false);
					const searched = await call(server, "graph_search", { query: "who calls addNumbers?", project: "demo" });
					expect(searched.isError).toBeUndefined();
					expect(located).toEqual([]);
				},
				{
					...HUNK_OPTIONS,
					askReport: undefined,
					graph: { post: async () => {}, search: async () => [] },
					codeIndex: {
						locations: async (...args: unknown[]) => {
							located.push(args);
							return [];
						},
					} as unknown as CodeIndex,
				},
			);
		} finally {
			if (saved === undefined) delete process.env[LAYER_ENV];
			else process.env[LAYER_ENV] = saved;
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"C6b: with omp's host registry off, a thread whose omp hosts follows herdr's state, and the registry is never read (0576)",
	async () => {
		await demoProject(env.root);
		const saved = process.env[HOSTS_ENV];
		process.env[HOSTS_ENV] = "off";
		try {
			await withServer(
				async (server) => {
					const { paneId, path } = await startThread(server);
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
					try {
						server.herdr.roots.set(paneId, omp.pid);
						// herdr's working, then idle, decide: the brief's turn runs and ends.
						server.herdr.setState(paneId, "working");
						await until(() => server.service.busyThreads().working === 1);
						server.herdr.setState(paneId, "idle");
						await until(() => server.service.busyThreads().working === 0);
						// omp's host says a turn runs; it is never asked, and herdr's idle stands.
						await server.service.refresh();
						await server.service.refresh();
						expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 0 });
						expect(host.asked).toBe(0);
					} finally {
						await host.close();
						omp.kill();
						await omp.exited;
					}
				},
				{ hostPollMs: 50 },
			);
		} finally {
			if (saved === undefined) delete process.env[HOSTS_ENV];
			else process.env[HOSTS_ENV] = saved;
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"C7: each problem in jocasta.yaml is said once, as a notification at the start (0584)",
	async () => {
		writeSettings(env.config, { service: { colour: "red" } });
		await withServer(async (server) => {
			const said = () =>
				server.herdr
					.of("notify")
					.map(([note]) => (note as { body?: string }).body ?? "")
					.filter((body) => body.startsWith("settings: "));
			await until(() => said().length > 0);
			await server.service.refresh();
			expect(said()).toEqual([expect.stringMatching(/^settings: jocasta\.yaml:\d+: unknown key colour.*; ignored$/)]);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a start notification herdr couldn't show for want of a client is sent again once a client is attached, once; a later one isn't (0584)",
	async () => {
		writeSettings(env.config, { service: { colour: "red" } });
		const herdr = new FakeHerdr();
		// Bare `jocasta` starts the service before his terminal attaches.
		herdr.notifyResult = "no_foreground_client";
		await withServer(
			async (server) => {
				// Each call's line in notifications.jsonl is written once herdr answered it.
				const file = join(env.data, "notifications.jsonl");
				const results = () =>
					existsSync(file)
						? readJsonl(file)
								.filter((row) => String(row.body).startsWith("settings: "))
								.map((row) => row.result)
						: [];
				// The start's own refresh ticks may already have tried it again; each try while no client is kept.
				await until(() => results().length > 0);
				const tried = results().length;
				await until(async () => {
					await server.service.refresh();
					return results().length > tried;
				});
				herdr.notifyResult = "shown";
				await until(async () => {
					await server.service.refresh();
					return results().includes("shown");
				});
				// Shown: nothing is kept, so later ticks send nothing more.
				await server.service.refresh();
				await server.service.refresh();
				const said = results();
				expect(said.at(-1)).toBe("shown");
				expect(said.slice(0, -1).every((result) => result === "no_foreground_client")).toBe(true);
				expect(said.length).toBeGreaterThan(tried);
				// A client was attached: one herdr can't show later (his terminal closed since) isn't kept for the next.
				const later = { type: "toast", reason: "blocked", level: "warning", text: "Ship — waits for you" } as const;
				herdr.notifyResult = "no_foreground_client";
				await server.service.notify(later);
				herdr.notifyResult = "shown";
				await server.service.refresh();
				await server.service.refresh();
				const once = { title: "Ship", body: "waits for you", sound: "request" };
				expect(herdr.of("notify").filter(([note]) => Bun.deepEquals(note, once))).toHaveLength(1);
			},
			{ herdr },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"the start's notifications wait 10 minutes for a client: then they're dropped, and later ones aren't kept (0584)",
	async () => {
		const herdr = new FakeHerdr();
		herdr.notifyResult = "no_foreground_client";
		await withServer(
			async (server) => {
				const toast = (text: string) => ({ type: "toast", reason: "event", level: "warning", text }) as const;
				const sent = (body: string) =>
					herdr.of("notify").filter(([note]) => Bun.deepEquals(note, { title: "Jocasta", body, sound: "none" })).length;
				await server.service.notify(toast("early"));
				const t = Date.now();
				try {
					setSystemTime(new Date(t + 10 * 60_000 + 1_000));
					// Two ticks: one may find a resend from the service's own tick still running.
					await server.service.refresh();
					await server.service.refresh();
					await server.service.notify(toast("late"));
					const early = sent("early");
					herdr.notifyResult = "shown";
					await server.service.refresh();
					await server.service.refresh();
					expect(sent("early")).toBe(early);
					expect(sent("late")).toBe(1);
				} finally {
					setSystemTime();
				}
			},
			{ herdr },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"C10: the New thread form picks the picked thread's project, else the one of the pane he last focused, else the last one started in, else the first (0581)",
	async () => {
		const mains: Record<string, string> = {
			demo: (await demoProject(env.root)).mainPath,
			"client-a": (await demoProject(env.root, "client-a")).mainPath,
		};
		await withServer(async (server) => {
			const form = (paneId?: string) => server.service.newThreadForm(paneId);
			const { projects, project } = await form();
			expect([...projects].sort()).toEqual(["client-a", "demo"]);
			const [first, other] = projects as [string, string];
			expect(project).toBe(first);
			const thread = await startThread(server, { project: other });
			expect((await form()).project).toBe(other);
			// He focuses a pane in the first project's main checkout.
			await server.herdr.ensureWorkspace(first, mains[first] as string);
			const focused = server.herdr.paneList.at(-1);
			if (!focused) throw new Error("no pane");
			focused.focused = true;
			expect((await form()).project).toBe(first);
			// The thread picked in today wins.
			expect((await form(thread.paneId)).project).toBe(other);
		});
	},
	GIT_TEST_TIMEOUT,
);
