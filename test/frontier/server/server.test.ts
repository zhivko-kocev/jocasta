import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tokenCookie } from "../../../src/frontier/protocol.ts";
import { terminalArgv } from "../../../src/frontier/server/close.ts";
import { serviceState } from "../../../src/frontier/server/launch.ts";
import { cookieValue, LaunchCodes, refusedPage } from "../../../src/frontier/server/page-routes.ts";
import { bearerToken, hostAllowed, objectArgs } from "../../../src/frontier/server/routes.ts";
import { readServiceFile, serviceFilePath } from "../../../src/shared/service-file.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { Page, startTestServer, TEST_BOSS_KEY, withServer } from "./helpers.ts";

useTempJocasta();

test("the API takes the token as a bearer only; a page's one-time code becomes its own per-port cookie, never the token (0254, SEC-NET-4, SEC-LOC-11)", async () => {
	await withServer(async (server) => {
		const base = `http://127.0.0.1:${server.port}`;
		const bearer = { authorization: `Bearer ${server.token}` };
		expect((await fetch(`${base}/`)).status).toBe(403);
		expect((await fetch(`${base}/health`)).status).toBe(403);
		expect((await fetch(`${base}/ws`, { headers: { upgrade: "websocket" } })).status).toBe(403);
		expect((await fetch(`${base}/?t=${"0".repeat(64)}`, { redirect: "manual" })).status).toBe(403);
		// A token of the right length in characters but not in bytes is refused, not an error (SE-3).
		expect((await fetch(`${base}/?t=${"%C3%A9".repeat(64)}`, { redirect: "manual" })).status).toBe(403);
		// The API token is no page code, and no cookie.
		expect((await fetch(`${base}/?t=${server.token}`, { redirect: "manual" })).status).toBe(403);
		const tokenCookieHeader = { cookie: `${tokenCookie(server.port)}=${server.token}` };
		expect((await fetch(`${base}/health`, { headers: tokenCookieHeader })).status).toBe(403);
		expect((await fetch(`${base}/health`, { headers: { authorization: "Bearer wrong" } })).status).toBe(403);

		const url = server.url;
		expect(url).toMatch(new RegExp(`^${base}/\\?t=[0-9a-f]{64}$`));
		expect(url).not.toContain(server.token);
		const login = await fetch(url, { redirect: "manual" });
		expect(login.status).toBe(302);
		expect(login.headers.get("location")).toBe("/");
		const setCookie = login.headers.get("set-cookie") ?? "";
		expect(setCookie).toStartWith(`${tokenCookie(server.port)}=`);
		expect(setCookie).toContain("HttpOnly");
		expect(setCookie).not.toContain(server.token);
		// The code is single use.
		expect((await fetch(url, { redirect: "manual" })).status).toBe(403);
		const review = await fetch(`${server.url}&review=r-1`, { redirect: "manual" });
		expect(review.headers.get("location")).toBe("/?review=r-1");
		const cookie = { cookie: setCookie.split(";")[0] as string };

		// The page cookie opens the pages, nothing of the API.
		expect((await fetch(`${base}/whiteboard/connection`, { headers: cookie })).status).toBe(503);
		expect((await fetch(`${base}/health`, { headers: cookie })).status).toBe(403);
		expect((await fetch(`${base}/ws`, { headers: { ...cookie, upgrade: "websocket" } })).status).toBe(403);

		const health = await fetch(`${base}/health`, { headers: bearer });
		expect(await health.json()).toMatchObject({
			pid: process.pid,
			herdrRevivals: expect.any(Number),
			layer: "on",
			hosts: "on",
		});

		// A cross-site page can't open the socket.
		const crossSite = await fetch(`${base}/ws`, {
			headers: { ...bearer, origin: "http://example.org", upgrade: "websocket" },
		});
		expect(crossSite.status).toBe(403);

		const page = await Page.open(server);
		const snapshot = await page.waitFor((m) => m.type === "snapshot");
		expect(snapshot).toMatchObject({ type: "snapshot", hud: { running: 0, blocked: 0, spendToday: 0, quiet: false } });
		page.close();
	});
});

test("a browser's page from an earlier run gets a page saying so and what to do, with no token in it (0581, UX-9)", async () => {
	await withServer(async (server) => {
		const response = await fetch(`http://127.0.0.1:${server.port}/?review=r-1`, {
			headers: { accept: "text/html,application/xhtml+xml", cookie: `${tokenCookie(server.port)}=stale` },
		});
		expect(response.status).toBe(403);
		expect(response.headers.get("content-type")).toContain("text/html");
		const html = await response.text();
		expect(html).toContain(
			"This Jocasta page is from an earlier run. Ask the boss to show the review again; it opens a fresh page.",
		);
		expect(html).not.toContain(server.token);
	});
});

test("a start that fails after its relay and server listen stops them: both ports are free again (SRV-A-2)", async () => {
	const free = () => {
		const probe = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
		const port = probe.port as number;
		probe.stop(true);
		return port;
	};
	const relayPort = free();
	const port = free();
	// jocasta-service.json can't be written: a folder holds its name.
	mkdirSync(serviceFilePath(), { recursive: true });
	await expect(startTestServer({ relayPort, port })).rejects.toThrow();
	for (const taken of [relayPort, port]) {
		const again = Bun.serve({ hostname: "127.0.0.1", port: taken, fetch: () => new Response() });
		expect(again.port).toBe(taken);
		again.stop(true);
	}
});

test("the close question's terminal: Windows Terminal when installed, else the command itself in a console of its own (SEC-NET-9)", () => {
	const command = ["C:\\Program Files\\Jocasta\\jocasta.exe"];
	expect(terminalArgv(command, "C:\\wt.exe")).toEqual(["C:\\wt.exe", "-w", "new", "--title", "Jocasta", ...command]);
	expect(terminalArgv(command, null)).toEqual(command);
});

test("a page URL's launch code is taken once, within its minute (SEC-LOC-11)", () => {
	let now = 1_000;
	const codes = new LaunchCodes(() => now);
	const first = codes.mint();
	expect(codes.take(first)).toBe(true);
	expect(codes.take(first)).toBe(false);
	expect(codes.take("nope")).toBe(false);
	const late = codes.mint();
	now += 60_000;
	expect(codes.take(late)).toBe(false);
});

test("the request helpers: cookie by name, bearer, the Hosts answered, args only as an object, the earlier-run page for a browser", async () => {
	const request = (headers: Record<string, string>) => new Request("http://127.0.0.1/", { headers });
	expect(cookieValue(request({ cookie: "a=1; jocasta_service_9=x=y" }), "jocasta_service_9")).toBe("x=y");
	expect(cookieValue(request({ cookie: "a=1" }), "b")).toBeUndefined();
	expect(bearerToken(request({ authorization: "Bearer abc" }))).toBe("abc");
	expect(bearerToken(request({ authorization: "Basic abc" }))).toBe("");
	expect(hostAllowed("127.0.0.1:9", 9, "/")).toBe(true);
	expect(hostAllowed("localhost:9", 9, "/")).toBe(true);
	expect(hostAllowed("127.0.0.1:8", 9, "/")).toBe(false);
	expect(hostAllowed("host.docker.internal:9", 9, "/llm/v1/chat/completions")).toBe(true);
	expect(hostAllowed("host.docker.internal:9", 9, "/llm/v10")).toBe(false);
	expect(hostAllowed("host.docker.internal:9", 9, "/health")).toBe(false);
	expect(objectArgs({ a: 1 })).toEqual({ a: 1 });
	for (const value of [null, [1], "x", undefined]) expect(objectArgs(value)).toEqual({});
	const page = refusedPage(request({ accept: "text/html" }));
	expect(page.status).toBe(403);
	expect(await page.text()).toContain("This Jocasta page is from an earlier run.");
	expect(await refusedPage(request({})).text()).toBe("Forbidden");
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
		const authorization = `Bearer ${server.token}`;
		const evil = await fetch(`${base}/health`, { headers: { authorization, host: `evil.test:${server.port}` } });
		expect(evil.status).toBe(403);
		const local = await fetch(`${base}/health`, { headers: { authorization, host: `localhost:${server.port}` } });
		expect(local.status).toBe(200);
		const docker = { host: `host.docker.internal:${server.port}`, authorization: "Bearer nope" };
		expect((await fetch(`${base}/llm/v1/chat/completions`, { method: "POST", headers: docker })).status).toBe(401);
		expect((await fetch(`${base}/health`, { headers: { authorization, host: docker.host } })).status).toBe(403);
	});
});

test("jocasta-service.json names the server; every client gets the state feed; shutdown stops it (0253, 0254, 0552)", async () => {
	const server = await startTestServer();
	try {
		const file = readServiceFile();
		expect(file).toEqual({ port: server.port, token: server.token, pid: process.pid });
		expect(await serviceState(file)).toBe("up");

		const page = await Page.open(server);
		const second = await Page.open(server);
		await page.waitFor((m) => m.type === "snapshot");
		await second.waitFor((m) => m.type === "snapshot");
		expect(await server.shutdown({ ask: true })).toBe("stopped");
		await server.stopped;
		expect(existsSync(serviceFilePath())).toBe(false);
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

test("service.port is tried first; when busy the service takes a free port (0358)", async () => {
	const blocker = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response("busy") });
	try {
		await withServer(
			async (server) => {
				expect(server.port).not.toBe(blocker.port);
				expect(readServiceFile()?.port).toBe(server.port);
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
