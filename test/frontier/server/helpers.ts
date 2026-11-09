import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type ClientMessage, type ServerMessage, TOKEN_COOKIE } from "../../../src/frontier/protocol.ts";
import { type FrontierServer, type ServerOptions, startServer } from "../../../src/frontier/server/server.ts";
import { addProject, type Project } from "../../../src/shared/projects.ts";
import { openCatalog } from "../../../src/shared/schema.ts";
import { makeRepo } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";

/** A registered git project named `demo`. */
export function demoProject(root: string, name = "demo"): Project {
	const repo = makeRepo(join(root, name));
	const catalog = openCatalog();
	try {
		return addProject(catalog, repo);
	} finally {
		catalog.close();
	}
}

/** Writes Jocasta settings (`jocasta.yaml`; JSON is YAML) in the temp config dir. */
export function writeSettings(configDir: string, settings: Record<string, unknown>): void {
	mkdirSync(configDir, { recursive: true });
	writeFileSync(join(configDir, "jocasta.yaml"), JSON.stringify(settings));
}

/** A page's WebSocket, as the browser would open it: with the token cookie. */
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

	static async open(server: FrontierServer, query = ""): Promise<Page> {
		const ws = new WebSocket(`ws://127.0.0.1:${server.port}/ws${query}`, {
			headers: { cookie: `${TOKEN_COOKIE}=${server.token}` },
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
export type TestServer = FrontierServer & { herdr: FakeHerdr };

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
 * az), a free port (never `frontier.port`), an empty sessions dir, `cmd`-free commands, the boss key
 * `TEST_BOSS_KEY`, no terminal opened for the close question, every confirmation answered Allow (tests of Deny and of
 * the wait pass their own `confirm`), and no steer for a missing report (its test passes `askReport: undefined`).
 */
export async function startTestServer(options: ServerOptions = {}): Promise<TestServer> {
	const herdr = options.herdr instanceof FakeHerdr ? options.herdr : new FakeHerdr();
	const sessionsDir = join(process.env.JOCASTA_DATA_DIR ?? "", "omp-sessions");
	mkdirSync(sessionsDir, { recursive: true });
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
		confirm: async () => true,
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

/** Polls until `check` passes or the time runs out. */
export async function until(check: () => boolean, timeoutMs = 15_000): Promise<void> {
	const end = Date.now() + timeoutMs;
	while (!check()) {
		if (Date.now() > end) throw new Error("condition not met in time");
		await Bun.sleep(20);
	}
}
