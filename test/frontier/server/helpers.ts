import { afterAll, expect } from "bun:test";
import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { ClientMessage, ServerMessage } from "../../../src/frontier/protocol.ts";
import { reviewPrompt } from "../../../src/frontier/server/hunk.ts";
import { overlayFile } from "../../../src/frontier/server/overlay.ts";
import { type JocastaServer, type ServerOptions, startServer } from "../../../src/frontier/server/server.ts";
import { HANDOFF_PROMPT } from "../../../src/frontier/server/wording.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
import { dataDir, projectDir } from "../../../src/shared/paths.ts";
import { addProject, type Project } from "../../../src/shared/projects.ts";
import { openCatalog } from "../../../src/shared/schema.ts";
import { copyRepo, poll, sh } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";

/** A registered git project named `demo`: a copy of the name's template repo (TST-4). */
export async function demoProject(root: string, name = "demo"): Promise<Project> {
	const repo = copyRepo(join(root, name));
	const catalog = openCatalog();
	try {
		return await addProject(catalog, repo);
	} finally {
		catalog.close();
	}
}

/** Writes Jocasta settings (`jocasta.yaml`; JSON is YAML) in the temp config dir. */
export function writeSettings(configDir: string, settings: Record<string, unknown>): void {
	mkdirSync(configDir, { recursive: true });
	writeFileSync(join(configDir, "jocasta.yaml"), JSON.stringify(settings));
}

/** A page's WebSocket, as `today` opens it: with the service token as a bearer. */
export class Page {
	readonly received: ServerMessage[] = [];
	private waiters: { match: (m: ServerMessage) => boolean; resolve: (m: ServerMessage) => void }[] = [];
	private constructor(private readonly ws: WebSocket) {
		ws.addEventListener("message", (event) => {
			const message = JSON.parse(String(event.data)) as ServerMessage;
			this.received.push(message);
			this.waiters = this.waiters.filter((w) => {
				if (!w.match(message)) return true;
				w.resolve(message);
				return false;
			});
		});
	}

	static async open(server: JocastaServer, query = ""): Promise<Page> {
		const ws = new WebSocket(`ws://127.0.0.1:${server.port}/ws${query}`, {
			headers: { authorization: `Bearer ${server.token}` },
		} as unknown as string[]);
		const page = new Page(ws);
		const { promise, resolve, reject } = Promise.withResolvers<void>();
		ws.addEventListener("open", () => resolve(), { once: true });
		ws.addEventListener("error", () => reject(new Error("WebSocket refused")), { once: true });
		await promise;
		return page;
	}

	send(message: ClientMessage): void {
		this.ws.send(JSON.stringify(message));
	}

	/** The first message (already received or coming) matching `match`. */
	waitFor<T extends ServerMessage>(match: (m: ServerMessage) => m is T, timeoutMs?: number): Promise<T>;
	waitFor(match: (m: ServerMessage) => boolean, timeoutMs?: number): Promise<ServerMessage>;
	waitFor(match: (m: ServerMessage) => boolean, timeoutMs = 15_000): Promise<ServerMessage> {
		const seen = this.received.find(match);
		if (seen) return Promise.resolve(seen);
		const { promise, resolve, reject } = Promise.withResolvers<ServerMessage>();
		const timer = setTimeout(() => reject(new Error("timed out waiting for a server message")), timeoutMs);
		this.waiters.push({
			match,
			resolve: (m) => {
				clearTimeout(timer);
				resolve(m);
			},
		});
		return promise;
	}

	close(): void {
		this.ws.close();
	}
}

/** The boss key test servers run with (0484); `call` sends it from the boss pane. */
export const TEST_BOSS_KEY = "test-boss-key";

/** A test server with its fake herdr. */
export type TestServer = JocastaServer & { herdr: FakeHerdr };

/**
 * The collab relay in a test (0561): a free port (never his Jocasta's), every pane's relay resolved as loopback without
 * his omp, and an empty host registry polled only when a test asks for it.
 */
export function relayTestOptions(): Pick<ServerOptions, "relayPort" | "relayUrlOf" | "hostsDir" | "hostPollMs"> {
	return {
		relayPort: 0,
		relayUrlOf: async () => "ws://127.0.0.1:47499",
		hostsDir: join(process.env.JOCASTA_DATA_DIR ?? "", "collab-hosts"),
		hostPollMs: 60_000,
	};
}

/**
 * Starts a server for tests: a fake herdr (`options.herdr`'s when it is one, so a test can restart the service on
 * the same panes), no knowledge graph (a test never starts Graphiti's containers), pollers off (never the real gh or
 * az), a free port (never `service.port`), an empty sessions dir, `cmd`-free commands, the boss key
 * `TEST_BOSS_KEY`, no terminal opened for the close question, every confirmation answered Allow (tests of Deny and of
 * the wait pass their own `confirm`), no steer for a missing report (its test passes `askReport: undefined`), and the
 * boss's `jocasta` MCP entry as setup writes it, so no start warns that the boss has no tools (TST-14).
 */
export async function startTestServer(options: ServerOptions = {}): Promise<TestServer> {
	const herdr = options.herdr instanceof FakeHerdr ? options.herdr : new FakeHerdr();
	const data = process.env.JOCASTA_DATA_DIR ?? "";
	const sessionsDir = join(data, "omp-sessions");
	mkdirSync(sessionsDir, { recursive: true });
	const bossOmp = join(data, "home", ".omp");
	mkdirSync(bossOmp, { recursive: true });
	writeFileSync(
		join(bossOmp, "mcp.json"),
		JSON.stringify({ mcpServers: { jocasta: { type: "stdio", command: "jocasta", args: ["mcp", "--boss"] } } }),
	);
	const server = await startServer({
		herdr,
		graph: null,
		day: { pollers: false },
		port: 0,
		sessionsDir,
		agentWaitMs: 0,
		submitWaitMs: 0,
		refreshMs: 60_000,
		whiteboard: null,
		panePrompt: null,
		// Never his mise: a test pane's env is its own.
		mise: null,
		// Never his omp: the minute tick starts no weekly review (0470); a review would spawn a real `omp -p`.
		weeklyReviews: false,
		askReport: false,
		// Never his Docker: a test closing Jocasta stops no containers.
		stopContainers: async () => {},
		bossKey: TEST_BOSS_KEY,
		confirm: async () => ({ allow: true, how: "answered" }),
		reattach: () => {},
		...relayTestOptions(),
		...options,
	});
	// The service starts after the server listens; tests act on a started one.
	await server.ready;
	return Object.assign(server, { herdr });
}

/** Runs `fn` against a test server and always stops it, so the temp dir can be removed (EBUSY on Windows). */
export async function withServer(
	fn: (server: TestServer) => Promise<void>,
	options: ServerOptions = {},
): Promise<void> {
	const server = await startTestServer(options);
	try {
		await fn(server);
	} finally {
		await server.stop();
	}
}

/** Polls until `check` passes or the time runs out, which throws. */
export async function until(check: () => boolean | Promise<boolean>, timeoutMs = 15_000): Promise<void> {
	await poll(check, { timeoutMs, what: "the test's condition" });
}

/** hunk as the service starts it in these tests: the fake binary, run by Bun. */
export const HUNK = [process.execPath, join(import.meta.dir, "fake-hunk.ts")];

/**
 * Jocasta's hunk session daemon as the service finds it: a /health that answers, on a free port, so no daemon is ever
 * started (never hunk's or Jocasta's real port); one per test file, stopped after its tests. The service's hunk
 * options with it.
 */
export function useHunkDaemon(): { hunk: string[]; hunkPort: number } {
	const daemon = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => Response.json({ ok: true }) });
	afterAll(() => daemon.stop(true));
	return { hunk: HUNK, hunkPort: daemon.port as number };
}

/** The guard every pane loads before its own settings (0450). */
export const GUARD = ["--config", join(shippedConfigDir(), "guard.yml")];

/** The brief `startThread` gives. */
export const BRIEF = "Add a README line.";

/** The task file folder (0540) of the project's open thread titled `title`, the newest when several. */
export function taskDirOf(projectId: string, title = "Readme line"): string {
	const root = join(projectDir(projectId), "threads");
	let found: { dir: string; at: number } | undefined;
	for (const id of existsSync(root) ? readdirSync(root) : []) {
		const file = join(root, id, "thread.json");
		if (!existsSync(file)) continue;
		const record = JSON.parse(readFileSync(file, "utf8")) as { title: string; createdAt: number; closed: boolean };
		if (record.title === title && !record.closed && (!found || record.createdAt > found.at)) {
			found = { dir: join(root, id), at: record.createdAt };
		}
	}
	if (!found) throw new Error(`no task file for ${title}`);
	return found.dir;
}

/** A thread pane's omp as started again: its task's launch prompt where the shipped prompt goes (0540). */
export function threadOmp(projectId: string, title = "Readme line"): string[] {
	const prompt = join(taskDirOf(projectId, title), "prompt.md");
	return [
		"omp",
		"--continue",
		"--append-system-prompt",
		prompt,
		...GUARD,
		"--warn-cost",
		"50",
		"--config",
		overlayFile(projectId),
	];
}

/** The review prompt of a thread with its recorded brief (null: one found in herdr without one) and no report. */
export function review(range: string, brief: string | null = BRIEF): string {
	return reviewPrompt(HUNK, range, { rules: [], ...(brief ? { brief } : {}), report: null });
}

/** The boss pane: workspace Jocasta's pane. */
export function bossPane(herdr: FakeHerdr): string | undefined {
	const jocasta = herdr.workspaceList.find((w) => w.label === "Jocasta")?.id;
	return herdr.paneList.find((p) => p.workspaceId === jocasta)?.paneId;
}

/**
 * `jocasta mcp`'s call: `POST /api/mcp/<tool>` with the service token, as `src/mcp/server.ts` sends it, from the boss
 * pane with its key unless `paneId` says another pane (null: none); the pane's own key (0578) is what names it, read
 * from the env herdr started it with; `bossKey` the key sent (null: none).
 */
export async function call(
	server: TestServer,
	tool: string,
	args: Record<string, unknown>,
	paneId: string | null = bossPane(server.herdr) ?? null,
	bossKey: string | null = paneId !== null && paneId === bossPane(server.herdr) ? TEST_BOSS_KEY : null,
): Promise<{ text: string; isError?: boolean }> {
	const paneKey = paneId ? server.herdr.envOf(paneId)?.JOCASTA_PANE_KEY : undefined;
	const response = await fetch(`http://127.0.0.1:${server.port}/api/mcp/${tool}`, {
		method: "POST",
		headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
		body: JSON.stringify({ args, ...(paneKey ? { paneKey } : {}), ...(bossKey ? { bossKey } : {}) }),
	});
	return (await response.json()) as { text: string; isError?: boolean };
}

/** The thread variables of the project's first thread, `Readme line` (0457). */
export const README_THREAD_ENV = {
	JOCASTA_THREAD: "readme-line",
	COMPOSE_PROJECT_NAME: "demo-readme-line",
	JOCASTA_PORT_OFFSET: "10",
};

/** An omp session file in omp's shape (title, session header, assistant messages). */
export function sessionFile(
	cwd: string,
	messages: { text: string; cost: number; ask?: string; bash?: { command: string; ok: boolean } }[],
): string {
	const dir = join(dataDir(), "omp-sessions", "--demo--");
	mkdirSync(dir, { recursive: true });
	// Each file its own session and message ids, as omp writes them: copies of one session count once.
	const stem = Math.random().toString(16).slice(2);
	const file = join(dir, `2026-10-13T10-00-00-000Z_${stem}.jsonl`);
	const now = Date.now();
	const lines = [
		{ type: "title", v: 1, title: "Readme line" },
		{ type: "session", id: `s-${stem}`, cwd, timestamp: new Date(now).toISOString() },
		...messages.flatMap((m, i) => [
			{
				type: "message",
				id: `${stem}-m${i}`,
				message: {
					role: "assistant",
					provider: "anthropic",
					model: "claude-opus-5-5",
					timestamp: now,
					stopReason: "stop",
					usage: { cost: { total: m.cost } },
					content: [
						{ type: "text", text: m.text },
						...(m.ask
							? [{ type: "toolCall", id: "c1", name: "ask", arguments: { questions: [{ question: m.ask }] } }]
							: []),
						...(m.bash
							? [{ type: "toolCall", id: `b${i}`, name: "bash", arguments: { command: m.bash.command } }]
							: []),
					],
				},
			},
			// The bash call's result: exit 0, or an error result with its exit code (omp `tools/bash.ts`).
			...(m.bash
				? [
						{
							type: "message",
							id: `r${i}`,
							message: {
								role: "toolResult",
								toolCallId: `b${i}`,
								toolName: "bash",
								timestamp: now,
								content: [{ type: "text", text: "ok" }],
								...(m.bash.ok ? {} : { isError: true, details: { exitCode: 1 } }),
							},
						},
					]
				: []),
		]),
	];
	writeFileSync(file, `${lines.map((l) => JSON.stringify(l)).join("\n")}\n`);
	return file;
}

/**
 * Starts a thread through the MCP endpoint. Its pane's omp reports idle as herdr makes it (TST-2), or, `late`, only
 * once herdr made the worktree, while `thread_start` already polls for it (agentUp's wait stays covered).
 */
export async function startThread(
	server: TestServer,
	options: { project?: string; title?: string; brief?: string; late?: boolean } = {},
): Promise<{ paneId: string; path: string; reply: string }> {
	const before = server.herdr.of("createWorktree").length;
	server.herdr.ompUpOnCreate = options.late ? undefined : "idle";
	const replied = call(server, "thread_start", {
		project: options.project ?? "demo",
		title: options.title ?? "Readme line",
		brief: options.brief ?? BRIEF,
	});
	try {
		await until(() => server.herdr.of("createWorktree").length === before + 1);
		const pane = server.herdr.paneList.at(-1);
		if (!pane?.cwd) throw new Error("no thread pane");
		if (options.late) server.herdr.setState(pane.paneId, "idle");
		const reply = await replied;
		return { paneId: pane.paneId, path: pane.cwd, reply: reply.text };
	} finally {
		server.herdr.ompUpOnCreate = undefined;
	}
}

/** A pane's env with its pane key (0578), whatever key it got. */
export function keyed(vars: Record<string, string>): Record<string, string> {
	return { ...vars, JOCASTA_PANE_KEY: expect.any(String) as unknown as string };
}

/** omp's proposal of a plan through `xd://propose`, its turn silently aborted (omp 18.10's shape), appended to `file`. */
export function appendProposal(file: string): void {
	const entries = [
		{
			type: "message",
			id: "t1",
			message: {
				role: "toolResult",
				toolCallId: "c9",
				toolName: "write",
				content: [{ type: "text", text: "Plan ready for review." }],
				details: {
					xdev: {
						tool: "propose",
						mode: "execute",
						args: { title: "readme-line" },
						inner: { planFilePath: "local://readme-line-plan.md", title: "readme-line", planExists: true },
					},
				},
				isError: false,
			},
		},
		{ type: "message", id: "a9", message: { role: "assistant", content: [], stopReason: "aborted" } },
		{ type: "mode_change", id: "m2", mode: "plan", data: { planFilePath: "local://readme-line-plan.md" } },
	];
	appendFileSync(file, `${entries.map((e) => JSON.stringify(e)).join("\n")}\n`);
}

/** A plan-mode session with no plan proposed yet. */
export function planModeFile(cwd: string): string {
	const file = sessionFile(cwd, [{ text: "I wrote the plan.", cost: 0.25 }]);
	const entry = { type: "mode_change", id: "m1", mode: "plan", data: { planFilePath: "local://readme-line-plan.md" } };
	appendFileSync(file, `${JSON.stringify(entry)}\n`);
	return file;
}

/** A plan-mode session in omp 18.10's shape, its plan proposed through `xd://propose` and the turn silently aborted. */
export function proposedPlanFile(cwd: string): string {
	const file = planModeFile(cwd);
	appendProposal(file);
	return file;
}

/** A commit on the thread's branch, as its task would make. */
export function commitIn(path: string, text: string, message: string): void {
	writeFileSync(join(path, "README.md"), text);
	sh(path, "commit", "-qam", message);
}

/** A `/handoff` entry in omp's shape, as the watcher and `handedOff` read it. */
export function handoffEntry(file: string): void {
	appendFileSync(
		file,
		`${JSON.stringify({ type: "compaction", id: "h1", method: "handoff", summary: "Handed off.", timestamp: new Date().toISOString() })}\n`,
	);
}

/** Runs a thread's task to its `/handoff`: a commit, the review turn on its test line, then `/handoff` sent. */
export async function toHandoff(server: TestServer, paneId: string, path: string): Promise<string> {
	const range = "main...feature/readme-line";
	const prompts = () => server.herdr.of("prompt");
	server.herdr.setState(paneId, "working");
	commitIn(path, "demo\nA line.\n", "Add a README line");
	server.herdr.setState(paneId, "idle");
	await until(() => prompts().some(([p, t]) => p === paneId && t === review(range)));
	const file = sessionFile(path, [
		{ text: "Reviewed.\n\n**TESTS: pass — bun test**", cost: 0.1, bash: { command: "bun test", ok: true } },
	]);
	server.herdr.setState(paneId, "working", file);
	server.herdr.setState(paneId, "idle");
	await until(() => prompts().some(([p, t]) => p === paneId && t === HANDOFF_PROMPT));
	return file;
}

/** Restores the thread's pane as a shell, lets the service relaunch it, and brings its new omp up idle without an event. */
export async function relaunchIdle(
	server: TestServer,
	herdr: FakeHerdr,
	path: string,
	session?: string,
): Promise<void> {
	await server.service.refresh();
	herdr.restore();
	await server.service.refresh();
	const fresh = herdr.paneList.find((p) => resolve(p.cwd ?? "") === path)?.paneId as string;
	herdr.setState(fresh, "idle", session, false);
	await server.service.refresh();
	await server.service.refresh();
}

/** The boss pane's `[event]` lines so far, one per line. */
export function bossLines(server: TestServer): string[] {
	const boss = bossPane(server.herdr);
	return server.herdr
		.of("prompt")
		.filter(([p]) => p === boss)
		.flatMap(([, t]) => String(t).split("\n"));
}

/** The boss answered and is idle again: lines waiting for it go in. */
export function bossIdle(server: TestServer): void {
	const boss = bossPane(server.herdr) as string;
	server.herdr.setState(boss, "working");
	server.herdr.setState(boss, "idle");
}

/** `jocasta thread|worktree`'s call: `POST /api/cli` with the service token, as `src/shared/thread-cli.ts` sends it. */
export async function cli(
	server: TestServer,
	command: string,
	args: Record<string, unknown> = {},
): Promise<{ text: string; isError?: boolean }> {
	const response = await fetch(`http://127.0.0.1:${server.port}/api/cli`, {
		method: "POST",
		headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
		body: JSON.stringify({ command, args }),
	});
	return (await response.json()) as { text: string; isError?: boolean };
}

/** The thread pane in worktree `path` (not its hunk pane beside it). */
export function threadPane(herdr: FakeHerdr, path: string): string {
	return herdr.paneList.find((p) => resolve(p.cwd ?? "") === path && p.label !== "review")?.paneId as string;
}
