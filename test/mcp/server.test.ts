import { afterEach, expect, test } from "bun:test";
import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { PassThrough } from "node:stream";
import { MCP_PROTOCOL_VERSION, type McpServerOptions, runMcpServer } from "../../src/mcp/server.ts";
import { NOT_RUNNING } from "../../src/shared/service-file.ts";
import { useTempDirs } from "../shared/fixture.ts";

const tempDir = useTempDirs("jocasta-mcp-");

const servers: { stop(force?: boolean): unknown }[] = [];
afterEach(() => {
	for (const s of servers.splice(0)) s.stop(true);
});

/** A parsed reply; `result` lists the fields these tests read. */
interface Reply {
	jsonrpc: string;
	id: unknown;
	result?: {
		[key: string]: unknown;
		tools?: { name: string; description: string; inputSchema: unknown }[];
		content?: { type: string; text: string }[];
		nextCursor?: string;
	};
	error?: { code: number; message: string };
}

/** A thread pane's environment (0577): `report_result` and `check` are listed there. */
const THREAD_ENV = { JOCASTA_THREAD: "t-1" };

/** Feeds `lines` (raw text each, a newline appended) to a server, outside a thread unless `options.env` says so. */
async function session(lines: string[], options: Partial<McpServerOptions> = {}): Promise<Reply[]> {
	const out: string[] = [];
	const input = new ReadableStream<Uint8Array>({
		start(controller) {
			for (const line of lines) controller.enqueue(new TextEncoder().encode(`${line}\n`));
			controller.close();
		},
	});
	await runMcpServer({
		boss: false,
		input,
		output: (line) => out.push(line),
		service: () => undefined,
		env: {},
		...options,
	});
	for (const line of out) expect(line.includes("\n")).toBe(false);
	return out.map((line) => JSON.parse(line));
}

const request = (id: number, method: string, params?: unknown) =>
	JSON.stringify({ jsonrpc: "2.0", id, method, ...(params === undefined ? {} : { params }) });

/** omp's handshake (`packages/coding-agent/src/mcp/client.ts:99-125`). */
const HANDSHAKE = [
	request(1, "initialize", {
		protocolVersion: MCP_PROTOCOL_VERSION,
		capabilities: { roots: { listChanged: false } },
		clientInfo: { name: "omp", version: "18.10.3" },
	}),
	JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }),
];

function fakeService(answer: (tool: string, body: unknown, auth: string | null) => Response | Promise<Response>) {
	const calls: { tool: string; body: unknown; auth: string | null }[] = [];
	const server = Bun.serve({
		port: 0,
		hostname: "127.0.0.1",
		async fetch(req) {
			const url = new URL(req.url);
			const tool = url.pathname.replace("/api/mcp/", "");
			const body = await req.json();
			const auth = req.headers.get("authorization");
			calls.push({ tool, body, auth });
			return answer(tool, body, auth);
		},
	});
	servers.push(server);
	return { address: { port: server.port as number, token: "tok-1" }, calls };
}

test("initialize answers omp's protocol version and a tools capability; the notification gets no reply", async () => {
	const replies = await session([...HANDSHAKE, request(2, "ping")]);
	expect(replies).toEqual([
		{
			jsonrpc: "2.0",
			id: 1,
			result: {
				protocolVersion: "2025-11-25",
				capabilities: { tools: { listChanged: false } },
				serverInfo: { name: "jocasta", version: expect.any(String) },
			},
		},
		{ jsonrpc: "2.0", id: 2, result: {} },
	]);
});

test("initialize echoes an older supported version and offers its own for an unknown one", async () => {
	const replies = await session([
		request(1, "initialize", { protocolVersion: "2025-06-18" }),
		request(2, "initialize", { protocolVersion: "1999-01-01" }),
	]);
	expect(replies.map((r) => r.result?.protocolVersion)).toEqual(["2025-06-18", MCP_PROTOCOL_VERSION]);
});

test("tools/list: graph_search everywhere, report_result and check only in a thread with the layer on, the boss's tools only with --boss; the boss's graph_search names the project", async () => {
	const listed = async (options: Partial<McpServerOptions>) => {
		const [reply] = await session([request(1, "tools/list", {})], options);
		const tools = reply?.result?.tools ?? [];
		for (const t of tools) expect(t.inputSchema).toMatchObject({ type: "object", additionalProperties: false });
		expect(reply?.result?.nextCursor).toBeUndefined();
		return tools;
	};
	const thread = { env: THREAD_ENV };
	const boss = { boss: true };
	const names = async (options: Partial<McpServerOptions>) => (await listed(options)).map((t) => t.name);
	const graphSearch = async (options: Partial<McpServerOptions>) =>
		(await listed(options)).find((t) => t.name === "graph_search")?.inputSchema;
	expect(await graphSearch(thread)).toMatchObject({ required: ["query"] });
	expect(await graphSearch(thread)).not.toHaveProperty("properties.project");
	expect(await graphSearch(boss)).toMatchObject({ required: ["project", "query"] });
	expect(await names(thread)).toEqual(["graph_search", "report_result", "check"]);
	// His other omp sessions and the project panes load the same server (0577), and the layer can be off (0576).
	expect(await names({})).toEqual(["graph_search"]);
	expect(await names({ env: { ...THREAD_ENV, JOCASTA_LAYER: "off" } })).toEqual(["graph_search"]);
	expect(await names({ ...boss, env: THREAD_ENV })).not.toContain("check");
	// report_result's schema (0539): the outcome, a summary of at most 200 characters, the lists.
	expect((await listed(thread)).find((t) => t.name === "report_result")?.inputSchema).toMatchObject({
		required: ["outcome", "summary", "files", "tests", "decisions", "questions", "followUps"],
		properties: {
			summary: { maxLength: 200 },
			outcome: { anyOf: [{ const: "done" }, { const: "blocked" }, { const: "failed" }] },
		},
	});
	expect((await listed(boss)).find((t) => t.name === "thread_start")?.inputSchema).toMatchObject({
		properties: { done_when: { type: "array", items: { type: "string" } } },
		required: ["project", "title", "brief"],
	});
	expect(await names(boss)).toEqual([
		"graph_search",
		"projects",
		"thread_start",
		"thread_status",
		"thread_send",
		"thread_stop",
		"thread_finish",
		"show",
		"today",
		"note",
		"remind",
		"inbox_done",
		"worklog",
		"lesson",
		"big_picture_review",
		"worktree_remove",
	]);
});

test("outside a thread, report_result and check are unknown tools", async () => {
	const replies = await session([
		request(1, "tools/call", { name: "check", arguments: {} }),
		request(2, "tools/call", { name: "report_result", arguments: {} }),
	]);
	expect(replies.map((r) => r.error?.code)).toEqual([-32602, -32602]);
});

test("tools/call posts args, pane key and boss key to /api/mcp/<tool> with the bearer token", async () => {
	const fake = fakeService((tool) =>
		Response.json(tool === "thread_stop" ? { text: "No thread named x.", isError: true } : { text: `ok ${tool}` }),
	);
	const replies = await session(
		[
			...HANDSHAKE,
			request(2, "tools/call", {
				name: "graph_search",
				arguments: { project: "demo", query: "why sqlite", limit: 5 },
			}),
			request(3, "tools/call", {
				name: "thread_start",
				arguments: { project: "demo", title: "Readme", brief: "Add a line" },
			}),
			request(4, "tools/call", { name: "thread_stop", arguments: { thread: "x" } }),
			request(5, "tools/call", { name: "today" }),
		],
		{ boss: true, service: () => fake.address, paneKey: "pk-7", bossKey: "key-1" },
	);
	const byId = new Map(replies.map((r) => [r.id, r]));
	expect(byId.get(2)?.result).toEqual({ content: [{ type: "text", text: "ok graph_search" }], isError: false });
	expect(byId.get(3)?.result).toEqual({ content: [{ type: "text", text: "ok thread_start" }], isError: false });
	expect(byId.get(4)?.result).toEqual({ content: [{ type: "text", text: "No thread named x." }], isError: true });
	expect(byId.get(5)?.result).toEqual({ content: [{ type: "text", text: "ok today" }], isError: false });
	const sorted = [...fake.calls].sort((a, b) => a.tool.localeCompare(b.tool));
	const from = { paneKey: "pk-7", bossKey: "key-1" };
	expect(sorted).toEqual([
		{
			tool: "graph_search",
			auth: "Bearer tok-1",
			body: { args: { project: "demo", query: "why sqlite", limit: 5 }, ...from },
		},
		{
			tool: "thread_start",
			auth: "Bearer tok-1",
			body: { args: { project: "demo", title: "Readme", brief: "Add a line" }, ...from },
		},
		{ tool: "thread_stop", auth: "Bearer tok-1", body: { args: { thread: "x" }, ...from } },
		{ tool: "today", auth: "Bearer tok-1", body: { args: {}, ...from } },
	]);
});

test("a thread pane's call sends its pane key and no boss key", async () => {
	const fake = fakeService(() => Response.json({ text: "ok" }));
	await session([request(1, "tools/call", { name: "graph_search", arguments: { query: "why" } })], {
		service: () => fake.address,
		paneKey: "pk-9",
	});
	expect(fake.calls.map((c) => c.body)).toEqual([{ args: { query: "why" }, paneKey: "pk-9" }]);
});

test("a service reply without a result, or an HTTP error, is a tool error", async () => {
	const fake = fakeService((tool) =>
		tool === "note" ? new Response("nope", { status: 401 }) : Response.json({ text: "denied" }, { status: 403 }),
	);
	const replies = await session(
		[
			request(1, "tools/call", { name: "note", arguments: { text: "hi", project: "demo" } }),
			request(2, "tools/call", { name: "today", arguments: {} }),
		],
		{ boss: true, service: () => fake.address },
	);
	const byId = new Map(replies.map((r) => [r.id, r.result]));
	expect(byId.get(1)).toEqual({
		content: [{ type: "text", text: "note: Jocasta's service answered HTTP 401 without a result" }],
		isError: true,
	});
	expect(byId.get(2)).toEqual({ content: [{ type: "text", text: "denied" }], isError: true });
});

const notRunning = {
	content: [{ type: "text", text: "Jocasta isn't running; start it with jocasta." }],
	isError: true,
};

test("no service: every tool answers the not-running text", async () => {
	expect(NOT_RUNNING).toBe("Jocasta isn't running; start it with jocasta.");
	const replies = await session(
		[
			request(1, "tools/call", { name: "graph_search", arguments: { project: "demo", query: "x" } }),
			request(2, "tools/call", { name: "projects", arguments: {} }),
		],
		{ boss: true, service: () => undefined },
	);
	expect(replies.map((r) => r.result)).toEqual([notRunning, notRunning]);
});

test("a refused connection is the not-running text too", async () => {
	const closed = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => new Response("") });
	const port = closed.port as number;
	closed.stop(true);
	const [reply] = await session([request(1, "tools/call", { name: "graph_search", arguments: { query: "x" } })], {
		service: () => ({ port, token: "t" }),
	});
	expect(reply?.result).toEqual(notRunning);
});

test("by default a call goes to the service <data>/jocasta-service.json names while its pid lives; frontier.json is still read (0584)", async () => {
	const fake = fakeService(() => Response.json({ text: "ok" }));
	const saved = process.env.JOCASTA_DATA_DIR;
	const dir = tempDir();
	process.env.JOCASTA_DATA_DIR = dir;
	const call = async () => {
		const [reply] = await session([request(1, "tools/call", { name: "graph_search", arguments: { query: "x" } })], {
			service: undefined,
		});
		return reply?.result?.content?.[0]?.text;
	};
	try {
		expect(await call()).toBe(NOT_RUNNING);
		const file = (pid: number) => JSON.stringify({ port: fake.address.port, token: fake.address.token, pid });
		writeFileSync(join(dir, "jocasta-service.json"), file(process.pid));
		expect(await call()).toBe("ok");
		expect(fake.calls.map((c) => c.auth)).toEqual(["Bearer tok-1"]);
		// A pid far above any real one on the test machine.
		writeFileSync(join(dir, "jocasta-service.json"), file(2 ** 30));
		expect(await call()).toBe(NOT_RUNNING);
		// The old name, from a service an older build started, is read when the new one is missing; the new one wins.
		writeFileSync(join(dir, "frontier.json"), file(process.pid));
		expect(await call()).toBe(NOT_RUNNING);
		rmSync(join(dir, "jocasta-service.json"));
		expect(await call()).toBe("ok");
	} finally {
		if (saved === undefined) delete process.env.JOCASTA_DATA_DIR;
		else process.env.JOCASTA_DATA_DIR = saved;
	}
});

test("malformed input gets JSON-RPC errors and the server keeps serving", async () => {
	const replies = await session(
		[
			"{not json",
			"[1,2]",
			"42",
			JSON.stringify({ jsonrpc: "2.0", id: 1 }),
			JSON.stringify({ jsonrpc: "2.0", id: { x: 1 }, method: "ping" }),
			JSON.stringify({ jsonrpc: "2.0", id: 2, result: {} }),
			"",
			request(3, "resources/list"),
			request(4, "tools/call", { name: "nope" }),
			request(5, "tools/call", { name: "graph_search", arguments: {} }),
			request(6, "tools/call", { name: "graph_search", arguments: { query: "x", limit: 0 } }),
			request(7, "tools/call", { name: "graph_search", arguments: { query: "x", extra: 1 } }),
			request(8, "tools/call", { name: "graph_search", arguments: "x" }),
			request(9, "tools/call", { name: "thread_start", arguments: { project: "demo", title: "t", brief: "b" } }),
			request(10, "ping"),
		],
		{ boss: false },
	);
	const error = (code: number) => ({ code, message: expect.any(String) });
	const tool = (text: string) => ({ content: [{ type: "text", text }], isError: true });
	expect(replies).toEqual([
		{ jsonrpc: "2.0", id: null, error: error(-32700) },
		{ jsonrpc: "2.0", id: null, error: error(-32600) },
		{ jsonrpc: "2.0", id: null, error: error(-32600) },
		{ jsonrpc: "2.0", id: 1, error: error(-32600) },
		{ jsonrpc: "2.0", id: null, error: error(-32600) },
		{ jsonrpc: "2.0", id: 3, error: error(-32601) },
		{ jsonrpc: "2.0", id: 4, error: error(-32602) },
		{ jsonrpc: "2.0", id: 5, result: tool("graph_search: missing query") },
		{ jsonrpc: "2.0", id: 6, result: tool("graph_search: limit must be an integer in range") },
		{ jsonrpc: "2.0", id: 7, result: tool("graph_search: unknown argument extra") },
		{ jsonrpc: "2.0", id: 8, error: error(-32602) },
		{ jsonrpc: "2.0", id: 9, error: error(-32602) },
		{ jsonrpc: "2.0", id: 10, result: {} },
	]);
});

test("report_result and thread_start's done_when are checked before the service is asked (0539, 0540)", async () => {
	const report = {
		outcome: "done",
		summary: "Login retries back off",
		files: ["src/auth.ts"],
		tests: [{ command: "bun test", passed: true }],
		decisions: [{ question: "Backoff cap", choice: "30 s", why: "the API's limit" }],
		questions: [],
		followUps: [],
	};
	const replies = await session(
		[
			request(1, "tools/call", { name: "report_result", arguments: { ...report, outcome: "maybe" } }),
			request(2, "tools/call", { name: "report_result", arguments: { ...report, summary: "x".repeat(201) } }),
			request(3, "tools/call", { name: "report_result", arguments: { ...report, extra: 1 } }),
			request(4, "tools/call", { name: "report_result", arguments: { ...report, files: undefined } }),
			request(5, "tools/call", { name: "report_result", arguments: report }),
		],
		{ env: THREAD_ENV },
	);
	const texts = replies.map((r) => r.result?.content?.[0]?.text);
	expect(texts[0]).toStartWith("report_result: outcome ");
	expect(texts[1]).toStartWith("report_result: summary ");
	expect(texts[2]).toBe("report_result: unknown field extra in the report");
	expect(texts[3]).toStartWith("report_result: ");
	// A valid report goes on to the service; none runs here.
	expect(texts[4]).toBe(NOT_RUNNING);
	const boss = await session(
		[
			request(6, "tools/call", {
				name: "thread_start",
				arguments: { project: "demo", title: "t", brief: "b", done_when: ["bun test", ""] },
			}),
		],
		{ boss: true },
	);
	expect(boss[0]?.result?.content?.[0]?.text).toBe("thread_start: done_when must be a list of non-empty strings");
});

test("a Node stream works as input, split across chunks, with CRLF line ends", async () => {
	const input = new PassThrough();
	const out: string[] = [];
	const done = runMcpServer({ boss: false, input, output: (l) => out.push(l), service: () => undefined });
	const text = `${request(1, "ping")}\r\n${request(2, "tools/list")}\n`;
	input.write(text.slice(0, 10));
	input.write(text.slice(10, 50));
	input.end(text.slice(50));
	await done;
	expect(out.map((l) => JSON.parse(l).id)).toEqual([1, 2]);
});
