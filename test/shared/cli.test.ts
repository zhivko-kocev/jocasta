import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { ServiceState } from "../../src/frontier/server/launch.ts";
import { UserError } from "../../src/shared/errors.ts";
import { herdrSession } from "../../src/shared/herdr-session.ts";
import { projectCommand } from "../../src/shared/project-cli.ts";
import { route } from "../../src/shared/route.ts";
import {
	attachedSurface,
	lastLoggedError,
	openSurface,
	type SurfaceDeps,
	serviceStopped,
} from "../../src/shared/surface.ts";
import { GIT_TEST_TIMEOUT, makeRepo, poll, useTempJocasta, writeProjectsYaml } from "./fixture.ts";

const env = useTempJocasta();
const DEMO = "https://github.com/example-org/demo.git";

// A live-check shell sets JOCASTA_HERDR_SESSION (0441): every test starts without it, and it is put back after.
const savedSession = process.env.JOCASTA_HERDR_SESSION;
beforeEach(() => {
	delete process.env.JOCASTA_HERDR_SESSION;
});
afterEach(() => {
	if (savedSession === undefined) delete process.env.JOCASTA_HERDR_SESSION;
	else process.env.JOCASTA_HERDR_SESSION = savedSession;
});

test("the jocasta command's words (0387, 0467, 0468, 0472, 0544); anything else is usage", () => {
	expect(route([])).toEqual({ role: "surface" });
	expect(route(["setup"])).toEqual({ role: "setup" });
	expect(route(["setup", "--refresh", "herdr.toml"])).toEqual({ role: "setup", refresh: "herdr.toml" });
	expect(route(["uninstall"])).toEqual({ role: "uninstall" });
	expect(route(["service"])).toEqual({ role: "service" });
	expect(route(["diagnose"])).toEqual({ role: "diagnose" });
	expect(route(["mcp"])).toEqual({ role: "mcp", boss: false });
	expect(route(["mcp", "--boss"])).toEqual({ role: "mcp", boss: true });
	expect(route(["project", "list"])).toEqual({ role: "project", args: ["list"] });
	expect(route(["thread", "list"])).toEqual({ role: "thread", args: ["list"] });
	expect(route(["worktree", "remove", "x"])).toEqual({ role: "worktree", args: ["remove", "x"] });
	// What the herdr plugin runs (0553), not in the usage.
	expect(route(["plugin", "today"])).toEqual({ role: "plugin", part: "today" });
	expect(route(["plugin", "ask"])).toEqual({ role: "plugin", part: "ask" });
	expect(route(["plugin", "startup"])).toEqual({ role: "plugin", part: "startup" });
	expect(route(["plugin", "action", "new-thread"])).toEqual({ role: "plugin", part: "action", action: "new-thread" });
	expect(route(["--version"])).toEqual({ role: "version" });
	expect(route(["-v"])).toEqual({ role: "version" });
	expect(route(["--help"])).toEqual({ role: "help" });
	// Retired words (0387) are no command and are named; stray arguments to a command are invocation errors.
	for (const args of [["guard"], ["harness"], ["memory", "import"], ["fix it"]]) {
		expect(route(args)).toEqual({ role: "usage", unknown: args[0] });
	}
	for (const args of [
		["mcp", "-x"],
		["setup", "now"],
		["setup", "--refresh"],
		["uninstall", "--yes"],
		["diagnose", "now"],
		["service", "--x"],
		["plugin"],
		["plugin", "today", "now"],
		["plugin", "action", "merge"],
		["plugin", "action", "stop", "x"],
	]) {
		expect(route(args)).toEqual({ role: "usage" });
	}
});

test("herdr's session: jocasta, or JOCASTA_HERDR_SESSION for live checks; a bad name is an invocation error (0441)", () => {
	expect(herdrSession()).toBe("jocasta");
	process.env.JOCASTA_HERDR_SESSION = "";
	expect(herdrSession()).toBe("jocasta");
	process.env.JOCASTA_HERDR_SESSION = "jocasta-polish";
	expect(herdrSession()).toBe("jocasta-polish");
	for (const bad of ["../x", "-x", "a b", "x".repeat(65)]) {
		process.env.JOCASTA_HERDR_SESSION = bad;
		expect(() => herdrSession()).toThrow(
			new UserError("JOCASTA_HERDR_SESSION must be letters, digits, '.', '_' or '-'", 2),
		);
	}
});

test(
	"project add twice says it's already a project; list shows watched repos, or how to add one; other verbs are usage (0111)",
	async () => {
		expect(await projectCommand(["list"])).toEqual([
			"No projects yet: add one with jocasta project add <path>, or a watched root in projects.yaml.",
		]);
		const code = join(env.root, "code");
		const repo = makeRepo(join(code, "demo"), { origin: DEMO });
		const [added] = await projectCommand(["add", repo]);
		expect(added).toStartWith("added demo (");
		expect((await projectCommand(["add", repo]))[0]).toMatch(/^demo \(p-[0-9a-f]+\) is already a project, at /);
		writeProjectsYaml(env.config, `version: 1\nroots:\n  - path: ${code.replaceAll("\\", "/")}\n    depth: 1\n`);
		expect((await projectCommand(["list"]))[0]).toMatch(/^p-[0-9a-f]{16} {2}demo {2}/);
		await expect(projectCommand(["rename", "x"])).rejects.toThrow(UserError);
	},
	GIT_TEST_TIMEOUT,
);

test("project usage and --help touch nothing: no catalog, no data folder (as thread --help)", async () => {
	for (const args of [["--help"], [], ["rename", "x"], ["add"], ["list", "x"]]) {
		await expect(projectCommand(args)).rejects.toThrow(
			new UserError("usage: jocasta project list | add <path> | remove <name|id>"),
		);
	}
	expect(existsSync(env.data)).toBe(false);
});

/** A started service that keeps running. */
const running = () => ({ exited: new Promise<number>(() => {}) });

function surface(over: Partial<SurfaceDeps> = {}) {
	const calls = {
		started: 0,
		attached: [] as { argv: string[]; env: Record<string, string> }[],
		/** What closing Jocasta did, in order (0434). */
		closed: [] as string[],
	};
	const printed: string[] = [];
	const deps: SurfaceDeps = {
		serviceHealth: async () => ({ state: "up" }),
		startService: () => {
			calls.started++;
			return running();
		},
		lastError: () => undefined,
		herdr: "herdr.exe",
		herdrConfig: join(env.root, "herdr.toml"),
		attach: async (argv, e) => {
			calls.attached.push({ argv, env: e });
			return 0;
		},
		print: (l) => void printed.push(l),
		sleep: async () => {},
		enterSurface: () => ({ leave: () => true }),
		onTerminalClose: () => () => {},
		tellServiceClosed: async () => {
			calls.closed.push("tell service");
			return false;
		},
		stopHerdr: async () => void calls.closed.push("stop herdr"),
		stopContainers: () => void calls.closed.push("stop containers"),
		exit: () => {},
		watchServiceStop: () => async () => false,
		herdrUp: async () => true,
		...over,
	};
	return { deps, calls, printed };
}

test("bare jocasta starts the service when none answers, then attaches herdr's session jocasta (0392, 0552)", async () => {
	mkdirSync(env.root, { recursive: true });
	writeFileSync(join(env.root, "herdr.toml"), "");
	let state: ServiceState = "down";
	const { deps, calls } = surface({
		serviceHealth: async () => ({ state }),
		startService: () => {
			calls.started++;
			state = "up";
			return running();
		},
	});
	expect(await openSurface(deps)).toBe(0);
	expect(calls.started).toBe(1);
	expect(calls.attached).toEqual([
		{ argv: ["herdr.exe", "--session", "jocasta"], env: { HERDR_CONFIG_PATH: join(env.root, "herdr.toml") } },
	]);

	// Running already: no second service.
	const again = surface();
	await openSurface(again.deps);
	expect(again.calls.started).toBe(0);

	// A throwaway session for live checks (0441).
	process.env.JOCASTA_HERDR_SESSION = "jocasta-polish";
	const live = surface();
	await openSurface(live.deps);
	expect(live.calls.attached[0]?.argv).toEqual(["herdr.exe", "--session", "jocasta-polish"]);
});

test("a service that is closing is waited for, then started once, after saying so (LC-3)", async () => {
	const states: ServiceState[] = ["closing", "closing", "down"];
	const order: string[] = [];
	const { deps, calls } = surface({
		serviceHealth: async () => ({ state: states.shift() ?? "up" }),
		print: (l) => void order.push(l),
		startService: () => {
			calls.started++;
			order.push("start");
			return running();
		},
	});
	await openSurface(deps);
	expect(calls.started).toBe(1);
	expect(order.slice(0, 2)).toEqual(["jocasta: Jocasta is closing; it starts again once it has.", "start"]);
	expect(calls.attached).toHaveLength(1);
});

test("a hung service (busy: its process lives, its /health doesn't answer) gets a new one started, which takes over (0534)", async () => {
	let state: ServiceState = "busy";
	const { deps, calls } = surface({
		serviceHealth: async () => ({ state }),
		startService: () => {
			calls.started++;
			state = "up";
			return running();
		},
	});
	await openSurface(deps);
	expect(calls.started).toBe(1);
	expect(calls.attached).toHaveLength(1);
});

test("a service it started that doesn't answer in time is said; a missing herdr.toml points at setup; no herdr refuses", async () => {
	const { deps, calls, printed } = surface({ serviceHealth: async () => ({ state: "down" }), waitMs: 0 });
	await openSurface(deps);
	expect(calls.started).toBe(1);
	expect(printed.join("\n")).toContain("the service is still starting");
	expect(calls.attached).toHaveLength(1);
	expect(calls.attached[0]?.env).toEqual({});
	expect(printed.join("\n")).toContain("run `jocasta setup`");
	await expect(openSurface(surface({ herdr: undefined }).deps)).rejects.toThrow("herdr isn't installed");
});

test("a service that exits before it answers is said with its last error, and nothing attaches: exit 1 (0582)", async () => {
	let asked = 0;
	const { deps, calls, printed } = surface({
		serviceHealth: async () => {
			asked++;
			return { state: "down" };
		},
		startService: () => {
			calls.started++;
			return { exited: Promise.resolve(1) };
		},
		lastError: () => "C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token",
	});
	expect(await openSurface(deps)).toBe(1);
	expect(printed).toEqual([
		"jocasta: the service stopped while starting: C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token",
		"jocasta: run `jocasta diagnose` for the full log.",
	]);
	expect(calls.attached).toEqual([]);
	expect(asked).toBeLessThan(5);

	// One that left because another service answers: that one is attached to.
	let state: ServiceState = "down";
	const left = surface({
		serviceHealth: async () => ({ state }),
		startService: () => {
			state = "up";
			return { exited: Promise.resolve(0) };
		},
	});
	expect(await openSurface(left.deps)).toBe(0);
	expect(left.calls.attached).toHaveLength(1);
});

test("lastLoggedError: the newest ERROR line's message since the start, without the service's lead (0582)", () => {
	const dir = join(env.root, "logs-last");
	mkdirSync(dir, { recursive: true });
	writeFileSync(
		join(dir, "jocasta-2026-11-08.log"),
		"2026-11-08T23:59:00.000Z ERROR [service] didn't start: yesterday's\n",
	);
	writeFileSync(
		join(dir, "jocasta-2026-11-09.log"),
		[
			"2026-11-09T10:00:00.000Z ERROR [service] didn't start: earlier",
			"2026-11-09T10:05:00.000Z WARN [setup] a warning",
			"2026-11-09T10:05:01.000Z ERROR [service] didn't start: C:/x/projects.yaml:4: Unexpected token",
			"2026-11-09T10:05:02.000Z INFO [service] after",
			"",
		].join("\n"),
	);
	expect(lastLoggedError(dir, Date.parse("2026-11-09T10:05:00.500Z"))).toBe("C:/x/projects.yaml:4: Unexpected token");
	expect(lastLoggedError(dir, Date.parse("2026-11-09T11:00:00.000Z"))).toBeUndefined();
	expect(lastLoggedError(join(env.root, "none"), 0)).toBeUndefined();
});

test("herdr's client exiting closes Jocasta: the service is told when it answers, else the containers and herdr stop here (0434, 0549)", async () => {
	const told = surface({
		tellServiceClosed: async () => {
			told.calls.closed.push("tell service");
			return true;
		},
	});
	await openSurface(told.deps);
	expect(told.calls.closed).toEqual(["tell service"]);
	// The client exited by itself: what happens next is said, since a question may open in a new terminal.
	expect(told.printed).toContain("jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.");

	const alone = surface();
	expect(await openSurface(alone.deps)).toBe(0);
	// The containers' stop goes out first: a closing console leaves seconds (LC-7).
	expect(alone.calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);

	// herdr that won't stop is said; the containers stopped all the same.
	const stuck = surface({
		stopHerdr: async () => {
			throw new Error("didn't stop in 3 s");
		},
	});
	await openSurface(stuck.deps);
	expect(stuck.printed).toContain("jocasta: herdr session jocasta didn't stop: didn't stop in 3 s");
	expect(stuck.calls.closed).toEqual(["tell service", "stop containers"]);

	// herdr that never answers still has the containers stopped first.
	const reached = Promise.withResolvers<void>();
	const hung = surface({
		stopHerdr: () => {
			reached.resolve();
			return new Promise<void>(() => {});
		},
	});
	void openSurface(hung.deps);
	await reached.promise;
	expect(hung.calls.closed).toEqual(["tell service", "stop containers"]);
});

test("closing the terminal tells the service once and ends this process (0549)", async () => {
	let close: (() => void) | undefined;
	const exited = Promise.withResolvers<void>();
	const client = Promise.withResolvers<number>();
	const { deps, calls } = surface({
		onTerminalClose: (handler) => {
			close = handler;
			return () => {};
		},
		attach: () => client.promise,
		exit: () => exited.resolve(),
	});
	const run = openSurface(deps);
	await poll(() => close !== undefined, { intervalMs: 1, what: "the terminal-close handler" });
	close?.();
	await exited.promise;
	expect(calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);
	client.resolve(0);
	await run;
	// The client's exit after it closes nothing more.
	expect(calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);
});

test("bare jocasta exits 0 when the service stopped herdr itself; any other client exit keeps herdr's code (0491)", async () => {
	const herdrQuits = (over: Partial<SurfaceDeps>) => surface({ attach: async () => 1, ...over }).deps;
	const stopped = surface({ attach: async () => 1, watchServiceStop: () => async () => true });
	expect(await openSurface(stopped.deps)).toBe(0);
	// Its own stop ended everything: nothing is closed a second time, and nothing says Jocasta is closing.
	expect(stopped.calls.closed).toEqual([]);
	expect(stopped.printed).not.toContain(
		"jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.",
	);
	expect(await openSurface(herdrQuits({}))).toBe(1);
	// The check is made only for a failing client, and only once herdr's client has exited.
	let watched = 0;
	let checked = 0;
	const watch = () => {
		watched++;
		return async () => {
			checked++;
			return true;
		};
	};
	expect(await openSurface(surface({ watchServiceStop: watch }).deps)).toBe(0);
	expect([watched, checked]).toEqual([1, 0]);

	// Which service stopped it: the one jocasta-service.json named as the client started.
	const named = { port: 1, token: "t", pid: 7 };
	const closing = async () => "closing" as const;
	const up = async () => "up" as const;
	const down = async () => "down" as const;
	// Stop everything under way: /health says closing.
	expect(await serviceStopped(named, named, closing)).toBe(true);
	// Its close already ended by removing the file, or another service has started since.
	expect(await serviceStopped(named, undefined, down)).toBe(true);
	expect(await serviceStopped(named, { ...named, pid: 8 }, up)).toBe(true);
	// herdr quit while the service runs, or the service died (its file stays): herdr's code.
	expect(await serviceStopped(named, named, up)).toBe(false);
	expect(await serviceStopped(named, named, down)).toBe(false);
	// No service when the client started: herdr's code.
	expect(await serviceStopped(undefined, undefined, closing)).toBe(false);
});

test("a herdr server that dies under the client is started again by the service and its client attached again (0518)", async () => {
	/** A surface whose client exits 1 every time, each attach recorded. */
	const failing = (over: Partial<SurfaceDeps>) => {
		const s = surface(over);
		s.deps.attach = async (argv, e) => {
			s.calls.attached.push({ argv, env: e });
			return 1;
		};
		return s;
	};
	/** herdr's server: answering, or gone until `back` more pings. */
	const server = { up: true, back: 0 };
	const herdrUp = async () => {
		if (server.up) return true;
		if (server.back-- > 0) return false;
		server.up = true;
		return true;
	};
	const exits = [1, 0];
	const order: string[] = [];
	let slept = 0;
	const healed = surface({
		herdrUp,
		sleep: async (ms) => {
			slept += ms;
		},
		attach: async (argv, e) => {
			healed.calls.attached.push({ argv, env: e });
			order.push("attach");
			const code = exits.shift() ?? 0;
			// The first client's server dies under it; the service has it back after a few pings.
			if (code === 1) Object.assign(server, { up: false, back: 3 });
			return code;
		},
		tellServiceClosed: async () => {
			order.push("close");
			return true;
		},
	});
	// The later client's exit is him quitting it while its server runs: Jocasta closes as before.
	expect(await openSurface(healed.deps)).toBe(0);
	expect(order).toEqual(["attach", "attach", "close"]);
	// Attached again the same way as the first time.
	expect(healed.calls.attached[1]).toEqual(healed.calls.attached[0]);
	expect(slept).toBe(3 * 250);
	expect(healed.printed).toContain(
		"jocasta: herdr session jocasta stopped; waiting for the service to start it again.",
	);

	// herdr's server still running after the client exits and the service started none again: he quit it, a close.
	const quit = failing({ serviceHealth: async () => ({ state: "up", herdrRevivals: 0 }) });
	expect(await openSurface(quit.deps)).toBe(1);
	expect(quit.calls.attached).toHaveLength(1);
	expect(quit.calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);

	// No service answers, so nothing starts herdr again: a close, without waiting.
	let alonePings = 0;
	const alone = failing({
		serviceHealth: async () => ({ state: "down" }),
		waitMs: 0,
		herdrUp: async () => {
			alonePings++;
			return false;
		},
	});
	expect(await openSurface(alone.deps)).toBe(1);
	expect(alone.calls.attached).toHaveLength(1);
	expect(alone.calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);
	expect(alonePings).toBe(1);

	// herdr not back within 30 s: a close, said.
	let waited = 0;
	const late = failing({
		herdrUp: async () => false,
		sleep: async (ms) => {
			waited += ms;
		},
	});
	expect(await openSurface(late.deps)).toBe(1);
	expect(waited).toBe(30_000);
	expect(late.calls.attached).toHaveLength(1);
	expect(late.calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);
	expect(late.printed).toContain("jocasta: herdr session jocasta isn't back after 30 s.");

	// A service that starts closing while it waits starts nothing: the wait ends there.
	let state: ServiceState = "up";
	let closingWait = 0;
	const stopping = failing({
		serviceHealth: async () => ({ state }),
		herdrUp: async () => false,
		sleep: async (ms) => {
			closingWait += ms;
			state = "closing";
		},
	});
	expect(await openSurface(stopping.deps)).toBe(1);
	expect(closingWait).toBe(250);
	expect(stopping.calls.attached).toHaveLength(1);

	// The service's own stop (0491) is still checked first: 0, nothing closed, herdr never pinged.
	let pinged = 0;
	const own = failing({
		watchServiceStop: () => async () => true,
		herdrUp: async () => {
			pinged++;
			return false;
		},
	});
	expect(await openSurface(own.deps)).toBe(0);
	expect([own.calls.attached.length, pinged]).toEqual([1, 0]);
	expect(own.calls.closed).toEqual([]);
});

test("a herdr crash the service healed before the client looked is not him quitting: the client attaches again (0534)", async () => {
	// herdr's server crashed under the client and the service started it again at once: by the time the client's exit
	// is looked at, herdr answers again. Only the service's restart count says it wasn't him.
	let revivals = 0;
	const exits = [1, 0];
	const order: string[] = [];
	const crashed = surface({
		serviceHealth: async () => ({ state: "up", herdrRevivals: revivals }),
		herdrUp: async () => true,
		attach: async () => {
			order.push("attach");
			const code = exits.shift() ?? 0;
			// The crash and the service's restart both happen before the client's exit is looked at.
			if (code === 1) revivals++;
			return code;
		},
		tellServiceClosed: async () => {
			order.push("close");
			return true;
		},
	});
	expect(await openSurface(crashed.deps)).toBe(0);
	expect(order).toEqual(["attach", "attach", "close"]);

	// The same exit with no restart since the client attached is him quitting: a close at once.
	const quit = surface({
		serviceHealth: async () => ({ state: "up", herdrRevivals: 3 }),
		attach: async () => 1,
		tellServiceClosed: async () => {
			quit.calls.closed.push("tell service");
			return true;
		},
	});
	expect(await openSurface(quit.deps)).toBe(1);
	expect(quit.calls.closed).toEqual(["tell service"]);
});

test("a client exit while another jocasta holds the session (a second terminal, a re-attach) closes nothing (0434)", async () => {
	const { deps, calls } = surface({ enterSurface: () => ({ leave: () => false }) });
	await openSurface(deps);
	expect(calls.attached).toHaveLength(1);
	expect(calls.closed).toEqual([]);

	// A client that couldn't start closes nothing either.
	let left = 0;
	const failed = surface({
		enterSurface: () => ({ leave: () => ++left > 0 }),
		attach: async () => {
			throw new Error("spawn failed");
		},
	});
	await expect(openSurface(failed.deps)).rejects.toThrow("spawn failed");
	expect(left).toBe(1);
	expect(failed.calls.closed).toEqual([]);
});

test("the terminal closing closes Jocasta once, with herdr's client exiting at the same time, then exits (0434)", async () => {
	const client = Promise.withResolvers<number>();
	const listening = Promise.withResolvers<() => void>();
	const exited = Promise.withResolvers<void>();
	const s = surface({
		attach: () => client.promise,
		onTerminalClose: (handler) => {
			listening.resolve(handler);
			return () => {};
		},
		exit: () => exited.resolve(),
	});
	const opened = openSurface(s.deps);
	(await listening.promise)();
	client.resolve(0);
	expect(await opened).toBe(0);
	await exited.promise;
	expect(s.calls.closed).toEqual(["tell service", "stop containers", "stop herdr"]);
});

test("attachedSurface: the last jocasta to leave closes; files of jocasta processes that are gone don't count", () => {
	const dir = join(env.root, "surfaces");
	const alive = new Set([101, 102]);
	const started = () => undefined;
	const first = attachedSurface(dir, 101, (pid) => alive.has(pid), started);
	const second = attachedSurface(dir, 102, (pid) => alive.has(pid), started);
	writeFileSync(join(dir, "999"), "");
	expect(first.leave()).toBe(false);
	alive.delete(101);
	expect(second.leave()).toBe(true);
	expect(readdirSync(dir)).toEqual([]);
});

test("attachedSurface: a file whose pid now belongs to a process started after it was written is stale (LC-4)", () => {
	const dir = join(env.root, "surfaces");
	const own = attachedSurface(
		dir,
		101,
		() => true,
		() => Date.now(),
	);
	const stale = join(dir, "999");
	writeFileSync(stale, "");
	const hourAgo = new Date(Date.now() - 3_600_000);
	utimesSync(stale, hourAgo, hourAgo);
	expect(own.leave()).toBe(true);
	expect(existsSync(stale)).toBe(false);
});
