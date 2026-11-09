import { expect, test } from "bun:test";
import { appendFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { HANDOFF_PROMPT } from "../../../src/frontier/server/wording.ts";
import { GIT_TEST_TIMEOUT, poll, sh, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import {
	appendProposal,
	bossIdle,
	bossLines,
	bossPane,
	call,
	commitIn,
	demoProject,
	HUNK,
	handoffEntry,
	planModeFile,
	proposedPlanFile,
	review,
	startTestServer,
	startThread,
	toHandoff,
	until,
	useHunkDaemon,
	withServer,
} from "./helpers.ts";

const env = useTempJocasta();
const HUNK_OPTIONS = useHunkDaemon();

test(
	"B1: a plan omp writes after herdr's idle event is still 'plan ready', and thread_send never types into a plan awaiting him (boss manual rule 4; 0405, 0407)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				server.herdr.setState(bossPane(server.herdr) as string, "idle");
				const { paneId, path } = await startThread(server);
				const planReady = "[event] demo/Readme line — plan ready, awaiting you";
				const said = (line: string) => bossLines(server).filter((l) => l.startsWith(line)).length;
				const typed = () => server.herdr.of("prompt").filter(([p]) => p === paneId).length;

				// The turn was read when it started; its proposal is in the file only 300 ms after herdr said idle.
				const file = planModeFile(path);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle");
				// The race itself: the proposal is written on the real clock after the idle event, as omp's writer did.
				await Bun.sleep(300);
				appendProposal(file);
				await until(() => said(planReady) === 1);
				expect(said("[event] demo/Readme line — finished its turn")).toBe(0);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });

				// He sends it back; the next proposal comes only after the turn was said finished.
				appendFileSync(
					file,
					`${[
						{ type: "message", id: "u1", message: { role: "user", timestamp: Date.now(), content: "Smaller." } },
						{
							type: "message",
							id: "a1",
							message: { role: "assistant", content: [{ type: "text", text: "Revised." }] },
						},
					]
						.map((e) => JSON.stringify(e))
						.join("\n")}\n`,
				);
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await until(() => {
					bossIdle(server);
					return said("[event] demo/Readme line — finished its turn") === 1;
				});
				appendProposal(file);
				const before = typed();
				const reply = await call(server, "thread_send", {
					thread: "Readme line",
					text: "Carry on and show him a plan.",
				});
				expect(reply).toEqual({
					text: "demo/Readme line's plan is ready and awaiting his approval; he decides in its pane.",
					isError: true,
				});
				expect(typed()).toBe(before);
				// The plan the idle event missed now waits on him, and the boss hears it.
				await until(() => {
					bossIdle(server);
					return said(planReady) === 2;
				});
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"B3: a plan awaiting him survives Stop everything and reopen: the resumed thread counts on the HUD, in Awaiting you and in the close question (0432, 0434, 0438)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		let server = await startTestServer({ herdr, ...HUNK_OPTIONS, agentWaitMs: 10_000 });
		let file = "";
		try {
			const { paneId, path } = await startThread(server);
			file = proposedPlanFile(path);
			herdr.setState(paneId, "working", file);
			herdr.setState(paneId, "idle");
			await until(() => server.service.busyThreads().awaiting === 1);
			await server.service.refresh();
		} finally {
			await server.stop();
		}
		// herdr comes back with every pane a shell; the thread's omp resumes with `--continue`, back in plan mode.
		herdr.restore();
		server = await startTestServer({ herdr, ...HUNK_OPTIONS });
		try {
			const fresh = herdr.paneList.find((p) => p.cwd && resolve(p.cwd).includes("readme-line"))?.paneId as string;
			appendFileSync(
				file,
				`${JSON.stringify({ type: "mode_change", id: "m3", mode: "plan", data: { planFilePath: "local://readme-line-plan.md" } })}\n`,
			);
			herdr.setState(fresh, "idle", file);
			await poll(
				async () => {
					if (server.service.busyThreads().awaiting !== 0) return true;
					await server.service.refresh();
					return false;
				},
				{ timeoutMs: 5_000, intervalMs: 100 },
			);
			expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });
			expect(server.service.hud().awaiting).toBe(1);
			expect((await call(server, "thread_status", {})).text).toBe(
				"demo/Readme line [feature/readme-line] plan ready\n  spend $0.25",
			);
			expect(await call(server, "thread_send", { thread: "Readme line", text: "Go." })).toMatchObject({
				isError: true,
			});
			// The only text typed into it is omp's `/plan-review`, once (0494).
			expect(herdr.of("prompt").filter(([p]) => p === fresh)).toEqual([[fresh, "/plan-review"]]);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"a done and reviewed thread survives Stop everything and reopen: still in Awaiting you, on the HUD and in the close question (0432, 0549)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		let server = await startTestServer({ herdr, ...HUNK_OPTIONS, agentWaitMs: 10_000 });
		let file = "";
		try {
			const { paneId, path } = await startThread(server);
			file = await toHandoff(server, paneId, path);
			handoffEntry(file);
			await until(() => server.service.hud().awaiting === 1);
			await server.service.refresh();
		} finally {
			await server.stop();
		}
		// herdr comes back with every pane a shell; the thread's omp starts again, idle.
		herdr.restore();
		server = await startTestServer({ herdr, ...HUNK_OPTIONS });
		try {
			const fresh = herdr.paneList.find((p) => p.cwd && resolve(p.cwd).includes("readme-line"))?.paneId as string;
			herdr.setState(fresh, "idle", file);
			await server.service.refresh();
			await server.service.refresh();
			expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });
			expect(server.service.hud().awaiting).toBe(1);
			expect((await call(server, "thread_status", {})).text).toStartWith("demo/Readme line [feature/readme-line] done");
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

/**
 * A thread whose omp left `session` (a plan proposed, or plan mode only) through Stop everything and a reopen (as B3):
 * herdr restores its pane as a shell, the thread's omp resumes with `--continue` and goes idle, its screen `screen`.
 */
async function reopened(session: (cwd: string) => string, screen: string) {
	await demoProject(env.root);
	const herdr = new FakeHerdr();
	// A plan-mode turn's end reads the session again for 200 ms, not the product's 2 s (TST-5, TST-7).
	const settle = { planSettleMs: 200 };
	let server = await startTestServer({ herdr, ...HUNK_OPTIONS, ...settle, agentWaitMs: 10_000 });
	let file = "";
	try {
		const { paneId, path } = await startThread(server);
		file = session(path);
		herdr.setState(paneId, "working", file);
		herdr.setState(paneId, "idle");
		await server.service.refresh();
	} finally {
		await server.stop();
	}
	herdr.restore();
	server = await startTestServer({ herdr, ...HUNK_OPTIONS, ...settle });
	const fresh = herdr.paneList.find((p) => p.cwd && resolve(p.cwd).includes("readme-line"))?.paneId as string;
	// The resumed omp re-enters plan mode in its session.
	appendFileSync(
		file,
		`${JSON.stringify({ type: "mode_change", id: "m3", mode: "plan", data: { planFilePath: "local://readme-line-plan.md" } })}\n`,
	);
	herdr.screens.set(fresh, screen);
	herdr.setState(fresh, "idle", file);
	/** Runs the service's refresh tick until `check` passes, or `times` ticks when no check is given. */
	const look = async (check?: () => boolean, times = 10) => {
		if (check) {
			await until(() => {
				void server.service.refresh();
				return check();
			});
		} else for (let i = 0; i < times; i++) await server.service.refresh();
	};
	const typed = () => herdr.of("prompt").filter(([p]) => p === fresh);
	const screenReads = () => herdr.of("screen").filter(([p]) => p === fresh).length;
	return { server, herdr, fresh, look, typed, screenReads };
}

/** omp's Plan Review as `pane read` shows it. */
const PLAN_REVIEW = [
	"Plan: readme-line",
	"> Approve and execute",
	"  Approve and compact context",
	"  Approve and keep context (~12k / 200k)",
	"  Refine plan",
	"  Save and quit",
	"↑↓ navigate  enter select  esc cancel",
].join("\n");

/** A question of omp's `ask` as `pane read` shows it. */
const QUESTION = [
	"Which file?",
	"> README.md",
	"  docs/intro.md",
	"  Other (type your own)",
	"↑↓ navigate  enter select  esc cancel",
].join("\n");

test(
	"a reopened thread whose plan awaits him gets /plan-review typed once, after its screen shows no picker (0494)",
	async () => {
		const { server, herdr, fresh, look, typed } = await reopened(proposedPlanFile, "plan mode  ▸ ");
		try {
			await look(() => typed().length > 0);
			expect(typed()).toEqual([[fresh, "/plan-review"]]);
			// It read the screen before typing.
			const calls = herdr.calls.map((c) => `${c.method} ${c.args[0]}`);
			expect(calls.indexOf(`screen ${fresh}`)).toBeLessThan(calls.indexOf(`prompt ${fresh}`));
			// Never a second time, even with the screen clear and omp idle again.
			herdr.screens.set(fresh, "");
			herdr.setState(fresh, "working");
			herdr.setState(fresh, "idle");
			await look();
			expect(typed()).toHaveLength(1);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"nothing is typed into a reopened thread while omp's Plan Review shows, not after it goes either (0494)",
	async () => {
		const { server, herdr, fresh, look, typed, screenReads } = await reopened(proposedPlanFile, PLAN_REVIEW);
		try {
			await look(() => screenReads() > 0);
			expect(server.service.busyThreads().awaiting).toBe(1);
			// The picker already open ends the wait.
			herdr.screens.set(fresh, "");
			await look();
			expect(typed()).toEqual([]);
			expect(screenReads()).toBe(1);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"nothing is typed into a reopened thread while a question shows; once it is answered the plan's review comes back (0494)",
	async () => {
		const { server, herdr, fresh, look, typed, screenReads } = await reopened(proposedPlanFile, QUESTION);
		try {
			await look(() => screenReads() >= 3);
			expect(server.service.busyThreads().awaiting).toBe(1);
			expect(typed()).toEqual([]);
			herdr.screens.set(fresh, "");
			await look(() => typed().length > 0);
			expect(typed()).toEqual([[fresh, "/plan-review"]]);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"a reopened thread whose plan isn't awaiting him gets nothing typed (0494)",
	async () => {
		const { server, herdr, fresh, look, typed } = await reopened(planModeFile, "");
		try {
			// The reopened omp is idle in plan mode with no proposal: the tick looks at it and finds no plan to show.
			await look();
			expect((await call(server, "thread_status", {})).text).toStartWith("demo/Readme line [feature/readme-line] idle");
			expect(server.service.busyThreads().awaiting).toBe(0);
			// A plan he gets later in this run is omp's own Plan Review, open: nothing is typed into it either.
			appendProposal(herdr.paneList.find((p) => p.paneId === fresh)?.sessionPath ?? "");
			herdr.setState(fresh, "working");
			herdr.setState(fresh, "idle");
			await look(() => server.service.busyThreads().awaiting === 1);
			await look();
			expect(typed()).toEqual([]);
			expect(herdr.of("screen")).toEqual([]);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT * 2,
);

test(
	"B4: the hunk session daemon the service started ends with Stop everything, also when Keep came first (0434, 0486)",
	async () => {
		await demoProject(env.root);
		// A free loopback port for the daemon, never hunk's or Jocasta's real one.
		const probe = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => new Response() });
		const port = probe.port as number;
		probe.stop(true);
		const up = async () => {
			try {
				return (await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(500) })).ok;
			} catch {
				return false;
			}
		};
		const answers = [false, true];
		const server = await startTestServer({
			hunk: HUNK,
			hunkPort: port,
			agentWaitMs: 10_000,
			stopContainers: async () => {},
			onQuestion: (popup) => {
				const choice = (answers.shift() ?? false) ? "Stop everything" : "Cancel";
				// One reader per question (0578): the view first, then the answer with its reader secret.
				const url = `http://127.0.0.1:${server.port}/api/popup/${popup.code}`;
				const authorization = `Bearer ${server.token}`;
				void fetch(url, { headers: { authorization } }).then((view) =>
					fetch(url, {
						method: "POST",
						headers: { authorization, "x-jocasta-reader": view.headers.get("x-jocasta-reader") ?? "" },
						body: JSON.stringify({ choice, typed: popup.view.code }),
					}),
				);
			},
		});
		try {
			const { paneId, path } = await startThread(server);
			// A task ends: the review starts hunk's daemon before its pane opens.
			server.herdr.setState(paneId, "working");
			commitIn(path, "demo\nA line.\n", "Add a README line");
			server.herdr.setState(paneId, "idle");
			await until(() => server.herdr.of("splitBeside").length === 1);
			expect(await up()).toBe(true);
			// The review turn runs: he is asked, keeps it running, and the daemon stays.
			server.herdr.setState(paneId, "working");
			expect(await server.shutdown({ ask: true })).toBe("kept");
			expect(await up()).toBe(true);
			// Later he stops everything: the daemon goes too.
			expect(await server.shutdown({ ask: true })).toBe("stopped");
			await server.stopped;
			expect(await up()).toBe(false);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"P6, B6: a reviewed thread counts in the close question as on the HUD; work the boss's thread_send started isn't said as his rejection, work he started is (0432, 0438, 0471)",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				server.herdr.setState(bossPane(server.herdr) as string, "idle");
				const { paneId, path } = await startThread(server);
				const said = (line: string) => bossLines(server).filter((l) => l.startsWith(line)).length;
				const file = await toHandoff(server, paneId, path);
				handoffEntry(file);
				await until(() => server.service.hud().awaiting === 1);
				expect(server.service.busyThreads()).toEqual({ working: 0, awaiting: 1 });

				// The boss relays his word into the reviewed thread.
				expect((await call(server, "thread_send", { thread: "Readme line", text: "Fix the conflict." })).text).toBe(
					"Sent to demo/Readme line.",
				);
				server.herdr.setState(paneId, "working");
				const relayed =
					"[event] demo/Readme line — at work again on its reviewed work, after your thread_send · offer once to save a lesson if he rejected that work";
				await until(() => {
					bossIdle(server);
					return said(relayed) === 1;
				});
				expect(said("[event] demo/Readme line — he sent its reviewed work back")).toBe(0);

				// The next task is reviewed; then he sets it to work himself.
				commitIn(path, "demo\nA line.\nAnother.\n", "Another line");
				server.herdr.setState(paneId, "idle");
				const reviewText = review("main...feature/readme-line");
				await until(() => server.herdr.of("prompt").filter(([p, t]) => p === paneId && t === reviewText).length === 2);
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await until(
					() => server.herdr.of("prompt").filter(([p, t]) => p === paneId && t === HANDOFF_PROMPT).length === 2,
				);
				appendFileSync(
					file,
					`${JSON.stringify({ type: "compaction", id: "h2", method: "handoff", summary: "Handed off.", timestamp: new Date().toISOString() })}\n`,
				);
				server.herdr.setState(paneId, "working");
				await until(() => {
					bossIdle(server);
					return said("[event] demo/Readme line — he sent its reviewed work back · offer once to save a lesson") === 1;
				});
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"P3: a task's review line counts the thread's own commits, not main's it merged in",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "config", "user.name", "Test");
		sh(project.mainPath, "config", "user.email", "test@example.com");
		await withServer(
			async (server) => {
				server.herdr.setState(bossPane(server.herdr) as string, "idle");
				const { paneId, path } = await startThread(server);
				// Its /handoff writes no entry (CO-4): the next run's commits count from the reviewed HEAD.
				await toHandoff(server, paneId, path);
				// main moves; the thread merges it in, then commits its own work.
				writeFileSync(join(project.mainPath, "other.txt"), "other\n");
				sh(project.mainPath, "add", "other.txt");
				sh(project.mainPath, "commit", "-qm", "Other work");
				server.herdr.setState(paneId, "working");
				sh(path, "merge", "--no-edit", "main");
				commitIn(path, "demo\nA line.\nOwn.\n", "Own line");
				server.herdr.setState(paneId, "idle");
				const reviewText = review("main...feature/readme-line");
				await until(() => server.herdr.of("prompt").filter(([p, t]) => p === paneId && t === reviewText).length === 2);
				server.herdr.setState(paneId, "working");
				server.herdr.setState(paneId, "idle");
				await until(() => {
					bossIdle(server);
					return bossLines(server).some((l) =>
						l.startsWith("[event] demo/Readme line — done and reviewed: 1 new commit «Own line»"),
					);
				});
			},
			{ agentWaitMs: 10_000, ...HUNK_OPTIONS },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"P8: a 'plan ready' line still waiting for the boss is dropped once he approved the plan",
	async () => {
		await demoProject(env.root);
		await withServer(
			async (server) => {
				const boss = bossPane(server.herdr) as string;
				server.herdr.setState(boss, "idle");
				const { paneId, path } = await startThread(server);
				// The boss waits on his answer to its own question: herdr refuses lines into it, so they wait.
				const prompt = server.herdr.prompt.bind(server.herdr);
				let refusing = true;
				server.herdr.prompt = async (target: string, text: string) => {
					if (target === boss && refusing) throw new Error("agent_blocked");
					return prompt(target, text);
				};
				const file = proposedPlanFile(path);
				server.herdr.setState(paneId, "working", file);
				server.herdr.setState(paneId, "idle");
				await until(() => server.service.busyThreads().awaiting === 1);
				// He approves in Plan Review: omp leaves plan mode and works.
				appendFileSync(file, `${JSON.stringify({ type: "mode_change", id: "m4", mode: "none" })}\n`);
				server.herdr.setState(paneId, "working");
				await until(() => server.service.busyThreads().awaiting === 0);
				refusing = false;
				bossIdle(server);
				// A later line reaches the boss: any plan line still waiting would have gone with or before it.
				server.herdr.setState(paneId, "idle");
				await until(() => bossLines(server).some((l) => l.startsWith("[event] demo/Readme line — finished its turn")));
				expect(bossLines(server).filter((l) => l.includes("plan ready"))).toEqual([]);
			},
			{ agentWaitMs: 10_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);
