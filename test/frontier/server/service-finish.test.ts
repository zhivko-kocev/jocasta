import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { startServer } from "../../../src/frontier/server/server.ts";
import { HANDOFF_PROMPT, quoted } from "../../../src/frontier/server/wording.ts";
import { worklogPath } from "../../../src/frontier/server/worklog.ts";
import { GIT_TEST_TIMEOUT, poll, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import {
	bossPane,
	call,
	commitIn,
	demoProject,
	Page,
	relayTestOptions,
	review,
	startTestServer,
	startThread,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"thread_finish merge: refuses a dirty or off-trunk main checkout and conflicts, leaving all in place; then squash-merges, removes the worktree and workspace and deletes the branch (0426)",
	async () => {
		const project = await demoProject(env.root);
		const main = project.mainPath;
		sh(main, "config", "user.name", "Test");
		sh(main, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const finish = (how = "merge") => call(server, "thread_finish", { thread: "Readme line", how });
				expect(await finish()).toEqual({
					text: "demo/Readme line has no commits ahead of main; nothing to finish.",
					isError: true,
				});
				// A question he dismissed ends the turn (CO-3); with no commits yet it was just a turn.
				server.herdr.setState(paneId, "blocked");
				expect((await finish()).text).toBe("demo/Readme line isn't idle (blocked); finish it once it is.");
				server.herdr.setState(paneId, "idle");
				await until(() => server.service.hud().blocked === 0);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				commitIn(path, "demo\nA line.\nAnother.\n", "Another line");
				expect((await finish("rebase")).text).toBe('thread_finish: how is "merge" or "pr".');

				writeFileSync(join(main, "README.md"), "edited by hand\n");
				expect(await finish()).toEqual({
					text: "the main checkout of demo has uncommitted changes; nothing was merged.",
					isError: true,
				});
				sh(main, "checkout", "--", "README.md");
				sh(main, "checkout", "-q", "-b", "elsewhere");
				expect((await finish()).text).toBe("the main checkout of demo is on elsewhere, not main; nothing was merged.");
				sh(main, "checkout", "-q", "main");

				// trunk moved on with a clashing line: bringing it into the branch conflicts (0454); aborted, nothing changes.
				commitIn(main, "demo\nClash.\n", "Clash");
				const trunkHead = sh(main, "rev-parse", "HEAD");
				const branchHead = sh(path, "rev-parse", "HEAD");
				expect(await finish()).toEqual({
					text: "main moved since demo/Readme line's review, and bringing it into feature/readme-line conflicts in README.md; that merge was aborted and everything stays.",
					isError: true,
				});
				expect(sh(main, "rev-parse", "HEAD")).toBe(trunkHead);
				expect(sh(path, "rev-parse", "HEAD")).toBe(branchHead);
				expect(sh(main, "status", "--porcelain")).toBe("");
				expect(sh(path, "status", "--porcelain")).toBe("");
				expect(existsSync(path)).toBe(true);
				expect(server.herdr.of("closeWorkspace")).toEqual([]);
				sh(main, "reset", "-q", "--hard", "HEAD~1");

				const reply = await finish();
				expect(reply.isError).toBeUndefined();
				expect(reply.text).toMatch(
					/^Merged demo\/Readme line into main as [0-9a-f]+ \(2 commits squashed\); its worktree, workspace and branch are removed\.$/,
				);
				expect(readFileSync(join(main, "README.md"), "utf8").replaceAll("\r\n", "\n")).toBe(
					"demo\nA line.\nAnother.\n",
				);
				expect(sh(main, "log", "-1", "--format=%s%n%b")).toBe(
					"Readme line\nSquash-merged from feature/readme-line:\n- Add a README line\n- Another line",
				);
				expect(sh(main, "rev-list", "--count", "HEAD")).toBe("2");
				expect(existsSync(path)).toBe(false);
				expect(sh(main, "branch", "--list", "feature/readme-line")).toBe("");
				expect(server.herdr.of("closeWorkspace")).toHaveLength(1);
				expect((await call(server, "thread_status", {})).text).toBe("No threads.");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_finish merge: a pre-commit hook refusing the squash commit merges nothing and leaves his main checkout as it was; herdr failing to close the workspace after the merge keeps the thread (TST-11, CO-10)",
	async () => {
		const project = await demoProject(env.root);
		const main = project.mainPath;
		sh(main, "config", "user.name", "Test");
		sh(main, "config", "user.email", "test@example.com");
		await withServer(async (server) => {
			const { path } = await startThread(server);
			commitIn(path, "demo\nA line.\n", "Add a README line");
			// The repo's own hook, outside the checkout so main stays clean: it refuses every commit from now on.
			const hooks = join(env.root, "hooks");
			mkdirSync(hooks, { recursive: true });
			writeFileSync(join(hooks, "pre-commit"), "#!/bin/sh\necho 'lint failed' >&2\nexit 1\n", { mode: 0o755 });
			sh(main, "config", "core.hooksPath", hooks);
			const trunkHead = sh(main, "rev-parse", "HEAD");
			const finish = () => call(server, "thread_finish", { thread: "Readme line", how: "merge" });
			expect(await finish()).toEqual({
				text: "demo/Readme line: the squash commit failed (lint failed); nothing was merged.",
				isError: true,
			});
			expect(sh(main, "rev-parse", "HEAD")).toBe(trunkHead);
			expect(sh(main, "status", "--porcelain")).toBe("");
			expect(existsSync(path)).toBe(true);
			expect(sh(main, "branch", "--list", "feature/readme-line")).toContain("feature/readme-line");
			expect(server.herdr.of("closeWorkspace")).toEqual([]);
			expect((await call(server, "thread_status", {})).text).toStartWith("demo/Readme line [feature/readme-line]");

			// The hook lets it through; herdr then fails to close the workspace: the commit has landed, the rest stays.
			sh(main, "config", "--unset", "core.hooksPath");
			server.herdr.closeWorkspace = async () => {
				throw new Error("herdr is gone");
			};
			const reply = await finish();
			expect(reply.text).toMatch(
				/^Merged demo\/Readme line into main as [0-9a-f]+ \(1 commit squashed\), but herdr couldn't close its workspace \(herdr is gone\); its worktree, workspace and branch stay\.$/,
			);
			expect(sh(main, "rev-list", "--count", "HEAD")).toBe("2");
			expect(existsSync(path)).toBe(true);
			expect(sh(main, "branch", "--list", "feature/readme-line")).toContain("feature/readme-line");
			expect((await call(server, "thread_status", {})).text).toStartWith("demo/Readme line [feature/readme-line]");
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_finish pr: gh missing or logged out is a clear refusal; else the thread pushes and opens the PR, and an open PR on its last commit removes the worktree (0426)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "remote", "add", "origin", "https://github.com/test/demo.git");
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				const reply = await call(server, "thread_finish", { thread: "Readme line", how: "pr" });
				expect(reply).toEqual({
					text: "gh isn't installed: install GitHub CLI (https://cli.github.com), then gh auth login.",
					isError: true,
				});
			},
			{ agentWaitMs: 10_000, gh: ["jocasta-test-no-such-gh"] },
		);
		sh(project.mainPath, "worktree", "remove", "--force", join(env.root, "demo.wt", "readme-line"));
		sh(project.mainPath, "branch", "-D", "feature/readme-line");

		const ghState = join(env.root, "gh-state.json");
		const gh = [process.execPath, join(import.meta.dir, "fake-gh.ts"), ghState];
		const ghIs = (state: { auth?: string; head?: string }) => writeFileSync(ghState, JSON.stringify(state));
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				server.herdr.setState(boss, "idle");
				commitIn(path, "demo\nA line.\n", "Add a README line");
				const finish = () => call(server, "thread_finish", { thread: "Readme line", how: "pr" });
				ghIs({ auth: "fail" });
				expect((await finish()).text).toBe(
					"gh isn't logged in (gh auth status: You are not logged into any GitHub hosts. To log in, run: gh auth login); he runs gh auth login.",
				);
				ghIs({});
				expect((await finish()).text).toBe(
					"Asked demo/Readme line to push feature/readme-line and open a PR against main; once it's open, its worktree and workspace are removed and you hear it.",
				);
				const asked = server.herdr.of("prompt").at(-1) as [string, string];
				expect(asked[0]).toBe(paneId);
				expect(asked[1]).toContain("git push -u origin feature/readme-line");
				expect(asked[1]).toContain("gh pr create --base main --head feature/readme-line");

				// The PR turn ends without the branch's last commit on GitHub: everything stays.
				ghIs({ head: "0000000" });
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				const notPushed =
					"[event] demo/Readme line — no PR: its PR doesn't have the branch's last commit (not pushed?); everything stays";
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === boss && t === notPushed));
				expect(server.herdr.of("closeWorkspace")).toEqual([]);
				// The boss answers that line and is idle again.
				server.herdr.setState(boss, "idle");

				// Asked again; this time the PR has it: the worktree and workspace go, the branch stays.
				await finish();
				ghIs({ head: sh(path, "rev-parse", "HEAD") });
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				const opened =
					"[event] demo/Readme line — PR open; its worktree and workspace are removed, the branch stays · https://github.com/test/demo/pull/7";
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === boss && t === opened));
				expect(existsSync(path)).toBe(false);
				expect(sh(project.mainPath, "branch", "--list", "feature/readme-line")).toContain("feature/readme-line");
			},
			{ agentWaitMs: 10_000, gh },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"after a service restart the next task reuses the thread's hunk pane; a hunk pane or a plain shell in a worktree is never a thread (CR-01, CR-02, F-13)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		const range = "main...feature/readme-line";
		let server = await startTestServer({ herdr, ...HUNK_OPTIONS, agentWaitMs: 10_000 });
		let paneId = "";
		let path = "";
		try {
			({ paneId, path } = await startThread(server));
			herdr.setState(paneId, "working");
			commitIn(path, "demo\nA line.\n", "Add a README line");
			herdr.setState(paneId, "idle");
			await until(() => herdr.of("splitBeside").length === 1);
			herdr.setState(paneId, "working");
			herdr.setState(paneId, "idle");
			await until(() => herdr.of("prompt").some(([, t]) => t === HANDOFF_PROMPT));
		} finally {
			await server.stop();
		}
		// He opens a shell beside it, in another tab of the thread's workspace.
		const workspaceId = herdr.paneList.find((p) => p.paneId === paneId)?.workspaceId as string;
		const shell = await herdr.createTab(workspaceId, { title: "shell", cwd: path, command: ["pwsh"] });
		server = await startTestServer({ herdr, ...HUNK_OPTIONS });
		try {
			const status = async () => (await call(server, "thread_status", {})).text;
			expect((await status()).split("\n")).toHaveLength(1);
			// The next task: the same hunk pane, no second one.
			herdr.setState(paneId, "working");
			commitIn(path, "demo\nA line.\nAnother.\n", "Another line");
			herdr.setState(paneId, "idle");
			await until(() => herdr.of("prompt").filter(([, t]) => t === review(range)).length === 2);
			expect(herdr.of("splitBeside")).toHaveLength(1);

			// Its omp pane closes: the hunk pane and the shell left in the workspace aren't adopted as a thread.
			herdr.paneList.splice(
				herdr.paneList.findIndex((p) => p.paneId === paneId),
				1,
			);
			await server.service.refresh();
			await server.service.refresh();
			expect(await status()).toBe("No threads.");
			expect(herdr.paneList.some((p) => p.paneId === shell.paneId)).toBe(true);
			expect(herdr.of("reportPaneMetadata").some(([p]) => p === shell.paneId)).toBe(false);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"events raised together reach a busy boss as one prompt after its turn, not back to back (CR-08)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const first = await startThread(server);
				const second = await startThread(server, { title: "Two", brief: "More." });
				expect(second.reply).toStartWith("Started demo/Two");
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				const bossPrompts = () => server.herdr.of("prompt").filter(([p]) => p === boss);
				// Both threads' workspaces close in one refresh.
				for (const id of new Set(server.herdr.paneList.filter((p) => p.paneId !== boss).map((p) => p.workspaceId))) {
					if (server.herdr.workspaceList.find((w) => w.id === id)?.label !== "demo") {
						await server.herdr.closeWorkspace(id);
					}
				}
				expect(first.paneId).toBeDefined();
				await server.service.refresh();
				expect(bossPrompts()).toHaveLength(1);
				expect(bossPrompts()[0]?.[1]).toContain("closed");
				// The boss's turn on the first line ends: the second follows.
				server.herdr.setState(boss, "working");
				server.herdr.setState(boss, "idle");
				await until(() => bossPrompts().length === 2);
				expect(bossPrompts()[1]?.[1]).toContain("closed");
			},
			{ agentWaitMs: 10_000, submitWaitMs: 0 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a task end whose idle event herdr lost is caught on the refresh tick; one pushed after the read isn't undone (CR-09)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				server.herdr.setState(paneId, "working");
				await until(() => server.service.hud().running === 1);
				commitIn(path, "demo\nA line.\n", "Add a README line");
				// herdr's idle event never arrives.
				server.herdr.setState(paneId, "idle", undefined, false);
				await server.service.refresh();
				await until(() => server.herdr.of("splitBeside").length === 1);
				expect(server.herdr.of("prompt").filter(([, t]) => t === review("main...feature/readme-line"))).toHaveLength(1);
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_finish merge: the workspace closes first, the worktree goes once the shells holding it exit, and a refresh meanwhile neither finds the thread again nor reports it closed (F-02, CR-11, F-13)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				commitIn(path, "demo\nA line.\n", "Add a README line");
				// As herdr does on Windows, closing leaves a process in the worktree for a moment (it locks the folder there);
				// a refresh runs in the middle.
				const close = server.herdr.closeWorkspace.bind(server.herdr);
				let holder: Bun.Subprocess | undefined;
				server.herdr.closeWorkspace = async (workspaceId) => {
					holder = Bun.spawn([process.execPath, "-e", "await Bun.sleep(1500)"], {
						cwd: path,
						stdout: "ignore",
						windowsHide: true,
					});
					await server.service.refresh();
					await close(workspaceId);
				};
				const reply = await call(server, "thread_finish", { thread: "Readme line", how: "merge" });
				await holder?.exited;
				expect(reply.text).toContain("its worktree, workspace and branch are removed");
				expect(existsSync(path)).toBe(false);
				expect(sh(project.mainPath, "branch", "--list", "feature/readme-line")).toBe("");
				await server.service.refresh();
				await server.service.refresh();
				expect(server.herdr.of("prompt").some(([, t]) => String(t).includes("closed"))).toBe(false);
				expect((await call(server, "thread_status", {})).text).toBe("No threads.");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"thread_finish merge removes the worktree only once no process works in it any more (0486)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				const { path } = await startThread(server);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				commitIn(path, "demo\nA line.\n", "Add a README line");
				// A process outside the pane's tree (he opened it, or it outlived an earlier pane) works in a subfolder.
				mkdirSync(join(path, "sub"));
				const holder = Bun.spawn([process.execPath, "-e", "await Bun.sleep(2000)"], {
					cwd: join(path, "sub"),
					stdout: "ignore",
					windowsHide: true,
				});
				let exited = false;
				const gone = holder.exited.then(() => {
					exited = true;
				});
				const finishing = call(server, "thread_finish", { thread: "Readme line", how: "merge" });
				// Real time: the holder is a real process; the folder stays for as long as it runs.
				await poll(
					() => {
						expect(existsSync(path)).toBe(true);
						return exited;
					},
					{ timeoutMs: 10_000, intervalMs: 50, what: "the holder to exit" },
				);
				await gone;
				expect((await finishing).text).toContain("its worktree, workspace and branch are removed");
				expect(existsSync(path)).toBe(false);
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a review whose notes hunk can't count says so instead of hiding it (F-03)",
	async () => {
		await demoProject(env.root);
		const broken = [process.execPath, join(import.meta.dir, "no-such-hunk.ts")];
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				server.herdr.setState(paneId, "working");
				commitIn(path, "demo\nA line.\n", "Add a README line");
				server.herdr.setState(paneId, "idle");
				await until(() => server.herdr.of("splitBeside").length === 1);
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				const line =
					"[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, notes not counted, tests unverified, no report";
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === boss && t === line));
			},
			{ agentWaitMs: 10_000, hunk: broken, hunkPort: HUNK_OPTIONS.hunkPort },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"no control characters reach the feed; a reminder not yet due doesn't await him; inbox_done takes the id as digits (N-3, N-5, F-11)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			const page = await Page.open(server);
			await page.waitFor((m) => m.type === "snapshot");

			const filed = await call(server, "note", { text: "Tidy\u001b[2J the docs\u0007.", project: "demo" });
			expect(filed.text).not.toContain("\u001b");
			await page.waitFor((m) => m.type === "today" && m.today.next.some((n) => n.text === "Tidy[2J the docs."));
			const reminded = await call(server, "remind", { text: "Later", at: "23:59" });
			expect(reminded.isError).toBeUndefined();
			expect(server.service.day.today().needsYou.some((n) => n.kind === "reminder")).toBe(false);
			expect(server.service.hud().awaiting).toBe(0);
			const id = /Reminder (\d+)/.exec(reminded.text)?.[1] as string;
			expect((await call(server, "inbox_done", { id })).text).toBe("Done: Later");
			// Outside text in an [event] line is one line in «» with no « or » of its own (SEC-2).
			expect(quoted("a «b»\n\tc")).toBe('«a "b" c»');
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"the server listens and answers the page before the service has started; the spend reads null until today's sessions are read",
	async () => {
		const herdr = new FakeHerdr();
		// herdr is slow to make workspace Jocasta: the service's start waits on it.
		const release = Promise.withResolvers<void>();
		const ensure = herdr.ensureWorkspace.bind(herdr);
		herdr.ensureWorkspace = async (...args) => {
			await release.promise;
			return ensure(...args);
		};
		const server = await startServer({
			herdr,
			graph: null,
			day: { pollers: false },
			port: 0,
			sessionsDir: join(env.data, "omp-sessions"),
			whiteboard: null,
			panePrompt: null,
			refreshMs: 60_000,
			mise: null,
			weeklyReviews: false,
			...relayTestOptions(),
		});
		try {
			let started = false;
			void server.ready.then(() => {
				started = true;
			});
			const page = await Page.open(Object.assign(server, { herdr }));
			const snapshot = await page.waitFor((m) => m.type === "snapshot");
			expect(snapshot.type === "snapshot" && snapshot.hud.spendToday).toBeNull();
			expect(started).toBe(false);
			release.resolve();
			await server.ready;
			expect(server.service.hud().spendToday).toBe(0);
			expect(herdr.workspaceList.map((w) => w.label)).toContain("Jocasta");
		} finally {
			release.resolve();
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"yesterday's missing worklog is written at the start from sessions read in steps, the event loop running meanwhile (0302)",
	async () => {
		const project = await demoProject(env.root);
		// A session from yesterday in the project, over 2 MB, so its read takes several steps. The day comes from the
		// service's clock, so a run across midnight can't shift it (SH-20).
		const fixed = new Date();
		fixed.setHours(12, 0, 0, 0);
		const yesterday = new Date(fixed);
		yesterday.setDate(yesterday.getDate() - 1);
		yesterday.setHours(10, 0, 0, 0);
		const dir = join(env.data, "omp-sessions", "--demo--");
		mkdirSync(dir, { recursive: true });
		const lines = [
			JSON.stringify({ type: "session", id: "y", cwd: project.mainPath, timestamp: yesterday.toISOString() }),
		];
		for (let i = 0; i < 1500; i++) {
			lines.push(
				JSON.stringify({
					type: "message",
					id: `m${i}`,
					message: {
						role: "assistant",
						timestamp: yesterday.getTime() + i * 1000,
						content: [{ type: "text", text: `${i} ${"x".repeat(1500)}` }],
					},
				}),
			);
		}
		writeFileSync(join(dir, "yesterday.jsonl"), `${lines.join("\n")}\n`);
		const pad = (n: number) => String(n).padStart(2, "0");
		const day = `${yesterday.getFullYear()}-${pad(yesterday.getMonth() + 1)}-${pad(yesterday.getDate())}`;
		let ticks = 0;
		const ticker = setInterval(() => ticks++, 0);
		try {
			await withServer(
				async (server) => {
					const before = ticks;
					await server.service.day.worklogWriting;
					expect(ticks).toBeGreaterThan(before);
					expect(readFileSync(worklogPath(day), "utf8")).toContain("demo");
				},
				{ day: { pollers: false, now: () => fixed } },
			);
		} finally {
			clearInterval(ticker);
		}
	},
	GIT_TEST_TIMEOUT,
);
