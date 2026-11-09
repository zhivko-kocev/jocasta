// What bare `jocasta` and a starting service read from the running one's `/health` (LC-1, LC-3, 0534).
import { afterEach, expect, test } from "bun:test";
import type { Server } from "bun";
import { serviceHealth, serviceState } from "../../../src/frontier/server/launch.ts";
import { freePort } from "../../../src/shared/fsx.ts";

const servers: Server<undefined>[] = [];
afterEach(() => {
	for (const server of servers.splice(0)) server.stop(true);
});

/** A loopback server answering `/health` with `answer`, or never when it is undefined. */
function service(answer?: () => Response): number {
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		fetch: () => (answer ? answer() : new Promise<Response>(() => {})),
	});
	servers.push(server);
	return server.port as number;
}

test("a service whose process lives but doesn't answer in time is busy, not gone (hung, or still starting)", async () => {
	const port = service();
	expect(await serviceState({ port, token: "t", pid: process.pid })).toBe("busy");
});

test("nothing listening, or another process answering, is down; no frontier.json is down", async () => {
	expect(await serviceState({ port: freePort(), token: "t", pid: process.pid })).toBe("down");
	const port = service(() => Response.json({ pid: process.pid + 1 }));
	expect(await serviceState({ port, token: "t", pid: process.pid })).toBe("down");
	expect(await serviceState(undefined)).toBe("down");
});

test("the service answers up with its herdr restarts, or closing while it closes Jocasta", async () => {
	let closing = false;
	const port = service(() =>
		Response.json({ pid: process.pid, herdrRevivals: 2, ...(closing ? { closing: true } : {}) }),
	);
	expect(await serviceHealth({ port, token: "t", pid: process.pid })).toEqual({ state: "up", herdrRevivals: 2 });
	closing = true;
	expect(await serviceState({ port, token: "t", pid: process.pid })).toBe("closing");
});
