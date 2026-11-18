// The canvas page (0552) in his browser: whiteboard's canvas at `/?review=<id>` or Home at `/`, its plain trouble line
// (0432), Jocasta's empty state (0419) and Monaco on Tokyo Night (0411), against a stand-in service in headless Edge
// (skipped where Edge isn't installed). One Edge serves the file; each test gets a tab in a context of its own.
import { afterAll, afterEach, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { canvasTrouble } from "../../../src/frontier/canvas/trouble.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { EDGE, Edge, type Tab } from "./cdp.ts";
import { contentPolicy, DIFF_PATH, type PageServer, startPageServer } from "./page-server.ts";

useTempJocasta();

let edge: Edge | undefined;
// Edge.close waits on real processes: up to 5 s for Edge to exit, then up to 30 s for its helpers to let go of the
// profile, which under a loaded suite outlasts the hook's default 5 s.
afterAll(async () => {
	await edge?.close();
}, 60_000);

let open: { server?: PageServer; browser?: Tab } = {};
afterEach(async () => {
	await open.browser?.close();
	open.server?.stop();
	open = {};
});

/** A fresh tab of the file's Edge (launched by the first test that needs one), closed after the test. */
async function newTab(): Promise<Tab> {
	edge ??= await Edge.launch();
	const tab = await edge.tab();
	open.browser = tab;
	return tab;
}

// What the Rust service answers `GET /whiteboard/connection` with when whiteboard can't be used (the messages
// `page_routes_tests.rs` and `whiteboard_tests.rs` assert on their side): the page turns each into one plain line.
test("the canvas's trouble line follows the service's real failures: off, not installed, died, or stopped answering (0432)", () => {
	// The service runs without whiteboard: retrying can't help, a restart can.
	const off = canvasTrouble("whiteboard is off");
	expect(off.reason).toBe("The canvas is off for this run of Jocasta.");
	expect(off.transient).toBe(false);

	// No whiteboard runtime beside the app: a reinstall, not a retry.
	const missing = canvasTrouble(
		"whiteboard isn't built: run `mise run build` (it builds forks/whiteboard), or install Jocasta's release",
	);
	expect(missing.reason).toBe("Jocasta's whiteboard files are missing.");
	expect(missing.transient).toBe(false);

	// A server that exits on its own (a busy port, a crash) may start on the next try.
	const died = canvasTrouble("whiteboard's server exited 3: no room");
	expect(died.reason).toBe("whiteboard's server didn't start.");
	expect(died.transient).toBe(true);

	// Its process ended (killed, as in the campaign) while the discovery file stayed behind.
	const stopped = canvasTrouble("whiteboard's server at http://127.0.0.1:1 stopped answering");
	expect(stopped.reason).toBe("whiteboard's server stopped answering.");
	expect(stopped.transient).toBe(true);
});

test.skipIf(!existsSync(EDGE))(
	"the boss's `show` link opens its review with the bar, and the Diff tab shows each changed file in Monaco on Tokyo Night, in the page's real bundle (0411, 0552)",
	async () => {
		const server = startPageServer({ whiteboard: { reviews: [] } });
		open.server = server;
		const browser = await newTab();
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
		// A file name with markup in it is text, never HTML: no element made of it, nothing the policy blocked (0484).
		expect(
			await browser.eval<[string, number]>(
				`(() => { const p = document.querySelector('.fake-diff-tab .wb-diff-file .path'); return [p.textContent, p.children.length]; })()`,
			),
		).toEqual([DIFF_PATH, 0]);
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
	"a code peek citing both sides of its file reads each range on its own side, find counts both, and reveal opens the base line (0393, 0411)",
	async () => {
		const server = startPageServer({ whiteboard: { reviews: [] } });
		open.server = server;
		const browser = await newTab();
		await browser.send("Page.navigate", { url: `${server.url}?review=peek` });
		await browser.waitFor(
			`document.querySelector('.fake-peek .monaco-editor, .fake-peek.wb-error, .fake-peek .wb-error') !== null`,
			"the peek",
			60_000,
		);
		expect(
			await browser.eval<string | null>(`document.querySelector('.fake-peek .wb-error')?.textContent ?? null`),
		).toBe(null);
		// The default side (head) first, then the base range, each numbered in its own file, a ⋯ line between.
		await browser.waitFor(`document.querySelectorAll('.fake-peek .view-line').length === 3`, "the peek's lines");
		expect(
			await browser.eval<string[]>(
				`[...document.querySelectorAll('.fake-peek .view-line')].sort((a, b) => parseFloat(a.style.top) - parseFloat(b.style.top)).map((v) => v.textContent.replace(/\\u00a0/g, ' '))`,
			),
		).toEqual(["export const attempts = 5;", "", "export const attempts = 3;"]);
		expect(
			await browser.eval<string[]>(
				`[...document.querySelectorAll('.fake-peek .line-numbers')].map((n) => n.textContent)`,
			),
		).toEqual(["1", "⋯", "1"]);

		await browser.waitFor(`window.__revealed !== undefined`, "the reveal");
		expect(await browser.eval<number>(`window.__found`)).toBe(2);
		expect(await browser.eval<boolean>(`window.__revealed`)).toBe(true);
		expect(await browser.eval<string>(`document.querySelector('.wb-source .meta').textContent`)).toBe(
			"line 1 · before the change",
		);
		await browser.waitFor(
			`(document.querySelector('.wb-source .view-lines')?.textContent ?? '').replace(/\\u00a0/g, ' ').includes('attempts = 3')`,
			"the base file in the panel",
		);
		expect(browser.errors).toEqual([]);
		expect(await browser.cspViolations()).toEqual([]);
	},
	{ timeout: 120_000 },
);

test.skipIf(!existsSync(EDGE))(
	"the bridge adds whiteboard's token to a path request, answers the verbs 0.5.2's server relays, keeps the diff and peek layouts apart, and says a render can't return a picture (0615)",
	async () => {
		const server = startPageServer({ whiteboard: { reviews: [] } });
		open.server = server;
		const browser = await newTab();
		await browser.send("Page.navigate", { url: `${server.url}?review=verbs` });
		await browser.waitFor(`window.__settings !== undefined`, "the bridge's answers");
		// The server answers 401 without its token: 200 means the host added it to the path request.
		expect(await browser.eval<number>(`window.__ping`)).toBe(200);
		const verbs = await browser.eval<{
			caps: { ok: boolean; result: Record<string, boolean> };
			open: { ok: boolean; result: Record<string, boolean> };
			render: { ok: boolean; error: string };
		}>(`window.__verbs`);
		// What the server's tool list and `open` read from the canvas: animations on, and the open's answer.
		expect(verbs.caps).toEqual({ ok: true, result: { softwareMapEnabled: true, animationsEnabled: true } });
		expect(verbs.open).toEqual({ ok: true, result: { softwareMapEnabled: true } });
		expect(verbs.render.ok).toBe(false);
		expect(verbs.render.error).toContain("cannot return pictures");
		// Setting the diff layout leaves the peek layout alone; the line numbers follow their own setting.
		expect(await browser.eval<unknown>(`window.__settings`)).toEqual({
			before: ["split", "unified", true],
			after: ["unified", "unified", false],
			settings: { softwareMapEnabled: true },
		});
		expect(browser.errors).toEqual([]);
		expect(await browser.cspViolations()).toEqual([]);
	},
	{ timeout: 60_000 },
);

test.skipIf(!existsSync(EDGE))(
	"before the first review the page shows Jocasta's empty state, and none of the page's element rules reach whiteboard's canvas (0419)",
	async () => {
		const server = startPageServer({
			whiteboard: { reviews: [{ reviewId: "s", title: "Scratchpad", kind: "scratchpad" }] },
		});
		open.server = server;
		const browser = await newTab();
		await browser.send("Page.navigate", { url: server.url });
		await browser.waitFor(`document.querySelector('.wb-empty') !== null`, "the empty state");

		// Only the scratchpad: Jocasta's words, not whiteboard's Welcome; the canvas isn't mounted.
		expect(await browser.eval<string>(`document.querySelector('.wb-empty h2').textContent`)).toBe(
			"Nothing on the canvas yet.",
		);
		expect(await browser.eval<string>(`document.querySelector('.wb-empty p').textContent`)).toBe(
			"Ask Jocasta or a thread to draw a diagram, a review or an animation; it appears here.",
		);
		expect(await browser.eval<number>(`document.querySelectorAll('.review-canvas-root').length`)).toBe(0);
		expect(await browser.eval<boolean>(`document.querySelector('.wb-mount').hidden`)).toBe(true);
		// One scroll area: the host never scrolls around whiteboard's page.
		expect(await browser.eval<string>(`getComputedStyle(document.querySelector('.wb-mount')).overflow`)).toBe("hidden");
		// No bar until a review is open (0432).
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-bar').length`)).toBe(0);

		// With a review in the catalog, Home polls it in and whiteboard's Home replaces the empty state.
		server.whiteboard.reviews.push({ reviewId: "r-1", title: "How retry works" });
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'home:2'`,
			"whiteboard's Home",
			15_000,
		);
		expect(await browser.eval<number>(`document.querySelectorAll('.wb-empty').length`)).toBe(0);
		expect(await browser.eval<boolean>(`document.querySelector('.wb-mount').hidden`)).toBe(false);

		// The page's button, focus, scrollbar, link and type rules stay in Jocasta's own parts.
		const inCanvas = await browser.eval<Record<string, string>>(
			`(() => { const s = getComputedStyle(document.querySelector('.review-canvas-root button')); const r = getComputedStyle(document.querySelector('.review-canvas-root')); return { transition: s.transitionDuration, border: s.borderTopStyle, scrollbar: r.scrollbarWidth, rendering: r.textRendering }; })()`,
		);
		expect(inCanvas).toEqual({ transition: "0s", border: "outset", scrollbar: "auto", rendering: "auto" });

		// A click on Home's review opens it, with the bar; the bar's button goes back to Home.
		await browser.eval(`document.querySelector('.review-canvas-root button').click()`);
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'review:r-1'`,
			"the review",
		);
		expect(
			await browser.eval<string>(`getComputedStyle(document.querySelector('.wb-bar button')).transitionDuration`),
		).toBe("0.18s, 0.18s, 0.18s");
		await browser.eval(`document.querySelector('.wb-bar button').click()`);
		await browser.waitFor(
			`document.querySelector('.review-canvas-root button')?.textContent === 'home:2'`,
			"whiteboard's Home again",
		);
		// Home never loads Monaco: it loads with the first code peek or diff (PM-2).
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
		open.server = server;
		const browser = await newTab();
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
	"when whiteboard's canvas script doesn't load, the page says so, and Try again reloads the page (0581)",
	async () => {
		const server = startPageServer({ whiteboard: { reviews: [], brokenScript: true } });
		open.server = server;
		const browser = await newTab();
		await browser.send("Page.navigate", { url: `${server.url}?review=r-1` });
		await browser.waitFor(`document.querySelector('.wb-unavailable p') !== null`, "the canvas's trouble");
		expect(await browser.eval<string>(`document.querySelector('.wb-unavailable p').textContent`)).toBe(
			"Jocasta's whiteboard files didn't load. Try again; if it keeps failing, restart Jocasta.",
		);
		// The browser keeps the failed import for the page's life: Try again is a fresh page, not a second import.
		await browser.eval(`window.__beforeReload = true`);
		await browser.eval(`document.querySelector('.wb-unavailable .btn').click()`);
		await browser.waitFor(
			`window.__beforeReload === undefined && document.querySelector('.wb-unavailable p') !== null`,
			"the reloaded page's trouble",
		);
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
			const browser = await newTab();
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
