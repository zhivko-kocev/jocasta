import { afterEach, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { IDLE_CHECK_FRAME } from "../../../src/frontier/server/wording.ts";
import { LAYER_ENV } from "../../../src/shared/layer.ts";
import type { PaneCheckAnswer } from "../../../src/shared/pane-check.ts";
import { sameDir } from "../../../src/shared/pathkey.ts";
import { writeServiceFile } from "../../../src/shared/service-file.ts";
import { GIT_TEST_TIMEOUT, useTempJocasta } from "../../shared/fixture.ts";
import { bossPane, commitIn, demoProject, startThread, type TestServer, until, withServer } from "./helpers.ts";

const env = useTempJocasta();
const ENTRY = join(import.meta.dir, "..", "..", "..", "src", "entry.ts");
const MISE = [process.execPath, join(import.meta.dir, "fake-mise-verify.ts")];

afterEach(() => {
	delete process.env.FAKE_VERIFY_DIR;
	delete process.env[LAYER_ENV];
});

/** What a test of a project pane's check works with. */
interface Fixture {
	server: TestServer;
	/** The project pane, in the project's main checkout. */
	pane: string;
	/** The thread started in the project: its pane is no project pane. */
	thread: string;
	/** The project's main checkout. */
	main: string;
	/** The fake `verify` task's controls: `exit` and `delay` files, read when a run starts. */
	control(name: "exit" | "delay", value: number): void;
	/** How many times `verify` ran. */
	runs(): number;
	/** How many runs started while another was under way. */
	overlaps(): number;
	/** The pane's check as the extension's role asks for it (`POST /api/pane-check`); "" for nothing. `signal` ends the ask. */
	ask(pane: string, signal?: AbortSignal): Promise<string>;
}

/** A service over a demo project with a thread (so the project pane exists) and a `verify` task that passes. */
async function withProjectPane(fn: (f: Fixture) => Promise<void>, options: { waitMs?: number } = {}): Promise<void> {
	const project = await demoProject(env.root);
	const dir = join(env.root, "verify");
	mkdirSync(dir);
	process.env.FAKE_VERIFY_DIR = dir;
	await withServer(
		async (server) => {
			const { paneId } = await startThread(server);
			const pane = server.herdr.paneList.find((p) => p.cwd && sameDir(p.cwd, project.mainPath))?.paneId as string;
			await fn({
				server,
				pane,
				thread: paneId,
				main: project.mainPath,
				control: (name, value) => writeFileSync(join(dir, name), String(value)),
				runs: () => lineCount(join(dir, "runs")),
				overlaps: () => lineCount(join(dir, "overlaps")),
				ask: async (target, signal) => {
					const paneKey = server.herdr.envOf(target)?.JOCASTA_PANE_KEY ?? "no-such-key";
					const response = await fetch(`http://127.0.0.1:${server.port}/api/pane-check`, {
						method: "POST",
						headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
						body: JSON.stringify({ paneKey }),
						...(signal ? { signal } : {}),
					});
					return ((await response.json()) as PaneCheckAnswer).text;
				},
			});
		},
		{
			mise: MISE,
			...(options.waitMs ? { paneCheckWaitMs: options.waitMs } : {}),
		},
	);
}

/** How many lines `file` has; 0 when it is absent. */
function lineCount(file: string): number {
	try {
		return readFileSync(file, "utf8").trim().split("\n").length;
	} catch {
		return 0;
	}
}

/** Edits the project's README, so its tree differs from the last one. */
function edit(main: string, text: string): void {
	writeFileSync(join(main, "README.md"), `demo\n${text}\n`);
}

const todayLines = (server: TestServer) => server.service.day.dashboard().activity.events.map((e) => e.text);

test(
	"a project pane's turn end with edits not yet checked gets the failing check in the threads' frame, once per tree; a pass and a tree with no edits say nothing and cost no run (0617)",
	async () => {
		await withProjectPane(async (f) => {
			// No edits since HEAD: nothing to check.
			expect(await f.ask(f.pane)).toBe("");
			expect(f.runs()).toBe(0);

			f.control("exit", 1);
			edit(f.main, "one");
			const text = await f.ask(f.pane);
			expect(text).toStartWith(`${IDLE_CHECK_FRAME}\n\nchecks: FAIL`);
			expect(f.runs()).toBe(1);
			// The event is called again after every settle, the continuation's included: the same tree is never answered twice.
			expect(await f.ask(f.pane)).toBe("");
			expect(f.runs()).toBe(1);

			// A pass says nothing; the tree is not checked again.
			f.control("exit", 0);
			edit(f.main, "two");
			expect(await f.ask(f.pane)).toBe("");
			expect(await f.ask(f.pane)).toBe("");
			expect(f.runs()).toBe(2);

			// Committed: the tree is HEAD's.
			commitIn(f.main, "demo\ntwo\n", "Add two");
			expect(await f.ask(f.pane)).toBe("");
			expect(f.runs()).toBe(2);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a thread's key, the boss's key and an unknown key get nothing and run nothing (0617)",
	async () => {
		await withProjectPane(async (f) => {
			f.control("exit", 1);
			edit(f.main, "one");
			expect(await f.ask(f.thread)).toBe("");
			expect(await f.ask(bossPane(f.server.herdr) as string)).toBe("");
			expect(await f.ask("no such pane")).toBe("");
			expect(f.runs()).toBe(0);
			// The project pane itself still gets it.
			expect(await f.ask(f.pane)).toStartWith(IDLE_CHECK_FRAME);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"with the layer off a project pane's check runs nothing and answers nothing (0576, 0617)",
	async () => {
		process.env[LAYER_ENV] = "off";
		await withProjectPane(async (f) => {
			f.control("exit", 1);
			edit(f.main, "one");
			expect(await f.ask(f.pane)).toBe("");
			expect(f.runs()).toBe(0);
		});
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a check that outlasts the wait lets the turn end; its failure reaches today and is owed once to the next request on the same tree, and dropped when the tree changed (0617)",
	async () => {
		await withProjectPane(
			async (f) => {
				f.control("exit", 1);
				f.control("delay", 1500);
				edit(f.main, "one");
				expect(await f.ask(f.pane)).toBe("");
				await until(() => f.runs() === 1);
				// Called again while the check runs: the same tree, no second run, nothing owed yet.
				expect(await f.ask(f.pane)).toBe("");
				expect(f.runs()).toBe(1);
				const failedLate =
					/^demo — checks failed on tree [0-9a-f]{6} after its turn ended; its agent gets them at its next turn end$/;
				await until(() => todayLines(f.server).some((line) => failedLate.test(line)));
				expect(todayLines(f.server).filter((line) => failedLate.test(line))).toHaveLength(1);
				expect(await f.ask(f.pane)).toStartWith(`${IDLE_CHECK_FRAME}\n\nchecks: FAIL`);
				expect(await f.ask(f.pane)).toBe("");
				expect(f.runs()).toBe(1);

				// Slow again, and the tree changes before it ends: the owed failure is for a tree no longer there.
				edit(f.main, "two");
				expect(await f.ask(f.pane)).toBe("");
				await until(() => f.runs() === 2);
				f.control("exit", 0);
				f.control("delay", 0);
				edit(f.main, "three");
				await until(() => todayLines(f.server).filter((line) => failedLate.test(line)).length === 2);
				expect(await f.ask(f.pane)).toBe("");
				expect(await f.ask(f.pane)).toBe("");
			},
			{ waitMs: 200 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a turn end on a new tree while the pane's last check still runs waits for it: two checks never run at once in one folder (0617)",
	async () => {
		await withProjectPane(
			async (f) => {
				f.control("exit", 1);
				f.control("delay", 1500);
				edit(f.main, "one");
				expect(await f.ask(f.pane)).toBe("");
				await until(() => f.runs() === 1);
				edit(f.main, "two");
				expect(await f.ask(f.pane)).toBe("");
				expect(f.runs()).toBe(1);
				await until(() => f.runs() === 2);
				await until(() => todayLines(f.server).filter((line) => line.includes("checks failed")).length === 2);
				expect(f.overlaps()).toBe(0);
			},
			{ waitMs: 200 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a role that stops asking mid-wait ends the service's wait; the failure then reaches today and is owed to the next turn end on the tree (0617)",
	async () => {
		await withProjectPane(
			async (f) => {
				f.control("exit", 1);
				f.control("delay", 1500);
				edit(f.main, "one");
				const role = new AbortController();
				const asked = f.ask(f.pane, role.signal).catch(() => "stopped asking");
				await until(() => f.runs() === 1);
				role.abort();
				expect(await asked).toBe("stopped asking");
				// Within the 60 s wait: had the service not seen the role leave, the failure would go to the gone request and today would say nothing.
				await until(() => todayLines(f.server).some((line) => line.includes("checks failed")));
				expect(await f.ask(f.pane)).toStartWith(`${IDLE_CHECK_FRAME}\n\nchecks: FAIL`);
				expect(f.runs()).toBe(1);
			},
			{ waitMs: 60_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"stopping the service ends a pane check's wait at once and writes no today line (0617)",
	async () => {
		await withProjectPane(
			async (f) => {
				f.control("exit", 1);
				f.control("delay", 20_000);
				edit(f.main, "one");
				const asked = f.ask(f.pane).catch(() => "");
				await until(() => f.runs() === 1);
				const started = Date.now();
				await f.server.stop();
				expect(Date.now() - started).toBeLessThan(10_000);
				expect(await asked).toBe("");
				expect(todayLines(f.server).some((line) => line.includes("checks failed"))).toBe(false);
			},
			{ waitMs: 18_000 },
		);
	},
	GIT_TEST_TIMEOUT,
);

/** Runs `jocasta pane-check` as the extension does, in the temp data folder, with the pane key given; its stdout and exit code. */
async function role(paneKey: string): Promise<{ out: string; code: number }> {
	const run = Bun.spawn([process.execPath, ENTRY, "pane-check"], {
		env: { ...process.env, JOCASTA_PANE_KEY: paneKey },
		stdout: "pipe",
		stderr: "ignore",
	});
	const [out, code] = await Promise.all([new Response(run.stdout).text(), run.exited]);
	return { out, code };
}

test(
	"jocasta pane-check prints the service's text for the pane's key, and prints nothing and exits 0 when no service runs (0617)",
	async () => {
		expect(await role("any-key")).toEqual({ out: "", code: 0 });
		await withProjectPane(async (f) => {
			writeServiceFile({ port: f.server.port, token: f.server.token, pid: process.pid });
			const key = f.server.herdr.envOf(f.pane)?.JOCASTA_PANE_KEY as string;
			f.control("exit", 1);
			edit(f.main, "one");
			const said = await role(key);
			expect(said.code).toBe(0);
			expect(said.out).toStartWith(`${IDLE_CHECK_FRAME}\n\nchecks: FAIL`);
			expect(await role(key)).toEqual({ out: "", code: 0 });
		});
	},
	GIT_TEST_TIMEOUT,
);
