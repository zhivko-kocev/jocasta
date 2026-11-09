// Jocasta opens and closes as one, with a guard (0434, 0438, 0549): the service's one shutdown path against a fake
// herdr; its close question goes through the question API (0552).
import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { POPUP_READER_HEADER } from "../../../src/frontier/protocol.ts";
import { askShutdown, SHUTDOWN_PATH } from "../../../src/frontier/server/launch.ts";
import { CANCEL, STOP } from "../../../src/frontier/server/popups.ts";
import { type ServerOptions, startServer } from "../../../src/frontier/server/server.ts";
import { runStopSteps, type StopStep, undoStart } from "../../../src/frontier/server/stop-sequence.ts";
import { dataDir } from "../../../src/shared/paths.ts";
import { readServiceFile, serviceFilePath } from "../../../src/shared/service-file.ts";
import { GIT_TEST_TIMEOUT, useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { demoProject, relayTestOptions, startTestServer, TEST_BOSS_KEY, type TestServer, until } from "./helpers.ts";

const env = useTempJocasta();

/**
 * A server whose close question is answered with `answers` in turn (Stop everything, typed with its code, for true;
 * Cancel for false) through `GET` then `POST /api/popup/<code>` with the reader secret, as what shows it would; every
 * question, terminal opened and container stop kept.
 */
async function guarded(answers: boolean[], options: ServerOptions = {}) {
	const asked: string[] = [];
	const steps: string[] = [];
	let reattached = 0;
	const server: TestServer = await startTestServer({
		onQuestion: (popup) => {
			asked.push(popup.view.message);
			const choice = (answers.shift() ?? false) ? STOP : CANCEL;
			const url = `http://127.0.0.1:${server.port}/api/popup/${popup.code}`;
			const authorization = `Bearer ${server.token}`;
			void (async () => {
				const read = await fetch(url, { headers: { authorization } });
				await fetch(url, {
					method: "POST",
					headers: { authorization, [POPUP_READER_HEADER]: read.headers.get(POPUP_READER_HEADER) ?? "" },
					body: JSON.stringify({ choice, typed: popup.view.code }),
				});
			})();
		},
		reattach: () => {
			reattached++;
		},
		stopContainers: async () => {
			steps.push(`containers after ${server.herdr.calls.map((c) => c.method).join(",")}`);
		},
		...options,
	});
	return { server, asked, steps, reattached: () => reattached };
}

/** Starts a thread through the MCP endpoint, as the boss's `thread_start` does, and returns its pane. */
async function startThread(server: TestServer): Promise<string> {
	const replied = fetch(`http://127.0.0.1:${server.port}/api/mcp/thread_start`, {
		method: "POST",
		headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
		body: JSON.stringify({
			args: { project: "demo", title: "Readme line", brief: "Add a README line." },
			bossKey: TEST_BOSS_KEY,
		}),
	});
	await until(() => server.herdr.of("createWorktree").length === 1);
	const pane = server.herdr.paneList.at(-1);
	if (!pane) throw new Error("no thread pane");
	server.herdr.setState(pane.paneId, "idle");
	await replied;
	return pane.paneId;
}

test("with no thread at work, closing asks nothing and stops everything", async () => {
	const { server, asked, steps } = await guarded([]);
	expect(await server.shutdown({ ask: true })).toBe("stopped");
	await server.stopped;
	expect(asked).toEqual([]);
	expect(steps).toHaveLength(1);
	expect(existsSync(serviceFilePath())).toBe(false);
});

test(
	"the guard asks only while a thread is working or awaiting him, counting them apart; Cancel leaves everything running (0434, 0438, 0549)",
	async () => {
		await demoProject(env.root);
		const { server, asked, steps, reattached } = await guarded([false, false]);
		try {
			const paneId = await startThread(server);
			server.herdr.setState(paneId, "working");
			await until(() => server.service.busyThreads().working === 1);
			const before = server.herdr.calls.length;
			expect(await server.shutdown({ ask: true })).toBe("kept");
			expect(asked).toEqual(["Stop 1 working thread? They pick up where they left off next time."]);

			server.herdr.setState(paneId, "blocked");
			await until(() => server.service.hud().blocked === 1);
			expect(await server.shutdown({ ask: true })).toBe("kept");
			expect(asked).toEqual([
				"Stop 1 working thread? They pick up where they left off next time.",
				"Stop 1 thread awaiting you? They pick up where they left off next time.",
			]);
			// The quit action asks where he is: no new terminal.
			expect(reattached()).toBe(0);

			// Kept: herdr was neither held down nor stopped, no container stopped, the service still answers.
			const after = server.herdr.calls.slice(before).map((c) => c.method);
			expect(after).not.toContain("keepDown");
			expect(after).not.toContain("stopServer");
			expect(steps).toEqual([]);
			expect(server.herdr.paneList.some((p) => p.paneId === paneId)).toBe(true);
			expect(readServiceFile()?.port).toBe(server.port);
			const health = await fetch(`http://127.0.0.1:${server.port}/api/mcp/thread_status`, {
				method: "POST",
				headers: { authorization: `Bearer ${server.token}` },
				body: JSON.stringify({ args: {} }),
			});
			expect(health.ok).toBe(true);

			// He answers, the turn ends: nothing to cut off, so closing asks nothing.
			server.herdr.setState(paneId, "working");
			server.herdr.setState(paneId, "idle");
			await until(() => {
				const { working, awaiting } = server.service.busyThreads();
				return working + awaiting === 0;
			});
			expect(await server.shutdown({ ask: true })).toBe("stopped");
			expect(asked).toHaveLength(2);
			expect(steps).toHaveLength(1);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"Stop: herdr is held down, mid-turn threads are kept for --restore, the service stops, then herdr's server, then the containers and what else registered (0434, 0549)",
	async () => {
		await demoProject(env.root);
		const { server, asked, steps } = await guarded([true]);
		server.stopsWith("the relay", () => void steps.push("relay"));
		const paneId = await startThread(server);
		server.herdr.setState(paneId, "working", join(env.root, "s.jsonl"));
		await until(() => server.service.busyThreads().working === 1);
		const before = server.herdr.calls.length;
		expect(await server.shutdown({ ask: true })).toBe("stopped");
		await server.stopped;
		expect(asked).toEqual(["Stop 1 working thread? They pick up where they left off next time."]);
		const methods = server.herdr.calls.slice(before).map((c) => c.method);
		// Held down first; the service's last word to herdr (clearing its tokens) comes before the server stops.
		expect(methods.indexOf("keepDown")).toBeGreaterThanOrEqual(0);
		expect(methods.indexOf("keepDown")).toBeLessThan(methods.indexOf("stopServer"));
		expect(methods.at(-1)).toBe("stopServer");
		expect(methods.filter((m) => m === "stopServer")).toHaveLength(1);
		expect(steps).toEqual([`containers after ${server.herdr.calls.map((c) => c.method).join(",")}`, "relay"]);
		// The thread at work comes back with `omp --restore <session>` on the next start (0549).
		const panes = JSON.parse(readFileSync(join(dataDir(), "panes.json"), "utf8")) as Record<
			string,
			{ restore?: string }
		>;
		expect(panes[paneId]?.restore).toBe(join(env.root, "s.jsonl"));
		expect(server.herdr.paneList).toEqual([]);
		expect(existsSync(serviceFilePath())).toBe(false);
		// Asked again once stopped: the same outcome, nothing more.
		expect(await server.shutdown({ ask: true })).toBe("stopped");
		expect(server.herdr.of("stopServer")).toHaveLength(1);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a close right after a service restart waits for the threads to be found, so a working one is still asked about (LC-9)",
	async () => {
		await demoProject(env.root);
		const herdr = new FakeHerdr();
		const first = await startTestServer({ herdr });
		const paneId = await startThread(first);
		herdr.setState(paneId, "working");
		await until(() => first.service.busyThreads().working === 1);
		// A failed start's stop: herdr and the thread keep running.
		await first.stop();

		// The next service's discover() waits on herdr's snapshot.
		const gate = Promise.withResolvers<void>();
		const snapshot = herdr.snapshot.bind(herdr);
		herdr.snapshot = async () => {
			await gate.promise;
			return snapshot();
		};
		const asked: string[] = [];
		const second = await startServer({
			herdr,
			graph: null,
			day: { pollers: false },
			port: 0,
			sessionsDir: join(dataDir(), "omp-sessions"),
			agentWaitMs: 0,
			submitWaitMs: 0,
			refreshMs: 60_000,
			whiteboard: null,
			panePrompt: null,
			stopContainers: async () => {},
			mise: null,
			weeklyReviews: false,
			...relayTestOptions(),
			onQuestion: (popup) => {
				asked.push(popup.view.message);
				popup.close();
			},
		});
		try {
			const outcome = second.shutdown({ ask: true });
			gate.resolve();
			expect(await outcome).toBe("kept");
			expect(asked).toEqual(["Stop 1 working thread? They pick up where they left off next time."]);
		} finally {
			await second.ready.catch(() => {});
			await second.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);

test("a stop step that throws or hangs never keeps the close from finishing (LC-5, LC-8)", async () => {
	// hud.txt can't be written (a folder took its name): the service's stop fails at that step.
	const { server } = await guarded([]);
	const hud = join(dataDir(), "hud.txt");
	rmSync(hud, { force: true, recursive: true });
	mkdirSync(hud, { recursive: true });
	writeFileSync(join(hud, "keep"), "");
	expect(await server.shutdown({ ask: true })).toBe("stopped");
	await server.stopped;
	expect(server.herdr.of("stopServer")).toHaveLength(1);
	expect(existsSync(serviceFilePath())).toBe(false);
	rmSync(hud, { force: true, recursive: true });

	// Containers that never answer are left after their bound.
	const hung = await startTestServer({
		stopContainers: () => new Promise<void>(() => {}),
		stopBoundsMs: { containers: 200 },
	});
	expect(await hung.shutdown({ ask: true })).toBe("stopped");
	await hung.stopped;
	expect(existsSync(serviceFilePath())).toBe(false);
});

test("the stop runner: `all` steps only on Stop everything, `finally` steps last even after a throw or a step past its bound; a failed start's undo runs newest first (SRV-A-2, SRV-A-3)", async () => {
	const ran: string[] = [];
	const failed: string[] = [];
	const timedOut: string[] = [];
	const step = (name: string, more: Partial<StopStep> = {}): StopStep => ({ name, run: () => ran.push(name), ...more });
	const steps: StopStep[] = [
		step("last", { when: "finally" }),
		step("first"),
		step("only all", { when: "all" }),
		step("throws", {
			run: () => {
				ran.push("throws");
				throw new Error("no");
			},
		}),
		step("hangs", { run: () => new Promise(() => {}), ms: 20, onTimeout: () => timedOut.push("hangs") }),
		step("after"),
	];
	await runStopSteps(steps, false, (name) => failed.push(name));
	expect(ran).toEqual(["first", "throws", "after", "last"]);
	expect(failed).toEqual(["throws", "hangs"]);
	expect(timedOut).toEqual(["hangs"]);
	ran.length = 0;
	await runStopSteps(steps.slice(0, 3), true, () => {});
	expect(ran).toEqual(["first", "only all", "last"]);
	ran.length = 0;
	await undoStart([step("relay"), step("herdr"), step("server")]);
	expect(ran).toEqual(["server", "herdr", "relay"]);
});

test("bare jocasta's POST /api/shutdown needs the token, is answered at once, and with nothing at work stops everything (0434, 0549)", async () => {
	const { server, asked, steps, reattached } = await guarded([]);
	const url = `http://127.0.0.1:${server.port}${SHUTDOWN_PATH}`;
	expect((await fetch(url, { method: "POST", headers: { authorization: "Bearer nope" } })).status).toBe(401);
	expect((await fetch(url, { method: "POST" })).status).toBe(401);
	const file = readServiceFile();
	if (!file) throw new Error("no jocasta-service.json");
	expect(await askShutdown(file)).toBe(true);
	await server.stopped;
	expect(asked).toEqual([]);
	expect(reattached()).toBe(0);
	expect(steps).toHaveLength(1);
	expect(server.herdr.of("stopServer")).toHaveLength(1);
	// Nobody answers now: bare jocasta stops herdr and the containers itself.
	expect(await askShutdown(file)).toBe(false);
});

test(
	"his client closing while a thread works opens a terminal on the session and asks there; Cancel keeps all, Stop stops all (0549)",
	async () => {
		await demoProject(env.root);
		const { server, asked, steps, reattached } = await guarded([false, true]);
		try {
			const paneId = await startThread(server);
			server.herdr.setState(paneId, "working");
			await until(() => server.service.busyThreads().working === 1);
			expect(await server.clientClosed()).toBe("kept");
			expect(reattached()).toBe(1);
			expect(asked).toEqual(["Stop 1 working thread? They pick up where they left off next time."]);
			expect(server.herdr.of("stopServer")).toHaveLength(0);
			expect(steps).toEqual([]);
			expect(await server.clientClosed()).toBe("stopped");
			expect(reattached()).toBe(2);
			expect(server.herdr.of("stopServer")).toHaveLength(1);
			expect(steps).toHaveLength(1);
		} finally {
			await server.stop();
		}
	},
	GIT_TEST_TIMEOUT,
);
