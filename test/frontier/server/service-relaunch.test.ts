import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pidAlive, processesIn } from "../../../src/shared/proctree.ts";
import { GIT_TEST_TIMEOUT, poll, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import {
	bossPane,
	call,
	commitIn,
	demoProject,
	handoffEntry,
	proposedPlanFile,
	relaunchIdle,
	review,
	sessionFile,
	startThread,
	toHandoff,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";
import { killAll, settles, sleeperTree } from "./sleepers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

/** A thread pane's omp end waited for briefly, and its leftovers looked for again soon (0490): the sleepers never exit. */
const LEFTOVERS = { leftoverWaitMs: 500, leftoverAgainMs: 200 };

test(
	"R3: a thread blocked on a question, relaunched, no longer awaits him: --continue doesn't resume the turn that asked (0438)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const asked = sessionFile(path, [{ text: "Which format?", cost: 0.1, ask: "CSV or JSON?" }]);
				herdr.setState(paneId, "working", asked);
				herdr.setState(paneId, "blocked");
				await until(() => server.service.busyThreads().awaiting === 1);
				await relaunchIdle(server, herdr, path, asked);
				await until(() => server.service.busyThreads().awaiting === 0);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 0 });
				expect(server.service.hud().awaiting).toBe(0);
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"R3: a thread whose plan awaits him, relaunched, still awaits him (0438, 0494)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const file = proposedPlanFile(path);
				herdr.setState(paneId, "working", file);
				herdr.setState(paneId, "idle");
				await until(() => server.service.busyThreads().awaiting === 1);
				await relaunchIdle(server, herdr, path, file);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });
				expect(server.service.hud().awaiting).toBe(1);
				expect((await call(server, "thread_status", {})).text).toBe(
					"demo/Readme line [feature/readme-line] plan ready\n  spend $0.25",
				);
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread pane herdr restored as a shell: its old omp, still running, and what it left end before omp starts again; a process something else started in its worktree keeps running (0490)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				// The old omp with its workers (real processes that outlive it), working in the worktree.
				const tree = await sleeperTree(join(path, "pane"), true);
				// He started it in the worktree from another terminal: a real process that stays up until killed.
				const other = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
					cwd: path,
					stdio: ["ignore", "ignore", "ignore"],
					windowsHide: true,
				});
				try {
					herdr.roots.set(paneId, tree.root);
					// The refresh tick notes the pane's terminal and reads its omp.
					await server.service.refresh();
					herdr.restore();
					let aliveAtRespawn: number[] | undefined;
					const respawn = herdr.respawn.bind(herdr);
					herdr.respawn = async (id, opts) => {
						if (id === paneId) aliveAtRespawn = tree.all.filter(pidAlive);
						return respawn(id, opts);
					};
					await server.service.refresh();
					expect(aliveAtRespawn).toEqual([]);
					expect(herdr.of("respawn").some(([id]) => id === paneId)).toBe(true);
					expect(pidAlive(other.pid)).toBe(true);
					expect((await call(server, "thread_status", {})).text).toContain("demo/Readme line");
				} finally {
					await tree.stop();
					killAll([other.pid]);
					await other.exited;
				}
			},
			{ herdr, agentWaitMs: 10_000, ...HUNK_OPTIONS, ...LEFTOVERS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"Stop everything ends what each thread pane's remembered omp left, also when herdr's server didn't; a process something else started in its worktree keeps running (0486, 0490)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				// The omp with its workers (real processes that outlive it), from a herdr server that no longer has them.
				const tree = await sleeperTree(join(path, "pane"), true);
				// He started it in the worktree from another terminal: a real process that stays up until killed.
				const other = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
					cwd: path,
					stdio: ["ignore", "ignore", "ignore"],
					windowsHide: true,
				});
				try {
					server.herdr.roots.set(paneId, tree.root);
					await server.service.refresh();
					// This fake herdr's stop ends no process.
					expect(await server.shutdown({ ask: false })).toBe("stopped");
					expect(tree.all.filter(pidAlive)).toEqual([]);
					expect(pidAlive(other.pid)).toBe(true);
				} finally {
					await tree.stop();
					killAll([other.pid]);
					await other.exited;
				}
			},
			{ agentWaitMs: 10_000, ...LEFTOVERS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"Stop everything: a thread in its review turn is saved for --restore and its omp ends, forced, before herdr's server stops; the boss's ends with herdr; only the ended omps' host entries go (0549, 0547)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				// The thread's and the boss's omps: real processes that outlive their roots, as omp's workers do.
				const omp = await sleeperTree(join(env.root, "thread-omp"), true);
				const boss = await sleeperTree(join(env.root, "boss-omp"), true);
				const ended = Bun.spawn([process.execPath, "-e", ""], {
					stdio: ["ignore", "ignore", "ignore"],
					windowsHide: true,
				});
				await ended.exited;
				try {
					server.herdr.roots.set(paneId, omp.root);
					server.herdr.roots.set(bossPane(server.herdr) as string, boss.root);
					await server.service.refresh();
					// Its task ends; its review turn runs.
					server.herdr.setState(paneId, "working");
					commitIn(path, "demo\nA line.\n", "Add a README line");
					server.herdr.setState(paneId, "idle");
					await until(() =>
						server.herdr.of("prompt").some(([p, t]) => p === paneId && t === review("main...feature/readme-line")),
					);
					const file = sessionFile(path, [{ text: "Reviewing.", cost: 0.1 }]);
					server.herdr.setState(paneId, "working", file);
					await until(() => server.service.busyThreads().working === 1);
					// omp's host registry: the thread's omp, the boss's, and another omp that already ended.
					const hosts = join(env.data, "collab-hosts");
					mkdirSync(hosts, { recursive: true });
					for (const [name, pid] of [
						["thread", omp.root],
						["boss", boss.root],
						["other", ended.pid],
					] as const) {
						writeFileSync(
							join(hosts, `${name}.json`),
							JSON.stringify({ version: 1, instanceId: name, pid, endpoint: `none-${name}`, createdAt: 0, token: "t" }),
						);
					}
					// What runs as herdr's server stops (this fake's stop ends no process).
					let atStop: { thread: number[]; boss: number[] } | undefined;
					const stopServer = server.herdr.stopServer.bind(server.herdr);
					server.herdr.stopServer = async () => {
						atStop = { thread: omp.all.filter(pidAlive), boss: boss.all.filter(pidAlive) };
						await stopServer();
					};
					expect(await server.shutdown({ ask: false })).toBe("stopped");
					expect(atStop).toEqual({ thread: [], boss: boss.all });
					const panes = JSON.parse(readFileSync(join(env.data, "panes.json"), "utf8")) as Record<
						string,
						{ restore?: string }
					>;
					expect(panes[paneId]?.restore).toBe(file);
					expect(readdirSync(hosts).toSorted()).toEqual(["boss.json", "other.json"]);
				} finally {
					await omp.stop();
					await boss.stop();
					// Nothing it started still works in the temp folder, so the folder can go (Windows' EBUSY).
					await settles(() => processesIn(env.root).length === 0);
				}
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS, ...LEFTOVERS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_finish merge after main moved: main is merged into the branch, tested and reviewed again, and merged only when that passes (0453, 0454)",
	async () => {
		const project = await demoProject(env.root);
		const main = project.mainPath;
		sh(main, "config", "user.name", "Test");
		sh(main, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				const { paneId, path } = await startThread(server);
				handoffEntry(await toHandoff(server, paneId, path));
				const prompts = () => server.herdr.of("prompt");
				const range = "main...feature/readme-line";
				// The boss answered its last line and is idle: lines waiting for it go in.
				const bossIdle = () => {
					server.herdr.setState(boss, "working");
					server.herdr.setState(boss, "idle");
				};
				// The session is read again once the handoff entry is seen (a read under a second ago is reused).
				const finish = async () => {
					let reply = { text: "" } as { text: string; isError?: boolean };
					await poll(
						async () => {
							reply = await call(server, "thread_finish", { thread: "Readme line", how: "merge" });
							return !reply.text.includes("still writing its handoff");
						},
						{ timeoutMs: 15_000, intervalMs: 100, what: "thread_finish past the handoff" },
					);
					return reply;
				};
				// Another thread's work landed on main meanwhile.
				writeFileSync(join(main, "other.txt"), "other\n");
				sh(main, "add", "other.txt");
				sh(main, "commit", "-qm", "Other work");
				expect((await finish()).text).toBe(
					"main moved since demo/Readme line's review: it was merged into feature/readme-line, which is being tested and reviewed again; it merges into main only when that passes, and you hear either way.",
				);
				expect(existsSync(join(path, "other.txt"))).toBe(true);
				await until(() => prompts().filter(([p, t]) => p === paneId && t === review(range)).length === 2);
				expect(sh(main, "log", "-1", "--format=%s")).toBe("Other work");

				// The new review's tests fail by their run: nothing is merged, he hears it with why, in one line (P3).
				const failed = sessionFile(path, [
					{ text: "Reviewed.\nTESTS: pass — bun test", cost: 0, bash: { command: "bun test", ok: false } },
				]);
				server.herdr.setState(paneId, "working", failed);
				server.herdr.setState(paneId, "idle");
				const notMerged =
					/^\[event\] demo\/Readme line — done and reviewed: main merged in, .*; not merged: its new review after main moved didn't pass \(tests fail\)$/m;
				await until(() => {
					bossIdle();
					return prompts().some(([p, t]) => p === boss && notMerged.test(String(t)));
				});
				expect(sh(main, "log", "-1", "--format=%s")).toBe("Other work");
				expect(existsSync(path)).toBe(true);

				// main moves again; this time the new review passes: the squash follows, as an event.
				handoffEntry(failed);
				await server.service.refresh();
				writeFileSync(join(main, "third.txt"), "third\n");
				sh(main, "add", "third.txt");
				sh(main, "commit", "-qm", "Third work");
				expect((await finish()).text).toStartWith("main moved since demo/Readme line's review");
				await until(() => prompts().filter(([p, t]) => p === paneId && t === review(range)).length === 3);
				const passed = sessionFile(path, [
					{ text: "Reviewed.\nTESTS: pass — bun test", cost: 0, bash: { command: "bun test", ok: true } },
				]);
				server.herdr.setState(paneId, "working", passed);
				server.herdr.setState(paneId, "idle");
				// One event for the merge: the review's outcome and the merge together, no separate "done and reviewed" (P3).
				const merged =
					/^\[event\] demo\/Readme line — its new review passed \(main merged in, .*tests pass, no report\); merged demo\/Readme line into main as [0-9a-f]+/m;
				await until(() => {
					bossIdle();
					return prompts().some(([p, t]) => p === boss && merged.test(String(t)));
				});
				const reviewedLines = prompts()
					.filter(([p]) => p === boss)
					.flatMap(([, t]) => String(t).split("\n"))
					.filter((line) => line.includes("done and reviewed: main merged in"));
				expect(reviewedLines).toHaveLength(1);
				expect(sh(main, "log", "-1", "--format=%s")).toBe("Readme line");
				expect(readFileSync(join(main, "README.md"), "utf8").replaceAll("\r\n", "\n")).toBe("demo\nA line.\n");
				expect(existsSync(path)).toBe(false);
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_start copies the .worktreeinclude files that are gitignored into the new worktree (0457)",
	async () => {
		const project = await demoProject(env.root);
		const main = project.mainPath;
		writeFileSync(join(main, ".gitignore"), ".env\nlocal/\n");
		writeFileSync(join(main, ".worktreeinclude"), ".env\nlocal/\nnotes.txt\n");
		sh(main, "add", ".gitignore", ".worktreeinclude");
		sh(main, "commit", "-qm", "ignore");
		writeFileSync(join(main, ".env"), "SECRET=1\n");
		mkdirSync(join(main, "local"));
		writeFileSync(join(main, "local", "settings.json"), "{}\n");
		// Matched but not gitignored: never copied.
		writeFileSync(join(main, "notes.txt"), "mine\n");
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				expect(readFileSync(join(path, ".env"), "utf8")).toBe("SECRET=1\n");
				expect(readFileSync(join(path, "local", "settings.json"), "utf8")).toBe("{}\n");
				expect(existsSync(join(path, "notes.txt"))).toBe(false);
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_status names open threads touching the same files; projects lists leftover worktrees, removed only by worktree_remove and never when dirty (0456)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const first = await startThread(server);
				const second = await startThread(server, { title: "Second", brief: "More." });
				commitIn(first.path, "demo\nOne.\n", "One");
				writeFileSync(join(second.path, "README.md"), "demo\nTwo.\n");
				const status = (await call(server, "thread_status", {})).text;
				expect(status).toContain("demo/Readme line [feature/readme-line] idle · overlaps Second: README.md");
				expect(status).toContain("demo/Second [feature/second] idle · overlaps Readme line: README.md");

				// The second thread is closed in herdr: its worktree is left on disk, with its change.
				const ws = server.herdr.paneList.find((p) => p.paneId === second.paneId)?.workspaceId as string;
				await server.herdr.closeWorkspace(ws);
				await server.service.refresh();
				const leftover = join(env.root, "demo.wt", "second");
				expect((await call(server, "projects", {})).text).toContain(
					`  left on disk: ${leftover} [feature/second] 0 ahead, 1 changed`,
				);
				expect(await server.service.leftovers()).toEqual([
					{ project: "demo", folder: leftover, branch: "feature/second", ahead: 0, dirty: 1 },
				]);
				expect(await call(server, "worktree_remove", { path: leftover })).toEqual({
					text: `${leftover} has 1 uncommitted change; nothing removed.`,
					isError: true,
				});
				expect(await call(server, "worktree_remove", { path: first.path })).toEqual({
					text: `${first.path} isn't a leftover worktree (a thread folder no open thread holds); nothing removed.`,
					isError: true,
				});
				sh(leftover, "checkout", "--", "README.md");
				expect((await call(server, "worktree_remove", { path: leftover })).text).toBe(
					`Removed ${leftover}; branch feature/second stays (0 ahead).`,
				);
				expect(existsSync(leftover)).toBe(false);
				expect(sh(project.mainPath, "branch", "--list", "feature/second")).toContain("feature/second");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);
