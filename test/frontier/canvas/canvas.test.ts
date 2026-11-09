// The canvas page (0552) in his browser: whiteboard's canvas at `/?review=<id>` or Home at `/`, its plain trouble line
// (0432), Jocasta's empty state (0419) and Monaco on Tokyo Night (0411), against a stand-in service in headless Edge
// (skipped where Edge isn't installed).
import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { canvasTrouble } from "../../../src/frontier/canvas/trouble.ts";
import { TOKEN_COOKIE } from "../../../src/frontier/protocol.ts";
import { contentPolicy } from "../../../src/frontier/server/server.ts";
import { canvasDir, startWhiteboard, type Whiteboard } from "../../../src/frontier/server/whiteboard.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { withServer } from "../server/helpers.ts";
import { Browser, EDGE } from "./cdp.ts";
import { type PageServer, startPageServer } from "./page-server.ts";

const env = useTempJocasta();

let open: { server?: PageServer; browser?: Browser } = {};
afterEach(async () => {
	await open.browser?.close();
	open.server?.stop();
	open = {};
});

/** Why whiteboard's server couldn't start, as `/whiteboard/connection` relays it to the page. */
async function readyError(whiteboard: Whiteboard): Promise<string> {
	try {
		await whiteboard.ready;
	} catch (error) {
		return (error as Error).message;
	} finally {
		await whiteboard.stop();
	}
	throw new Error("whiteboard started");
}

test("the canvas's trouble line follows the service's real failures: off, not installed, or a server that died (0432)", async () => {
	// The service runs without whiteboard: retrying can't help, a restart can.
	await withServer(async (server) => {
		const response = await fetch(`http://127.0.0.1:${server.port}/whiteboard/connection`, {
			headers: { cookie: `${TOKEN_COOKIE}=${server.token}` },
		});
		const off = canvasTrouble((await response.json()).error);
		expect(off.reason).toBe("The canvas is off for this run of Jocasta.");
		expect(off.transient).toBe(false);
	});

	// No whiteboard runtime beside the app: a reinstall, not a retry.
	const home = join(env.data, "whiteboard");
	const missing = canvasTrouble(await readyError(startWhiteboard({ runtime: null, home })));
	expect(missing.reason).toBe("Jocasta's whiteboard files are missing.");
	expect(missing.transient).toBe(false);

	// A server that exits on its own (a busy port, a crash) may start on the next try.
	const dist = join(env.root, "dying-whiteboard", "dist");
	mkdirSync(join(dist, "server"), { recursive: true });
	writeFileSync(join(dist, "server", "desktop-host.js"), `console.error("no room"); process.exit(3);`);
	const died = canvasTrouble(
		await readyError(startWhiteboard({ runtime: { node: process.execPath, cli: join(dist, "cli.js") }, home })),
	);
	expect(died.reason).toBe("whiteboard's server didn't start.");
	expect(died.transient).toBe(true);
});

/** A whiteboard Desktop server stand-in that comes up, writes its discovery file and answers until killed. */
const LIVE_WHITEBOARD = `
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const server = Bun.serve({ hostname: "127.0.0.1", port: Number(process.env.DEV_FAST_REVIEW_SERVER_PORT), fetch: () => Response.json({ ok: true }) });
const discovery = { url: "http://127.0.0.1:" + server.port, token: "wb-token", serverPid: process.pid };
const dir = join(process.env.DEV_REVIEW_HOME, "review-desktop", "instances");
mkdirSync(dir, { recursive: true });
writeFileSync(join(dir, "stable.json"), JSON.stringify(discovery));
console.log(JSON.stringify({ event: "ready" }));
`;

test.skipIf(!canvasDir())(
	"once whiteboard's server dies, the page hears it and shows Jocasta's failed state with Try again (0432, B-3)",
	async () => {
		const dist = join(env.root, "live-whiteboard", "dist");
		mkdirSync(join(dist, "server"), { recursive: true });
		writeFileSync(join(dist, "server", "desktop-host.js"), LIVE_WHITEBOARD);
		await withServer(
			async (server) => {
				const connection = () =>
					fetch(`http://127.0.0.1:${server.port}/whiteboard/connection`, {
						headers: { cookie: `${TOKEN_COOKIE}=${server.token}` },
					});
				expect((await connection()).status).toBe(200);
				// Its process ends (killed, as in the campaign); the discovery file stays behind.
				await server.whiteboard?.stop();
				const dead = await connection();
				expect(dead.status).toBe(503);
				const trouble = canvasTrouble((await dead.json()).error);
				expect(trouble.reason).toBe("whiteboard's server stopped answering.");
				expect(trouble.transient).toBe(true);
			},
			{
				whiteboard: {
					runtime: { node: process.execPath, cli: join(dist, "cli.js") },
					home: join(env.data, "whiteboard"),
				},
			},
		);
	},
);

test.skipIf(!existsSync(EDGE))(
	"the boss's `show` link opens its review with the bar, and the Diff tab shows each changed file in Monaco on Tokyo Night, in the page's real bundle (0411, 0552)",
	async () => {
		const server = startPageServer({ whiteboard: { reviews: [] } });
		const browser = await Browser.launch();
		open = { server, browser };
		await browser.send("Page.navigate", { url: `${server.url}?review=diff` });
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'review:diff'`,
			"the review",
		);
		// The bar names the review (the stand-in canvas sets no title) and goes back to the list; no empty state.
		expect(
			await browser.eval<string[]>(`[...document.querySelector('.wb-bar').children].map((e) => e.textContent)`),
		).toEqual(["Reviews", "Review"]);
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-empty, .wb-unavailable').length`)).toBe(0);
		expect(await browser.eval<string>(`document.title`)).toBe("Jocasta");
		await browser.waitFor(
			`document.querySelector('.fake-diff-tab .monaco-diff-editor, .fake-diff-tab .wb-error') !== null`,
			"the Diff tab",
			60_000,
		);
		expect(
			await browser.eval<string | null>(`document.querySelector('.fake-diff-tab .wb-error')?.textContent ?? null`),
		).toBe(null);
		await browser.waitFor(
			`[...document.querySelectorAll('.fake-diff-tab .view-lines')].map((v) => v.textContent.replace(/\\u00a0/g, ' ')).join('|').includes('attempts = 5')`,
			"the head's text",
		);
		// The theme's ground is palette.css's --bg-0, read from the page (0401, 0474).
		expect(
			await browser.eval<string>(
				`getComputedStyle(document.querySelector('.fake-diff-tab .monaco-editor-background')).backgroundColor`,
			),
		).toBe("rgb(26, 27, 38)");

		// Reviews: whiteboard's Home, and the bar goes.
		await browser.eval(`document.querySelector('.wb-bar button').click()`);
		await browser.waitFor(`document.querySelector('.wb-empty') !== null`, "the empty state");
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-bar').length`)).toBe(0);
		expect(browser.errors).toEqual([]);
		expect(await browser.cspViolations()).toEqual([]);
	},
	{ timeout: 120_000 },
);

test.skipIf(!existsSync(EDGE))(
	"before the first review the page shows Jocasta's empty state with the tutorial, and none of the page's element rules reach whiteboard's canvas (0419)",
	async () => {
		const server = startPageServer({
			whiteboard: { reviews: [{ reviewId: "s", title: "Scratchpad", kind: "scratchpad" }] },
		});
		const browser = await Browser.launch();
		open = { server, browser };
		await browser.send("Page.navigate", { url: server.url });
		await browser.waitFor(`document.querySelector('.wb-empty') !== null`, "the empty state");

		// Only the scratchpad: Jocasta's words, not whiteboard's Welcome; the canvas isn't mounted.
		expect(await browser.eval<string>(`document.querySelector('.wb-empty h2').textContent`)).toBe(
			"Nothing on the canvas yet.",
		);
		expect(await browser.eval<string>(`document.querySelector('.wb-empty p').textContent`)).toBe(
			"Ask Jocasta or a thread to draw a diagram or a review; it appears here.",
		);
		expect(await browser.eval<number>(`document.querySelectorAll('.review-canvas-root').length`)).toBe(0);
		expect(await browser.eval<boolean>(`document.querySelector('.wb-mount').hidden`)).toBe(true);
		// One scroll area: the host never scrolls around whiteboard's page.
		expect(await browser.eval<string>(`getComputedStyle(document.querySelector('.wb-mount')).overflow`)).toBe("hidden");
		// No bar until a review is open (0432).
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-bar').length`)).toBe(0);

		// "Take the whiteboard tour" opens whiteboard's tutorial review on the canvas (0432).
		expect(await browser.eval<string>(`document.querySelector('.wb-empty .btn').textContent`)).toBe(
			"Take the whiteboard tour",
		);
		await browser.eval(`document.querySelector('.wb-empty .btn').click()`);
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'review:tutorial'`,
			"the tutorial",
		);
		expect(server.whiteboard.tutorialOpens).toBe(1);
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-empty').length`)).toBe(0);
		expect(await browser.eval<boolean>(`document.querySelector('.wb-mount').hidden`)).toBe(false);

		// The page's button, focus, scrollbar, link and type rules stay in Jocasta's own parts.
		const inCanvas = await browser.eval<Record<string, string>>(
			`(() => { const s = getComputedStyle(document.querySelector('.review-canvas-root button')); const r = getComputedStyle(document.querySelector('.review-canvas-root')); return { transition: s.transitionDuration, border: s.borderTopStyle, scrollbar: r.scrollbarWidth, rendering: r.textRendering }; })()`,
		);
		expect(inCanvas).toEqual({ transition: "0s", border: "outset", scrollbar: "auto", rendering: "auto" });
		expect(
			await browser.eval<string>(`getComputedStyle(document.querySelector('.wb-bar button')).transitionDuration`),
		).toBe("0.18s, 0.18s, 0.18s");

		// Back on Home with a review in the catalog: whiteboard's Home lists it.
		server.whiteboard.reviews.push({ reviewId: "r-1", title: "How retry works" });
		await browser.eval(`document.querySelector('.wb-bar button').click()`);
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'home:2'`,
			"whiteboard's Home",
		);
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-empty').length`)).toBe(0);
		// Home and the tutorial's diagram never load Monaco: it loads with the first code peek or diff (PM-2).
		expect(
			await browser.eval<string[]>(
				`performance.getEntriesByType('resource').map((e) => e.name).filter((n) => /monaco/i.test(n))`,
			),
		).toEqual([]);
		expect(await browser.eval<boolean>(`'MonacoEnvironment' in self`)).toBe(false);

		expect(browser.errors).toEqual([]);
		expect(await browser.cspViolations()).toEqual([]);
	},
	{ timeout: 120_000 },
);

test.skipIf(!existsSync(EDGE))(
	"without whiteboard the page says plainly that the canvas couldn't start, and Try again asks once more (0432)",
	async () => {
		const server = startPageServer();
		const browser = await Browser.launch();
		open = { server, browser };
		await browser.send("Page.navigate", { url: `${server.url}?review=r-1` });
		await browser.waitFor(`document.querySelector('.wb-unavailable h2') !== null`, "the canvas's trouble");
		expect(await browser.eval<string>(`document.querySelector('.wb-unavailable h2').textContent`)).toBe(
			"The canvas couldn't start.",
		);
		expect(await browser.eval<string>(`document.querySelector('.wb-unavailable p').textContent`)).toBe(
			"whiteboard's server didn't start. Try again; if it keeps failing, restart Jocasta.",
		);
		expect(await browser.eval<string>(`document.querySelector('.wb-unavailable code').textContent`)).toBe("404");
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-bar').length`)).toBe(0);
		// Try again starts over: the old line goes, and the new failure brings exactly one back.
		await browser.eval(`window.__oldTrouble = document.querySelector('.wb-unavailable')`);
		await browser.eval(`document.querySelector('.wb-unavailable .btn').click()`);
		await browser.waitFor(
			`document.querySelector('.wb-unavailable') !== null && document.querySelector('.wb-unavailable') !== window.__oldTrouble`,
			"the second try's trouble",
		);
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-unavailable').length`)).toBe(1);
		expect(browser.errors).toEqual([]);
		expect(await browser.cspViolations()).toEqual([]);
	},
	{ timeout: 60_000 },
);

test.skipIf(!existsSync(EDGE))(
	"the page's policy lets libavoid's glue build functions from strings, but runs no inline script (0493)",
	async () => {
		const policy = contentPolicy(0);
		// What libavoid's embind glue does (`reviewLibavoid`): a Trusted Types policy, then `Function` on its strings.
		const glue = `const tt = globalThis.trustedTypes?.createPolicy?.("reviewLibavoid", { createScript: (s) => s });
try { window.__built = Function(tt ? tt.createScript("return 2") : "return 2")(); } catch (e) { window.__built = String(e); }`;
		const srv = Bun.serve({
			hostname: "127.0.0.1",
			port: 0,
			fetch(request) {
				const headers = { "content-security-policy": policy };
				return new URL(request.url).pathname === "/glue.js"
					? new Response(glue, { headers: { ...headers, "content-type": "text/javascript" } })
					: new Response(
							`<!doctype html><script>window.__inline = true;</script><script type="module" src="/glue.js"></script>`,
							{ headers: { ...headers, "content-type": "text/html" } },
						);
			},
		});
		try {
			const browser = await Browser.launch();
			open = { browser };
			await browser.send("Page.navigate", { url: `http://127.0.0.1:${srv.port}/` });
			await browser.waitFor("window.__built !== undefined", "the glue's function");
			expect(await browser.eval<unknown>("window.__built")).toBe(2);
			// The inline script is blocked, and is all the policy blocked.
			expect(await browser.eval<unknown>("window.__inline ?? false")).toBe(false);
			expect((await browser.cspViolations()).map((line) => line.split(" ")[0])).toEqual(["script-src-elem"]);
		} finally {
			srv.stop(true);
		}
	},
	{ timeout: 60_000 },
);
