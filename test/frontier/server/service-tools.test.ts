import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { hunkEnv } from "../../../src/frontier/server/hunk.ts";
import { overlayFile } from "../../../src/frontier/server/overlay.ts";
import { TOOLS } from "../../../src/frontier/server/tools.ts";
import { HANDOFF_PROMPT } from "../../../src/frontier/server/wording.ts";
import { runMcpServer } from "../../../src/mcp/server.ts";
import { pidAlive } from "../../../src/shared/proctree.ts";
import { GIT_TEST_TIMEOUT, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import {
	bossPane,
	call,
	commitIn,
	demoProject,
	GUARD,
	keyed,
	Page,
	README_THREAD_ENV,
	review,
	startTestServer,
	startThread,
	TEST_BOSS_KEY,
	threadOmp,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";
import { killAll, settles, shimTree } from "./sleepers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"graph_search, today, note and the client's pane.focus and link.open go where they should; speak is no tool (0552, 0581)",
	async () => {
		await demoProject(env.root);
		await withServer(async (server) => {
			// A pane Jocasta didn't start has no project (nor a pane key, 0578); the boss must name one.
			expect((await call(server, "graph_search", { query: "why" }, "p_none")).text).toBe(
				"No project here: graph_search answers in the project and thread panes Jocasta started; a pane Jocasta started before panes had keys gets one when Jocasta starts it again, once it is idle.",
			);
			expect(await call(server, "graph_search", { query: "why" })).toEqual({
				text: "graph_search: name the project whose graph to search.",
				isError: true,
			});
			expect(await call(server, "graph_search", { query: "why", project: "demo" })).toEqual({
				text: "the knowledge graph isn't available in this run.",
				isError: true,
			});
			expect(await call(server, "speak", { text: "Done." })).toEqual({ text: "no tool speak.", isError: true });
			expect((await call(server, "note", { text: "Tidy the docs.", project: "demo" })).text).toMatch(
				/^Filed into demo's next work \(inbox \d+\)\.$/,
			);
			expect((await call(server, "today", {})).text).toContain("demo: Tidy the docs.");

			const page = await Page.open(server);
			const snapshot = await page.waitFor((m) => m.type === "snapshot");
			expect(snapshot.type === "snapshot" && snapshot.today.next).toEqual([
				expect.objectContaining({ text: "Tidy the docs.", project: "demo" }),
			]);
			const boss = server.herdr.paneList[0]?.paneId as string;
			page.send({ type: "pane.focus", paneId: boss });
			await until(() => server.herdr.of("focus").length === 1);
			expect(server.herdr.of("focus")).toEqual([[boss]]);
			// A link that isn't one today or the dashboard shows is never opened (C9).
			page.send({ type: "link.open", url: "https://example.org/not-shown" });
			expect(await page.waitFor((m) => m.type === "error")).toEqual({
				type: "error",
				message: "That link isn't one today shows.",
			});
			// Without whiteboard nothing is shown, and the boss is told so.
			expect(await call(server, "show", { review: "r-1" })).toEqual({
				text: "show: whiteboard isn't running in this Jocasta; nothing can be shown.",
				isError: true,
			});
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"show(review) opens only a review whiteboard has, in his browser; otherwise it says why (0393, 0552)",
	async () => {
		const reviews: Record<string, string> = { "r-1": "Readme line" };
		const opened: string[] = [];
		await withServer(
			async (server) => {
				expect(await call(server, "show", { review: "demo/readme-line" })).toEqual({
					text: "show: no whiteboard review demo/readme-line; create one with whiteboard's session_create and pass its sessionId.",
					isError: true,
				});
				expect((await call(server, "show", { review: "r-1" })).text).toBe(
					'Opened review "Readme line" (r-1) in his browser.',
				);
				expect(opened).toEqual(["r-1"]);
			},
			{
				findReview: async (id) => (reviews[id] ? { title: reviews[id] } : null),
				openReview: (id) => void opened.push(id),
			},
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread closed in herdr: the boss hears it once, unspoken, and its worktree and branch stay (0405)",
	async () => {
		const project = await demoProject(env.root);
		await withServer(
			async (server) => {
				const page = await Page.open(server);
				await page.waitFor((m) => m.type === "snapshot");
				const { paneId, path } = await startThread(server);
				const boss = server.herdr.paneList[0]?.paneId as string;
				server.herdr.setState(boss, "idle");
				await server.herdr.closeWorkspace(
					server.herdr.paneList.find((p) => p.paneId === paneId)?.workspaceId as string,
				);
				await server.service.refresh();
				await server.service.refresh();
				const line = "[event] demo/Readme line — closed; its worktree and branch stay";
				await until(() => server.herdr.of("prompt").some(([p, t]) => p === boss && t === line));
				expect(server.herdr.of("prompt").filter(([, t]) => t === line)).toHaveLength(1);
				expect(existsSync(path)).toBe(true);
				expect(sh(project.mainPath, "branch", "--list", "feature/readme-line")).toContain("feature/readme-line");
				expect((await call(server, "thread_status", {})).text).toBe("No threads.");
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread pane he closes in herdr ends what its omp left running, the workers omp starts on its way out included, found through the shim's child; a process something else started in its worktree keeps running (0490)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const { paneId, path } = await startThread(server);
				// The pane's shim (real processes, working in the worktree); its real omp isn't up yet.
				const tree = await shimTree(join(path, "pane"), true);
				// He started it in the worktree from another terminal: a real process that stays up until killed.
				const other = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
					cwd: path,
					stdio: ["ignore", "ignore", "ignore"],
				});
				try {
					server.herdr.roots.set(paneId, tree.shim);
					// The refresh tick reads the pane's shim once herdr names it, then its omp on a later tick.
					await server.service.refresh();
					await tree.startOmp();
					await server.service.refresh();
					// He closes the thread's workspace in herdr: the shim goes, omp starts its workers and goes.
					tree.exit();
					process.kill(tree.shim, "SIGKILL");
					await server.herdr.closeWorkspace(
						server.herdr.paneList.find((p) => p.paneId === paneId)?.workspaceId as string,
					);
					await server.service.refresh();
					expect(await settles(() => tree.workers().length === 2)).toBe(true);
					expect(await settles(() => !tree.workers().some(pidAlive))).toBe(true);
					expect(pidAlive(tree.omp() as number)).toBe(false);
					expect(pidAlive(other.pid)).toBe(true);
					expect(existsSync(path)).toBe(true);
				} finally {
					await tree.stop();
					killAll([other.pid]);
					await other.exited;
				}
			},
			{ agentWaitMs: 10_000, leftoverWaitMs: 10_000, leftoverAgainMs: 200 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"herdr restored its panes as shells: Jocasta goes first, the boss, project and thread panes run omp again with their configs, the thread's unreviewed commit gets its review, and a second start leaves all alone (0394, 0404, 0424)",
	async () => {
		const project = await demoProject(env.root);
		const herdr = new FakeHerdr();
		const range = "main...feature/readme-line";
		let server = await startTestServer({ herdr, ...HUNK_OPTIONS, agentWaitMs: 10_000 });
		let threadPane: string;
		let path: string;
		try {
			({ paneId: threadPane, path } = await startThread(server));
			// The refresh tick notes each new pane's terminal.
			await server.service.refresh();
			// Another workspace made before Jocasta's (he opened it) pushes Jocasta off first after the restore.
			await herdr.ensureWorkspace("scratch", env.root);
			await herdr.moveWorkspace(herdr.workspaceList.find((w) => w.label === "scratch")?.id as string, 0);
		} finally {
			await server.stop();
		}
		// The task ends while the service is off; then herdr restarts and restores every pane as a shell.
		commitIn(path, "demo\nA line.\n", "Add a README line");
		herdr.restore();
		const before = herdr.calls.length;
		server = await startTestServer({ herdr, ...HUNK_OPTIONS });
		try {
			const respawned = herdr.calls
				.slice(before)
				.filter((c) => c.method === "respawn")
				.map((c) => c.args) as [string, { cwd: string; command: string[]; env?: Record<string, string> }][];
			const overlay = ["omp", "--continue", ...GUARD, "--config", overlayFile(project.id)];
			const thread = threadOmp(project.id);
			expect(respawned.map(([, o]) => o.command.slice(0, 3))).toEqual([
				["omp", "--continue", "--config"],
				["omp", "--continue", "--append-system-prompt"],
				["omp", "--continue", "--config"],
			]);
			expect(respawned[0]?.[1].command[3]).toEndWith("boss.yml");
			expect(respawned[0]?.[1].cwd).toEndWith("home");
			// Only the boss pane gets the run's boss key (0484); every pane its own pane key (0578).
			expect(respawned[0]?.[1].env).toEqual(keyed({ JOCASTA_BOSS_KEY: TEST_BOSS_KEY }));
			expect(respawned.slice(1).map(([, o]) => ({ ...o, cwd: resolve(o.cwd) }))).toEqual([
				{ cwd: path, command: thread, env: keyed({ ...hunkEnv(HUNK_OPTIONS.hunkPort), ...README_THREAD_ENV }) },
				{ cwd: project.mainPath, command: overlay, env: keyed(hunkEnv(HUNK_OPTIONS.hunkPort)) },
			]);
			const keys = respawned.map(([, o]) => o.env?.JOCASTA_PANE_KEY);
			expect(new Set(keys).size).toBe(3);
			// The scratch shell he opened isn't Jocasta's: left alone.
			expect(respawned.some(([, o]) => o.cwd === env.root)).toBe(false);
			expect(herdr.workspaceList[0]?.label).toBe("Jocasta");

			// omp comes up idle in the thread's new pane: its unreviewed commit is reviewed.
			const fresh = herdr.paneList.find((p) => resolve(p.cwd ?? "") === path)?.paneId as string;
			expect(fresh).toBeDefined();
			expect(fresh).not.toBe(threadPane);
			herdr.setState(fresh, "idle");
			await server.service.refresh();
			await until(() => herdr.of("prompt").some(([p, t]) => p === fresh && t === review(range)));
			herdr.setState(fresh, "working");
			herdr.setState(fresh, "idle");
			await until(() => herdr.of("prompt").some(([p, t]) => p === fresh && t === HANDOFF_PROMPT));
		} finally {
			await server.stop();
		}

		// The service starts again with omp running everywhere (the boss pane included) and the commit reviewed:
		// nothing is redone, except that the idle boss's omp, started by the earlier run, starts again with this run's
		// boss key (0484), resuming its session.
		const jocastaWs = herdr.workspaceList.find((w) => w.label === "Jocasta")?.id;
		const bossPane = herdr.paneList.find((p) => p.workspaceId === jocastaWs)?.paneId as string;
		herdr.setState(bossPane, "idle");
		const again = herdr.calls.length;
		server = await startTestServer({ herdr, ...HUNK_OPTIONS, bossKey: "next-run-key" });
		try {
			await server.service.refresh();
			const redone = herdr.calls.slice(again).filter((c) => ["respawn", "prompt", "splitBeside"].includes(c.method));
			expect(redone).toEqual([
				{
					method: "respawn",
					args: [bossPane, expect.objectContaining({ env: keyed({ JOCASTA_BOSS_KEY: "next-run-key" }) })],
				},
			]);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"a boss pane an earlier run started is told it lacks the key only when it sends none; a wrong key gets the plain refusal (0484)",
	async () => {
		const herdr = new FakeHerdr();
		let server = await startTestServer({ herdr, ...HUNK_OPTIONS });
		await server.stop();
		// The earlier run's boss is still at work, so this run doesn't start it again with its key yet.
		const boss = bossPane(herdr) as string;
		herdr.setState(boss, "working");
		server = await startTestServer({ herdr, ...HUNK_OPTIONS, bossKey: "next-run-key" });
		try {
			await server.service.refresh();
			expect(herdr.of("respawn")).toEqual([]);
			expect(await call(server, "projects", {}, boss, null)).toEqual({
				text: "projects: this boss session started before Jocasta's current run and lacks its boss key; Jocasta starts it again with the key once it is idle.",
				isError: true,
			});
			expect(await call(server, "projects", {}, boss, "wrong-key")).toEqual({
				text: "projects is the boss's tool; only the boss pane Jocasta started in workspace Jocasta can use it.",
				isError: true,
			});
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

/** What herdr's PowerShell shows in a new pane: its prompt alone. */
const PROMPT = "PS C:\\Users\\example>";

/**
 * A session as Jocasta's service finds it: herdr's default workspace (a shell in his home folder, made when his client
 * attached to the empty session) changed by `touch`, after `before`; then the service starts and refreshes once.
 */
async function defaultWorkspaceAfterStart(
	touch: (herdr: FakeHerdr, paneId: string) => Promise<void> | void = () => {},
	before: (herdr: FakeHerdr) => Promise<void> | void = () => {},
) {
	const herdr = new FakeHerdr();
	await before(herdr);
	const workspaceId = await herdr.ensureWorkspace("example", homedir());
	const paneId = herdr.paneList.find((p) => p.workspaceId === workspaceId)?.paneId as string;
	herdr.screens.set(paneId, `${PROMPT}\n\n`);
	await touch(herdr, paneId);
	const server = await startTestServer({ herdr, ...HUNK_OPTIONS });
	try {
		await server.service.refresh();
	} finally {
		await server.stop();
	}
	const closed = herdr.of("closeWorkspace").map(([id]) => id);
	return { herdr, workspaceId, closed };
}

test(
	"a session Jocasta created loses herdr's untouched default workspace once Jocasta's exists (0495)",
	async () => {
		const { herdr, workspaceId, closed } = await defaultWorkspaceAfterStart();
		expect(closed).toEqual([workspaceId]);
		expect(herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta"]);

		// An empty new session (Jocasta started its server, no client attached yet) has nothing to close.
		const empty = new FakeHerdr();
		const server = await startTestServer({ herdr: empty, ...HUNK_OPTIONS });
		try {
			await server.service.refresh();
		} finally {
			await server.stop();
		}
		expect(empty.of("closeWorkspace")).toEqual([]);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a session Jocasta didn't create keeps every workspace, the untouched default included (0495)",
	async () => {
		// herdr's server was already running (his client started it, or an earlier run): the same untouched default stays.
		const running = await defaultWorkspaceAfterStart(
			() => {},
			(herdr) => {
				herdr.startedServer = false;
			},
		);
		expect(running.closed).toEqual([]);
		// herdr restored a session that had Jocasta's workspace.
		const { closed } = await defaultWorkspaceAfterStart(
			() => {},
			async (herdr) => {
				await herdr.ensureWorkspace("Jocasta", homedir());
			},
		);
		expect(closed).toEqual([]);
		// A session with two workspaces before Jocasta's: one of them is his.
		const two = await defaultWorkspaceAfterStart(async (herdr) => {
			await herdr.ensureWorkspace("notes", homedir());
		});
		expect(two.closed).toEqual([]);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a touched default workspace stays: a second pane, an agent, another folder, or something typed (0495)",
	async () => {
		const touches: Record<string, (herdr: FakeHerdr, paneId: string) => Promise<void> | void> = {
			"a second pane": async (herdr, paneId) => {
				await herdr.splitBeside(paneId, { command: ["pwsh"], cwd: homedir() });
			},
			"an agent": (herdr, paneId) => herdr.setState(paneId, "idle", undefined, false),
			"another folder": (herdr, paneId) => {
				const pane = herdr.paneList.find((p) => p.paneId === paneId);
				if (pane) pane.cwd = env.root;
			},
			"a command run": (herdr, paneId) =>
				void herdr.screens.set(paneId, `${PROMPT} dir\n\n    Directory: C:\\Users\\example\n${PROMPT}`),
			"text typed, not run": (herdr, paneId) => void herdr.screens.set(paneId, `${PROMPT} git sta`),
		};
		for (const [what, touch] of Object.entries(touches)) {
			const { closed } = await defaultWorkspaceAfterStart(touch);
			expect({ what, closed }).toEqual({ what, closed: [] });
		}
	},
	GIT_TEST_TIMEOUT,
);

/** The tool names `jocasta mcp` lists (`tools/list`) with `boss` and `env`. */
async function mcpNames(boss: boolean, env: Record<string, string>): Promise<string[]> {
	const out: string[] = [];
	const list = `${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" })}\n`;
	const input = new ReadableStream<Uint8Array>({
		start(controller) {
			controller.enqueue(new TextEncoder().encode(list));
			controller.close();
		},
	});
	await runMcpServer({ boss, input, output: (line) => out.push(line), service: () => undefined, env });
	const reply = JSON.parse(out[0] ?? "{}") as { result?: { tools?: { name: string }[] } };
	return (reply.result?.tools ?? []).map((t) => t.name).sort();
}

test(
	"the service's tool registry agrees with jocasta mcp: a thread's pane lists exactly the tools any pane may call, the boss's the rest (0484, 0539, 0542, SVC-15)",
	async () => {
		const named = (boss: boolean) =>
			Object.entries(TOOLS)
				.filter(([, tool]) => tool.boss === boss)
				.map(([name]) => name)
				.sort();
		expect(await mcpNames(false, { JOCASTA_THREAD: "readme-line" })).toEqual(named(false));
		const boss = await mcpNames(true, {});
		expect(boss.filter((name) => !named(false).includes(name))).toEqual(named(true));
	},
	GIT_TEST_TIMEOUT,
);
