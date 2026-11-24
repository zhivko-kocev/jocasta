// The bridge extension (0675): hello with the pane's key, tools from the welcome, a call's round trip, a prompt
// delivered with `sendUserMessage`, and a turn-end `stop`; with no `JOCASTA_BRIDGE` it does nothing.
import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import bridge from "../../config/bridge-extension.ts";

type Handler = (event: object, context: object) => unknown;
type Line = Record<string, unknown>;

const cleanup: (() => void)[] = [];
afterEach(() => {
	for (const undo of cleanup.splice(0)) undo();
});

function fakePi() {
	const tools = new Map<string, { execute(id: string, args: object): Promise<{ content: { text: string }[] }> }>();
	const handlers = new Map<string, Handler>();
	const sent: [string, object | undefined][] = [];
	return {
		tools,
		handlers,
		sent,
		pi: {
			registerTool: (tool: { name: string; execute: never }) => tools.set(tool.name, tool),
			sendUserMessage: (text: string, options?: object) => sent.push([text, options]),
			on: (name: string, handler: Handler) => handlers.set(name, handler),
		},
	};
}

let serial = 0;

// These tests run a real socket against the extension, so they wait on the pipe's own timing (polling, never a guess).
async function until(check: () => boolean): Promise<boolean> {
	for (let tries = 0; tries < 200 && !check(); tries++) await Bun.sleep(10);
	return check();
}

async function service(onLine: (line: Line, reply: (message: Line) => void) => void) {
	const dir = mkdtempSync(join(tmpdir(), "bridge-ext-"));
	const address =
		process.platform === "win32" ? `\\\\.\\pipe\\bridge-ext-${process.pid}-${Date.now()}-${++serial}` : join(dir, "s");
	const lines: Line[] = [];
	const sockets = new Set<net.Socket>();
	const server = net.createServer((socket) => {
		let buffer = "";
		sockets.add(socket);
		const reply = (message: Line) => socket.write(`${JSON.stringify(message)}\n`);
		socket.on("data", (chunk) => {
			buffer += chunk.toString();
			for (let end = buffer.indexOf("\n"); end >= 0; end = buffer.indexOf("\n")) {
				const line = JSON.parse(buffer.slice(0, end)) as Line;
				buffer = buffer.slice(end + 1);
				lines.push(line);
				onLine(line, reply);
			}
		});
	});
	const listening = Promise.withResolvers<void>();
	server.listen(address, () => listening.resolve());
	await listening.promise;
	cleanup.push(() => {
		for (const socket of sockets) socket.destroy();
		server.close();
		rmSync(dir, { recursive: true, force: true });
	});
	return {
		address,
		lines,
		push: (message: Line) => {
			for (const socket of sockets) socket.write(`${JSON.stringify(message)}\n`);
		},
		drop: () => {
			server.close();
			for (const socket of sockets) socket.destroy();
		},
	};
}

type Context = Record<string, unknown>;

/** An extension welcomed by a service with one tool, its `session_start` run with `context`. */
async function connected(context: Context) {
	const svc = await service((line, reply) => {
		if (line.t === "hello") {
			reply({
				t: "welcome",
				v: 1,
				role: "thread",
				tools: [{ name: "probe", label: "Probe", description: "d", parameters: { type: "object" } }],
			});
		}
		if (line.t === "call") reply({ t: "result", id: line.id, ok: true, text: "pong" });
	});
	process.env.JOCASTA_BRIDGE = svc.address;
	process.env.JOCASTA_PANE_KEY = "key-1";
	const fake = fakePi();
	await bridge(fake.pi);
	cleanup.push(() => {
		delete process.env.JOCASTA_BRIDGE;
	});
	await fake.handlers.get("session_start")?.({}, context);
	const answered = async (id: string): Promise<Line | undefined> => {
		const isAnswer = (l: Line) => (l.t === "ack" || l.t === "nack") && l.id === id;
		await until(() => svc.lines.some(isAnswer));
		return svc.lines.find(isAnswer);
	};
	return { ...svc, ...fake, answered };
}

test("a prompt while omp is busy goes in with its deliverAs, an idle omp gets none, and each is acked", async () => {
	let idle = false;
	const run = await connected({ isIdle: () => idle });
	run.push({ t: "prompt", id: "p1", text: "steer this", deliver: "steer" });
	expect(await run.answered("p1")).toEqual({ t: "ack", id: "p1" });
	run.push({ t: "prompt", id: "p2", text: "queued" });
	expect(await run.answered("p2")).toEqual({ t: "ack", id: "p2" });
	run.push({ t: "prompt", id: "p4", text: "after this turn", deliver: "nextTurn" });
	expect(await run.answered("p4")).toEqual({ t: "ack", id: "p4" });
	idle = true;
	run.push({ t: "prompt", id: "p3", text: "plain", deliver: "steer" });
	expect(await run.answered("p3")).toEqual({ t: "ack", id: "p3" });
	expect(run.sent).toEqual([
		["steer this", { deliverAs: "steer" }],
		["queued", { deliverAs: "followUp" }],
		// omp has no nextTurn delivery for a user message: it waits for the turn as followUp does, never steers.
		["after this turn", { deliverAs: "followUp" }],
		["plain", undefined],
	]);
});

test("/plan-review opens omp's plan review instead of a message", async () => {
	let opened = 0;
	const run = await connected({
		isIdle: () => true,
		openPlanReview: () => {
			opened++;
		},
	});
	run.push({ t: "prompt", id: "r1", text: "/plan-review", deliver: "steer" });
	expect(await run.answered("r1")).toEqual({ t: "ack", id: "r1" });
	expect(opened).toBe(1);
	expect(run.sent).toEqual([]);
});

test("a compact message runs ctx.compact as a handoff with its guidance and is acked; without it, a nack", async () => {
	const calls: object[] = [];
	const run = await connected({
		compact: (options: object) => {
			calls.push(options);
		},
	});
	run.push({ t: "compact", id: "c1", mode: "handoff", instructions: "keep the plan" });
	expect(await run.answered("c1")).toEqual({ t: "ack", id: "c1" });
	expect(calls).toEqual([{ mode: "handoff", internalGuidance: "keep the plan" }]);
	const bare = await connected({});
	bare.push({ t: "compact", id: "c2", instructions: "x" });
	expect(await bare.answered("c2")).toMatchObject({ t: "nack", id: "c2" });
});

test("abort and shutdown messages reach ctx.abort and ctx.shutdown", async () => {
	const done: string[] = [];
	const run = await connected({
		abort: () => done.push("abort"),
		shutdown: () => done.push("shutdown"),
	});
	run.push({ t: "abort" });
	run.push({ t: "shutdown" });
	await until(() => done.length >= 2);
	expect(done).toEqual(["abort", "shutdown"]);
});

test("a call while the pipe is down says the service isn't reachable", async () => {
	const run = await connected({ isIdle: () => true });
	expect((await run.tools.get("probe")?.execute("1", {}))?.content[0]?.text).toBe("pong");
	run.drop();
	await Bun.sleep(100); // the socket's close event has no other signal to wait on
	await expect(run.tools.get("probe")?.execute("2", {})).rejects.toThrow(
		"Jocasta's service isn't reachable; try again in a moment.",
	);
});

test("with no JOCASTA_BRIDGE the extension registers nothing", async () => {
	delete process.env.JOCASTA_BRIDGE;
	const { pi, tools, handlers } = fakePi();
	await bridge(pi);
	expect(tools.size).toBe(0);
	expect(handlers.size).toBe(0);
});

test("hello, welcome tools, a call's result and a prompt through sendUserMessage", async () => {
	const { address, lines } = await service((line, reply) => {
		if (line.t === "hello") {
			reply({
				t: "welcome",
				v: 1,
				role: "thread",
				tools: [{ name: "probe", label: "Probe", description: "d", parameters: { type: "object" } }],
			});
		}
		if (line.t === "call") reply({ t: "result", id: line.id, ok: true, text: "pong" });
		if (line.t === "stop") reply({ t: "stop_result", id: line.id, context: "fix the gate" });
	});
	process.env.JOCASTA_BRIDGE = address;
	process.env.JOCASTA_PANE_KEY = "key-1";
	const { pi, tools, handlers, sent } = fakePi();
	await bridge(pi);
	expect(lines[0]).toMatchObject({ t: "hello", v: 1, key: "key-1", harness: "omp", caps: ["prompt"] });
	const answer = await tools.get("probe")?.execute("1", { x: 1 });
	expect(answer?.content[0]?.text).toBe("pong");
	const context = { isIdle: () => true, abort() {}, shutdown() {} };
	await handlers.get("session_start")?.({}, context);
	await Bun.sleep(50);
	expect(lines.some((line) => line.t === "event" && line.name === "caps")).toBe(true);
	const stop = await handlers.get("session_stop")?.({}, context);
	expect(stop).toEqual({ continue: true, additionalContext: "fix the gate" });
	expect(await handlers.get("session_stop")?.({ stop_hook_active: true }, context)).toBeUndefined();
	delete process.env.JOCASTA_BRIDGE;
	expect(sent).toEqual([]);
});

test("the session file goes up with the caps event and with each agent_end, since the hello comes before session_start (0676)", async () => {
	let file = "first.jsonl";
	const run = await connected({
		isIdle: () => true,
		abort() {},
		shutdown() {},
		sessionManager: { getSessionFile: () => file },
	});
	expect(await until(() => run.lines.some((line) => line.t === "event" && line.name === "caps"))).toBe(true);
	expect(run.lines.find((line) => line.t === "event" && line.name === "caps")?.session).toBe("first.jsonl");
	file = "second.jsonl";
	await run.handlers.get("agent_end")?.({}, { isIdle: () => true, sessionManager: { getSessionFile: () => file } });
	expect(await until(() => run.lines.some((line) => line.name === "agent_end"))).toBe(true);
	expect(run.lines.find((line) => line.name === "agent_end")).toMatchObject({ idle: true, session: "second.jsonl" });
});
