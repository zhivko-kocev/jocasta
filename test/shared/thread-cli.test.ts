import { afterEach, expect, test } from "bun:test";
import { UserError } from "../../src/shared/errors.ts";
import {
	CLI_PATH,
	cliRequest,
	EXIT,
	runCliCommand,
	THREAD_USAGE,
	WORKTREE_USAGE,
} from "../../src/shared/thread-cli.ts";

const servers: { stop(force?: boolean): unknown }[] = [];
afterEach(() => {
	for (const s of servers.splice(0)) s.stop(true);
});

test("jocasta thread|worktree words become the service's commands; the changes ask him (0544)", () => {
	expect(
		cliRequest("thread", [
			"new",
			"demo",
			"Fix the login",
			"--done-when",
			"bun test a",
			"--title",
			"Login",
			"--done-when",
			"bun test b",
		]),
	).toEqual({
		command: "thread new",
		args: { project: "demo", brief: "Fix the login", title: "Login", done_when: ["bun test a", "bun test b"] },
		asks: false,
	});
	expect(cliRequest("thread", ["new", "demo", "Fix it"])).toEqual({
		command: "thread new",
		args: { project: "demo", brief: "Fix it" },
		asks: false,
	});
	expect(cliRequest("thread", ["list"])).toEqual({ command: "thread list", args: {}, asks: false });
	expect(cliRequest("thread", ["list", "demo"])).toEqual({
		command: "thread list",
		args: { project: "demo" },
		asks: false,
	});
	expect(cliRequest("thread", ["status", "Login"])).toEqual({
		command: "thread status",
		args: { thread: "Login" },
		asks: false,
	});
	expect(cliRequest("thread", ["send", "Login", "Go on."])).toEqual({
		command: "thread send",
		args: { thread: "Login", text: "Go on." },
		asks: true,
	});
	expect(cliRequest("thread", ["stop", "Login"])).toEqual({
		command: "thread stop",
		args: { thread: "Login" },
		asks: true,
	});
	expect(cliRequest("thread", ["finish", "Login", "pr"])).toEqual({
		command: "thread finish",
		args: { thread: "Login", how: "pr" },
		asks: true,
	});
	expect(cliRequest("worktree", ["list"])).toEqual({ command: "worktree list", args: {}, asks: false });
	expect(cliRequest("worktree", ["remove", "C:/x"])).toEqual({
		command: "worktree remove",
		args: { path: "C:/x" },
		asks: true,
	});
	const usage = (word: "thread" | "worktree", args: string[]) => {
		try {
			cliRequest(word, args);
		} catch (error) {
			expect(error).toBeInstanceOf(UserError);
			expect((error as UserError).exitCode).toBe(EXIT.usage);
			return (error as Error).message;
		}
		throw new Error(`no usage error for ${args.join(" ")}`);
	};
	for (const args of [
		[],
		["new", "demo"],
		["new", "demo", "brief", "extra"],
		["new", "demo", "brief", "--done-when"],
		["new", "demo", "brief", "--title", "a", "--title", "b"],
		["new", "demo", "brief", "--force"],
		["list", "a", "b"],
		["status"],
		["send", "x"],
		["stop", "x", "y"],
		["finish", "x", "squash"],
		["finish", "x"],
	]) {
		expect(usage("thread", args)).toBe(THREAD_USAGE);
	}
	for (const args of [[], ["list", "x"], ["remove"], ["prune"]]) expect(usage("worktree", args)).toBe(WORKTREE_USAGE);
});

/** A service answering `POST /api/cli` with `reply`; records what it got. */
function fakeService(reply: Response | (() => Response)) {
	const calls: { path: string; auth: string | null; body: unknown }[] = [];
	const server = Bun.serve({
		port: 0,
		hostname: "127.0.0.1",
		async fetch(req) {
			calls.push({ path: new URL(req.url).pathname, auth: req.headers.get("authorization"), body: await req.json() });
			return typeof reply === "function" ? reply() : reply.clone();
		},
	});
	servers.push(server);
	return { address: { port: server.port as number, token: "tok" }, calls };
}

test("a command goes to the service with its token; exit 0 done, 1 refused, 3 when Jocasta isn't running (0544)", async () => {
	const out: string[] = [];
	const err: string[] = [];
	const deps = (address?: { port: number; token: string }) => ({
		service: () => address,
		print: (line: string) => out.push(line),
		printError: (line: string) => err.push(line),
	});
	const ok = fakeService(Response.json({ text: "Stopped Login's run." }));
	expect(await runCliCommand(cliRequest("thread", ["stop", "Login"]), deps(ok.address))).toBe(EXIT.done);
	expect(ok.calls).toEqual([
		{ path: CLI_PATH, auth: "Bearer tok", body: { command: "thread stop", args: { thread: "Login" } } },
	]);
	expect(out).toEqual(["Stopped Login's run."]);
	expect(err).toEqual(["jocasta: asking you first in herdr's popup (Allow with its code; 2 minutes)…"]);

	out.length = 0;
	err.length = 0;
	const refused = fakeService(Response.json({ text: "He chose Deny; nothing was stopped.", isError: true }));
	expect(await runCliCommand(cliRequest("thread", ["list"]), deps(refused.address))).toBe(EXIT.refused);
	expect(err).toEqual(["jocasta: He chose Deny; nothing was stopped."]);
	expect(out).toEqual([]);

	err.length = 0;
	expect(await runCliCommand(cliRequest("thread", ["list"]), deps())).toBe(EXIT.notRunning);
	expect(err).toEqual(["jocasta: Jocasta isn't running; start it with jocasta."]);

	// jocasta-service.json names a live process, but nothing listens on its port.
	err.length = 0;
	const gone = fakeService(Response.json({ text: "x" }));
	servers.pop()?.stop(true);
	expect(await runCliCommand(cliRequest("worktree", ["list"]), deps(gone.address))).toBe(EXIT.notRunning);
	expect(err).toEqual(["jocasta: Jocasta isn't running; start it with jocasta."]);

	err.length = 0;
	const broken = fakeService(() => new Response("nope", { status: 500 }));
	expect(await runCliCommand(cliRequest("worktree", ["list"]), deps(broken.address))).toBe(EXIT.refused);
	expect(err).toEqual(["jocasta: Jocasta's service answered HTTP 500 without a result"]);
});
