import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { TOKEN_COOKIE } from "../../../src/frontier/protocol.ts";
import {
	CANVAS_PATH,
	canvasAsset,
	canvasFiles,
	startWhiteboard,
	type WhiteboardRuntime,
} from "../../../src/frontier/server/whiteboard.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { withServer } from "./helpers.ts";

const env = useTempJocasta();

/**
 * Stands in for `node <whiteboard>/dist/server/desktop-host.js` (0393, 0419): listens on `DEV_FAST_REVIEW_SERVER_PORT`,
 * writes `<DEV_REVIEW_HOME>/review-desktop/instances/stable.json`, says `ready`, serves `/health`, and records its
 * environment. `FAKE_WHITEBOARD_FAIL` exits 3 at once.
 */
const FAKE = `
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const home = process.env.DEV_REVIEW_HOME;
if (process.env.FAKE_WHITEBOARD_FAIL) { console.error("no room"); process.exit(3); }
const port = Number(process.env.DEV_FAST_REVIEW_SERVER_PORT);
const server = Bun.serve({ hostname: "127.0.0.1", port, fetch: () => Response.json({ ok: true }) });
const discovery = { version: 3, url: "http://127.0.0.1:" + server.port, token: "wb-token", serverPid: process.pid, key: "stable" };
mkdirSync(join(home, "review-desktop", "instances"), { recursive: true });
writeFileSync(join(home, "review-desktop", "instances", "stable.json"), JSON.stringify(discovery));
writeFileSync(join(home, "env.json"), JSON.stringify({ argv: process.argv.slice(2), DO_NOT_TRACK: process.env.DO_NOT_TRACK, TELEMETRY: process.env.DEV_FAST_REVIEW_TELEMETRY_DISABLED, SERVER_DIR: process.env.DEV_REVIEW_SERVER_DIR ?? null, APP_PID: process.env.DEV_FAST_REVIEW_APP_PID }));
console.log(JSON.stringify({ event: "ready", ...discovery }));
`;

/** A runtime whose `cli.js` sits beside `server/desktop-host.js`, as in whiteboard's `dist/`. */
function fakeRuntime(): WhiteboardRuntime {
	const dist = join(env.root, "fake-whiteboard", "dist");
	mkdirSync(join(dist, "server"), { recursive: true });
	writeFileSync(join(dist, "server", "desktop-host.js"), FAKE);
	return { node: process.execPath, cli: join(dist, "cli.js") };
}

test("the Desktop server starts with telemetry off on its own state and port, is reused while healthy, and stops on quit (0393, 0419)", async () => {
	const home = join(env.data, "whiteboard");
	process.env.DEV_REVIEW_SERVER_DIR = join(env.root, "elsewhere");
	try {
		const whiteboard = startWhiteboard({ runtime: fakeRuntime(), home });
		const port = Number(new URL(whiteboard.origin()).port);
		const connection = await whiteboard.ready;
		expect(connection.url).toBe(`http://127.0.0.1:${port}`);
		expect(connection.token).toBe("wb-token");
		const seen = await Bun.file(join(home, "env.json")).json();
		expect(seen).toEqual({
			argv: [],
			DO_NOT_TRACK: "1",
			TELEMETRY: "1",
			SERVER_DIR: null,
			APP_PID: String(process.pid),
		});
		expect((await fetch(`${connection.url}/health`)).ok).toBe(true);
		// A second companion on the same state uses the running server.
		const second = startWhiteboard({ runtime: fakeRuntime(), home });
		expect(await second.ready).toEqual(connection);
		expect(second.origin()).toBe(connection.url);
		await second.stop();
		expect((await fetch(`${connection.url}/health`)).ok).toBe(true);
		await whiteboard.stop();
		await expect(fetch(`${connection.url}/health`)).rejects.toThrow();
	} finally {
		delete process.env.DEV_REVIEW_SERVER_DIR;
	}
});

test("a server that exits says why; a missing build says how to build it", async () => {
	process.env.FAKE_WHITEBOARD_FAIL = "1";
	try {
		const failed = startWhiteboard({ runtime: fakeRuntime(), home: join(env.data, "whiteboard") });
		await expect(failed.ready).rejects.toThrow("whiteboard's server exited 3: no room");
	} finally {
		delete process.env.FAKE_WHITEBOARD_FAIL;
	}
	const missing = startWhiteboard({ runtime: null, home: join(env.data, "whiteboard") });
	await expect(missing.ready).rejects.toThrow("whiteboard isn't built: run `bun run build`");
});

test("the canvas build's entry, stylesheets and wasm come from vite's manifest; files outside assets/ aren't served", () => {
	const dir = join(env.root, "canvas");
	mkdirSync(join(dir, ".vite"), { recursive: true });
	mkdirSync(join(dir, "assets"), { recursive: true });
	writeFileSync(
		join(dir, ".vite", "manifest.json"),
		JSON.stringify({
			"src/desktop-entry.tsx": {
				file: "assets/canvas-1.js",
				name: "canvas",
				isEntry: true,
				css: ["assets/canvas-1.css"],
				assets: ["assets/libavoid-1.wasm", "assets/font.woff2"],
			},
		}),
	);
	writeFileSync(join(dir, "assets", "canvas-1.js"), "export {}");
	expect(canvasFiles(dir)).toEqual({
		script: `${CANVAS_PATH}assets/canvas-1.js`,
		styles: [`${CANVAS_PATH}assets/canvas-1.css`],
		wasm: `${CANVAS_PATH}assets/libavoid-1.wasm`,
	});
	expect(canvasAsset(dir, `${CANVAS_PATH}assets/canvas-1.js`)?.headers.get("content-type")).toContain(
		"text/javascript",
	);
	expect(canvasAsset(dir, `${CANVAS_PATH}.vite/manifest.json`)).toBeUndefined();
	expect(canvasAsset(dir, `${CANVAS_PATH}assets/..%2F..%2Fsecret`)).toBeUndefined();
});

test("the page gets whiteboard's server behind its token, and the CSP lets it connect there (0393)", async () => {
	await withServer(
		async (server) => {
			const cookie = { cookie: `${TOKEN_COOKIE}=${server.token}` };
			const base = `http://127.0.0.1:${server.port}`;
			expect((await fetch(`${base}/whiteboard/connection`)).status).toBe(403);
			const page = await fetch(`${base}/whiteboard/connection`, { headers: cookie });
			const origin = server.whiteboard?.origin() ?? "";
			expect(page.headers.get("content-security-policy")).toContain(
				`connect-src 'self' ws://127.0.0.1:${server.port} ${origin}`,
			);
			const body = await page.json();
			// The repo's clone may be unbuilt in a test run: then the page hears why.
			if (page.status === 200) {
				expect(body).toMatchObject({ url: origin, token: "wb-token", version: "0.2.4" });
				expect(body.canvas.script).toStartWith(CANVAS_PATH);
			} else {
				expect(page.status).toBe(503);
				expect(body.error).toContain("canvas isn't built");
			}
		},
		{ whiteboard: { runtime: fakeRuntime(), home: join(env.data, "whiteboard") } },
	);
});

test("with whiteboard off the page hears so and the CSP names only this server", async () => {
	await withServer(async (server) => {
		const response = await fetch(`http://127.0.0.1:${server.port}/whiteboard/connection`, {
			headers: { cookie: `${TOKEN_COOKIE}=${server.token}` },
		});
		expect(response.status).toBe(503);
		expect((await response.json()).error).toBe("whiteboard is off");
		expect(response.headers.get("content-security-policy")).toContain(
			`connect-src 'self' ws://127.0.0.1:${server.port};`,
		);
	});
});
