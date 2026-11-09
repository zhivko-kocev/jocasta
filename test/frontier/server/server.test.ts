import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { TOKEN_COOKIE } from "../../../src/frontier/protocol.ts";
import { frontierFilePath, readFrontierFile } from "../../../src/frontier/server/frontier-file.ts";
import { serviceState } from "../../../src/frontier/server/launch.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { Page, startTestServer, TEST_BOSS_KEY, withServer } from "./helpers.ts";

useTempJocasta();

test("refuses requests without the token; ?t=<token> becomes a cookie, keeping the review asked for; a bearer works too (0254)", async () => {
	await withServer(async (server) => {
		const base = `http://127.0.0.1:${server.port}`;
		expect((await fetch(`${base}/`)).status).toBe(403);
		expect((await fetch(`${base}/health`)).status).toBe(403);
		expect((await fetch(`${base}/ws`, { headers: { upgrade: "websocket" } })).status).toBe(403);
		expect((await fetch(`${base}/?t=${"0".repeat(64)}`, { redirect: "manual" })).status).toBe(403);
		// A token of the right length in characters but not in bytes is refused, not an error (SE-3).
		expect((await fetch(`${base}/?t=${"%C3%A9".repeat(64)}`, { redirect: "manual" })).status).toBe(403);
		expect((await fetch(`${base}/health`, { headers: { cookie: `${TOKEN_COOKIE}=wrong` } })).status).toBe(403);
		expect((await fetch(`${base}/health`, { headers: { authorization: "Bearer wrong" } })).status).toBe(403);

		const login = await fetch(`${base}/?t=${server.token}`, { redirect: "manual" });
		expect(login.status).toBe(302);
		expect(login.headers.get("location")).toBe("/");
		const cookie = login.headers.get("set-cookie") ?? "";
		expect(cookie).toContain(`${TOKEN_COOKIE}=${server.token}`);
		expect(cookie).toContain("HttpOnly");
		const review = await fetch(`${base}/?t=${server.token}&review=r-1`, { redirect: "manual" });
		expect(review.headers.get("location")).toBe("/?review=r-1");

		const health = await fetch(`${base}/health`, { headers: { cookie: `${TOKEN_COOKIE}=${server.token}` } });
		expect(await health.json()).toMatchObject({ pid: process.pid, herdrRevivals: expect.any(Number) });
		const bearer = await fetch(`${base}/health`, { headers: { authorization: `Bearer ${server.token}` } });
		expect(bearer.status).toBe(200);

		// A cross-site page carrying the cookie still can't open the socket.
		const crossSite = await fetch(`${base}/ws`, {
			headers: { cookie: `${TOKEN_COOKIE}=${server.token}`, origin: "http://example.org", upgrade: "websocket" },
		});
		expect(crossSite.status).toBe(403);

		const page = await Page.open(server);
		const snapshot = await page.waitFor((m) => m.type === "snapshot");
		expect(snapshot).toMatchObject({
			type: "snapshot",
			hud: { running: 0, blocked: 0, spendToday: 0 },
			boss: "unknown",
			quietHours: false,
		});
		page.close();
	});
});

test("the MCP endpoint takes the service token as a bearer and answers {text, isError}", async () => {
	await withServer(async (server) => {
		const url = `http://127.0.0.1:${server.port}/api/mcp/today`;
		// `today` is a boss tool (SEC-1, 0484): the call carries the boss key.
		const body = JSON.stringify({ args: {}, bossKey: TEST_BOSS_KEY });
		expect((await fetch(url, { method: "POST", body })).status).toBe(401);
		const wrong = await fetch(url, { method: "POST", body, headers: { authorization: "Bearer nope" } });
		expect(wrong.status).toBe(401);
		const ok = await fetch(url, { method: "POST", body, headers: { authorization: `Bearer ${server.token}` } });
		const reply = (await ok.json()) as { text: string; isError?: boolean };
		expect(reply.isError).toBeUndefined();
		expect(reply.text).toStartWith("TODAY · ");
		const unknown = await fetch(`http://127.0.0.1:${server.port}/api/mcp/nope`, {
			method: "POST",
			body,
			headers: { authorization: `Bearer ${server.token}` },
		});
		expect(await unknown.json()).toEqual({ text: "no tool nope.", isError: true });
	});
});

test("only its own Host is answered (DNS rebinding); Graphiti's container may use host.docker.internal for /llm/v1 (SE-3)", async () => {
	await withServer(async (server) => {
		const base = `http://127.0.0.1:${server.port}`;
		const cookie = `${TOKEN_COOKIE}=${server.token}`;
		const evil = await fetch(`${base}/health`, { headers: { cookie, host: `evil.test:${server.port}` } });
		expect(evil.status).toBe(403);
		const local = await fetch(`${base}/health`, { headers: { cookie, host: `localhost:${server.port}` } });
		expect(local.status).toBe(200);
		const docker = { host: `host.docker.internal:${server.port}`, authorization: "Bearer nope" };
		expect((await fetch(`${base}/llm/v1/chat/completions`, { method: "POST", headers: docker })).status).toBe(401);
		expect((await fetch(`${base}/health`, { headers: { cookie, host: docker.host } })).status).toBe(403);
	});
});

test("frontier.json names the server; every client gets the state feed; shutdown stops it (0253, 0254, 0552)", async () => {
	const server = await startTestServer();
	try {
		const file = readFrontierFile();
		expect(file).toEqual({ port: server.port, token: server.token, pid: process.pid });
		expect(await serviceState(file)).toBe("up");

		const page = await Page.open(server);
		const second = await Page.open(server);
		await page.waitFor((m) => m.type === "snapshot");
		await second.waitFor((m) => m.type === "snapshot");
		expect(await server.shutdown({ ask: true })).toBe("stopped");
		await server.stopped;
		expect(existsSync(frontierFilePath())).toBe(false);
		page.close();
		second.close();
	} finally {
		await server.stop();
	}
});

test("Right Ctrl's toggle goes to the boss pane as omp's dictation key (0556)", async () => {
	await withServer(async (server) => {
		const boss = server.herdr.paneList[0]?.paneId;
		await server.service.toggleDictation();
		expect(server.herdr.of("sendKeys")).toEqual([[boss, ["f9"]]]);
	});
});

test("frontier.port is tried first; when busy the service takes a free port (0358)", async () => {
	const blocker = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response("busy") });
	try {
		await withServer(
			async (server) => {
				expect(server.port).not.toBe(blocker.port);
				expect(readFrontierFile()?.port).toBe(server.port);
			},
			{ port: blocker.port as number },
		);
	} finally {
		blocker.stop(true);
	}
});

test("the boss pane is made first in workspace Jocasta in <data>/home, once, with boss.yml and its manual as APPEND_SYSTEM.md (0392, 0404, 0424)", async () => {
	await withServer(async (server) => {
		const made = server.herdr.of("ensureWorkspace");
		expect(made).toHaveLength(1);
		const [label, cwd, command] = made[0] as [string, string, string[]];
		expect(label).toBe("Jocasta");
		expect(cwd).toEndWith("home");
		expect(command.slice(0, 3)).toEqual(["omp", "--continue", "--config"]);
		expect(command[3]).toEndWith("boss.yml");
		expect(existsSync(command[3] as string)).toBe(true);
		const manual = join(cwd, ".omp", "APPEND_SYSTEM.md");
		expect(readFileSync(manual, "utf8")).toBe(
			readFileSync(join(dirname(command[3] as string), "boss-manual.md"), "utf8"),
		);
		expect(server.herdr.workspaceList[0]?.label).toBe("Jocasta");
		expect(server.service.bossState()).toBe("unknown");
	});
});
