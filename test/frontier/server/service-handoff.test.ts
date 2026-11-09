import { expect, setSystemTime, test } from "bun:test";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { hunkEnv } from "../../../src/frontier/server/hunk.ts";
import { overlayFile } from "../../../src/frontier/server/overlay.ts";
import { logsDir } from "../../../src/shared/log.ts";
import { GIT_TEST_TIMEOUT, poll, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { fakeHost } from "./fake-hosts.ts";
import {
	bossPane,
	call,
	commitIn,
	demoProject,
	GUARD,
	handoffEntry,
	keyed,
	Page,
	README_THREAD_ENV,
	review,
	sessionFile,
	startTestServer,
	startThread,
	TEST_BOSS_KEY,
	threadOmp,
	toHandoff,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"thread_finish merge waits for a /handoff that hasn't written its entry, then merges (CO-2)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const file = await toHandoff(server, paneId, path);
				const finish = () => call(server, "thread_finish", { thread: "Readme line", how: "merge" });
				expect(await finish()).toEqual({
					text: "demo/Readme line is still writing its handoff; try again in a minute.",
					isError: true,
				});
				expect(existsSync(path)).toBe(true);
				handoffEntry(file);
				// The session index reads a file again at most once a second: asked until it sees the entry.
				let reply = await finish();
				await poll(
					async () => {
						if (!reply.text.endsWith("is still writing its handoff; try again in a minute.")) return true;
						reply = await finish();
						return false;
					},
					{ timeoutMs: 15_000, what: "thread_finish past the handoff" },
				);
				expect(reply.isError).toBeUndefined();
				expect(reply.text).toStartWith("Merged demo/Readme line into main");
				expect(existsSync(path)).toBe(false);
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread states stay true: a dismissed question ends the turn, a handoff with no entry doesn't swallow the next task, and the page sees the boss take an urgent line (CO-3, CO-4, CO-7)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				await page.waitFor((m) => m.type === "snapshot");
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				server.herdr.setState(boss, "idle");
				await until(() => server.service.bossState() === "idle");

				// He dismisses its question: nothing awaits him any more.
				const asked = sessionFile(path, [{ text: "Which format?", cost: 0.1, ask: "CSV or JSON?" }]);
				server.herdr.setState(paneId, "working", asked);
				server.herdr.setState(paneId, "blocked");
				await until(() => server.service.hud().blocked === 1);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });
				// The urgent line went into the idle boss at once.
				await until(() =>
					server.herdr.of("prompt").some(([p, t]) => p === boss && String(t).includes("— awaiting you")),
				);
				server.herdr.setState(paneId, "idle");
				await until(() => server.service.hud().blocked === 0);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 0 });

				// The task ends, the review runs, /handoff is sent and writes no entry; the next task commits.
				await toHandoff(server, paneId, path);
				server.herdr.setState(paneId, "working");
				commitIn(path, "demo\nA line.\nAnother.\n", "Another line");
				server.herdr.setState(paneId, "idle");
				const reviewText = review("main...feature/readme-line");
				await until(() => server.herdr.of("prompt").filter(([p, t]) => p === paneId && t === reviewText).length === 2);
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread whose omp host answers leaves the handoff step once its session has the handoff's entry (F-01, 0547)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const file = await toHandoff(server, paneId, path);
				// The pane's omp: a real process, so its pid and start can be read (0490); its host answers idle.
				const omp = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
					stdio: ["ignore", "ignore", "ignore"],
					windowsHide: true,
				});
				const host = await fakeHost(join(env.data, "collab-hosts"), {
					pid: omp.pid,
					cwd: path,
					busy: false,
					inputRequired: false,
				});
				const ended = () => {
					const dir = logsDir();
					return (
						existsSync(dir) &&
						readdirSync(dir)
							.map((name) => readFileSync(join(dir, name), "utf8"))
							.join("")
							.includes("demo/Readme line: handoff → task (its handoff is written)")
					);
				};
				try {
					server.herdr.roots.set(paneId, omp.pid);
					await until(async () => {
						await server.service.refresh();
						return host.asked >= 2;
					});
					// /handoff ran with no turn reported by herdr or the host; its entry is in the session now.
					handoffEntry(file);
					await until(async () => {
						await server.service.refresh();
						return ended();
					});
				} finally {
					await host.close();
					omp.kill();
					await omp.exited;
				}
			},
			{ agentWaitMs: 10_000, hostPollMs: 50, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"two thread_start calls for one project at once make one project workspace (CO-5)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			const start = (title: string) => call(server, "thread_start", { project: "demo", title, brief: "Go." });
			const replies = await Promise.all([start("One"), start("Two")]);
			expect(replies.map((r) => r.text.split(",")[0])).toEqual([
				"Started demo/One on branch feature/one",
				"Started demo/Two on branch feature/two",
			]);
			expect(server.herdr.workspaceList.filter((w) => w.label === "demo")).toHaveLength(1);
			expect(server.herdr.of("createWorktree")).toHaveLength(2);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a gh that never answers: thread_finish pr answers within its bound, nothing changes, and the service still stops (CO-8)",
	async () => {
		await demoProject(env.root);
		const ghState = join(env.root, "gh-state.json");
		writeFileSync(ghState, JSON.stringify({ hang: true }));
		const gh = [process.execPath, join(import.meta.dir, "fake-gh.ts"), ghState];
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				const reply = await call(server, "thread_finish", { thread: "Readme line", how: "pr" });
				expect(reply).toEqual({ text: "gh didn't answer in a minute; nothing was changed.", isError: true });
				expect(server.herdr.of("prompt").some(([, t]) => String(t).includes("gh pr create"))).toBe(false);
			},
			{ agentWaitMs: 10_000, gh, ghTimeoutMs: 500 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"panes herdr restored as shells while the service runs get omp again with their flags: the boss with boss.yml, the thread and the project pane with their overlay (0431)",
	async () => {
		const project = await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				// The refresh tick notes each new pane's terminal.
				await server.service.refresh();
				herdr.restore();
				const before = herdr.calls.length;
				await server.service.refresh();
				const respawned = herdr.calls
					.slice(before)
					.filter((c) => c.method === "respawn")
					.map((c) => c.args) as [string, { cwd: string; command: string[]; env?: Record<string, string> }][];
				const overlay = ["omp", "--continue", ...GUARD, "--config", overlayFile(project.id)];
				const thread = threadOmp(project.id);
				expect(respawned).toHaveLength(3);
				expect(respawned[0]?.[1].command.slice(0, 3)).toEqual(["omp", "--continue", "--config"]);
				expect(respawned[0]?.[1].command[3]).toEndWith("boss.yml");
				expect(respawned[0]?.[1].cwd).toEndWith("home");
				expect(respawned.slice(1).map(([, o]) => ({ ...o, cwd: resolve(o.cwd) }))).toEqual([
					{ cwd: path, command: thread, env: keyed({ ...hunkEnv(HUNK_OPTIONS.hunkPort), ...README_THREAD_ENV }) },
					{ cwd: project.mainPath, command: overlay, env: keyed(hunkEnv(HUNK_OPTIONS.hunkPort)) },
				]);
				// The thread is known by its new pane.
				const fresh = herdr.paneList.find((p) => resolve(p.cwd ?? "") === path)?.paneId as string;
				expect((await call(server, "thread_status", {})).text).toContain("demo/Readme line");
				expect(fresh).toBeDefined();
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

/** The project workspace's pane: the project's own omp in its main checkout. */
function projectPane(herdr: FakeHerdr): string {
	const workspace = herdr.workspaceList.find((w) => w.label === "demo")?.id;
	return herdr.paneList.find((p) => p.workspaceId === workspace)?.paneId as string;
}

test(
	"an omp that ends in a Jocasta pane left open starts again there, resuming its session: the boss, the thread and the project pane, once per ending (0521)",
	async () => {
		const project = await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const boss = bossPane(herdr) as string;
				const own = projectPane(herdr);
				herdr.setState(boss, "idle");
				herdr.setState(own, "idle");
				// The refresh tick sees omp running in each pane's terminal.
				await server.service.refresh();
				for (const id of [boss, paneId, own]) herdr.setState(id, "unknown", undefined, false);
				const before = herdr.calls.length;
				await server.service.refresh();
				const respawned = herdr.calls
					.slice(before)
					.filter((c) => c.method === "respawn")
					.map((c) => c.args) as [string, { cwd: string; command: string[]; env?: Record<string, string> }][];
				expect(respawned.map(([id]) => id)).toEqual([boss, paneId, own]);
				expect(respawned[0]?.[1].command.slice(0, 3)).toEqual(["omp", "--continue", "--config"]);
				expect(respawned[0]?.[1].command[3]).toEndWith("boss.yml");
				expect(respawned[0]?.[1].env).toEqual(keyed({ JOCASTA_BOSS_KEY: TEST_BOSS_KEY }));
				const overlay = ["omp", "--continue", ...GUARD, "--config", overlayFile(project.id)];
				const thread = threadOmp(project.id);
				expect(respawned.slice(1).map(([, o]) => ({ ...o, cwd: resolve(o.cwd) }))).toEqual([
					{ cwd: path, command: thread, env: keyed({ ...hunkEnv(HUNK_OPTIONS.hunkPort), ...README_THREAD_ENV }) },
					{ cwd: project.mainPath, command: overlay, env: keyed(hunkEnv(HUNK_OPTIONS.hunkPort)) },
				]);
				// The new omps are still starting: one ending started each once.
				await server.service.refresh();
				await server.service.refresh();
				expect(herdr.calls.slice(before).filter((c) => c.method === "respawn")).toHaveLength(3);
				// The thread goes on in its new pane.
				expect((await call(server, "thread_status", {})).text).toContain("demo/Readme line");
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a Jocasta pane whose omp hasn't been seen running is left alone: the project pane before its omp reports (0521)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				await startThread(server);
				const before = herdr.calls.length;
				await server.service.refresh();
				await server.service.refresh();
				expect(herdr.paneList.find((p) => p.paneId === projectPane(herdr))?.agentState).toBe("unknown");
				expect(herdr.calls.slice(before).filter((c) => c.method === "respawn")).toEqual([]);
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread being finished: its omp ending as its workspace closes isn't started again (0521)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				server.herdr.setState(bossPane(server.herdr) as string, "idle");
				await server.service.refresh();
				commitIn(path, "demo\nA line.\n", "Add a README line");
				// herdr stops the workspace's terminals: omp ends while its pane is still listed, and a refresh runs.
				const close = server.herdr.closeWorkspace.bind(server.herdr);
				server.herdr.closeWorkspace = async (workspaceId) => {
					server.herdr.setState(paneId, "unknown", undefined, false);
					await server.service.refresh();
					await close(workspaceId);
				};
				const reply = await call(server, "thread_finish", { thread: "Readme line", how: "merge" });
				expect(reply.text).toContain("its worktree, workspace and branch are removed");
				await server.service.refresh();
				expect(server.herdr.of("respawn")).toEqual([]);
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"an omp that ends once the service stopped isn't started again, by it or by the next start (0521)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		const options = { herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS };
		const first = await startTestServer(options);
		let thread = "";
		try {
			thread = (await startThread(first)).paneId;
			await first.service.refresh();
		} finally {
			await first.stop();
		}
		herdr.setState(thread, "unknown", undefined, false);
		await first.service.refresh();
		expect(herdr.of("respawn")).toEqual([]);
		await withServer(async (server) => {
			await server.service.refresh();
			await server.service.refresh();
			expect(herdr.of("respawn").filter(([id]) => id === thread)).toEqual([]);
			expect((await call(server, "thread_status", {})).text).toContain("demo/Readme line");
		}, options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"an omp that keeps ending in its pane starts again after a wait that doubles, said once in Jocasta's log; one that ran 10 minutes starts again at once (0521)",
	async () => {
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				/** omp comes up in the boss pane, a refresh sees it, then it ends and a refresh runs. */
				const ends = async (): Promise<string> => {
					const boss = bossPane(herdr) as string;
					herdr.setState(boss, "idle", undefined, false);
					await server.service.refresh();
					herdr.setState(boss, "unknown", undefined, false);
					await server.service.refresh();
					return boss;
				};
				const starts = () => herdr.of("respawn").length;
				const before = starts();
				// The clock stands still between the steps, so each wait is exact.
				const t = Date.now();
				const at = (ms: number) => setSystemTime(new Date(t + ms));
				try {
					at(0);
					// The first ending: at once.
					await ends();
					expect(starts()).toBe(before + 1);
					// It ended again: it waits 10 s, said once in the log.
					const held = await ends();
					await server.service.refresh();
					expect(starts()).toBe(before + 1);
					const dir = logsDir();
					const log = readdirSync(dir)
						.map((name) => readFileSync(join(dir, name), "utf8"))
						.join("");
					expect(log.split(`omp ended in pane ${held}`)).toHaveLength(2);
					at(10_000);
					await server.service.refresh();
					expect(starts()).toBe(before + 2);
					// Once more: the wait is 20 s now.
					await ends();
					at(20_000);
					await server.service.refresh();
					expect(starts()).toBe(before + 2);
					at(30_000);
					await server.service.refresh();
					expect(starts()).toBe(before + 3);
					// It ran 10 minutes before it ended: at once again.
					at(30_000 + 10 * 60_000);
					await ends();
					expect(starts()).toBe(before + 4);
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
	"R3: a working thread whose pane herdr restored as a shell is relaunched; its new omp comes up idle without an event, and the close question counts 0 working (0438)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				herdr.setState(paneId, "working");
				await until(() => server.service.busyThreads().working === 1);
				await server.service.refresh();
				herdr.restore();
				await server.service.refresh();
				const fresh = herdr.paneList.find((p) => resolve(p.cwd ?? "") === path)?.paneId as string;
				expect(fresh).not.toBe(paneId);
				// The new omp sits idle; herdr pushes no change for it, its pane list says idle.
				herdr.setState(fresh, "idle", undefined, false);
				await server.service.refresh();
				await until(() => server.service.busyThreads().working === 0);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 0 });
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);
