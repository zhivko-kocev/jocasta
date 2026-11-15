// A stand-in for the Jocasta service, for canvas page checks in headless Edge: it serves the real built page with the
// service's content policy, and with `whiteboard`, stands in for whiteboard's server and canvas build too.
import { uiAsset } from "../../../src/frontier/server/assets.ts";
import { pageGuarded } from "../../../src/frontier/server/server.ts";
import { monacoWorker, WORKER_PATH } from "../../../src/frontier/server/whiteboard.ts";

export interface PageServer {
	url: string;
	/** With `whiteboard`: the stand-in catalog `GET /reviews-api` answers. */
	whiteboard: { reviews: FakeReview[] };
	stop(): void;
}

export interface FakeReview {
	reviewId: string;
	title: string;
	kind?: "scratchpad";
}

/** The token the stand-in whiteboard server expects on `/reviews-api/ping`, as the real one expects its own. */
const TOKEN = "fake-token";

/**
 * A stand-in for whiteboard 0.5.2's canvas build: it marks its container as the canvas root, as the real one does, and
 * draws one button naming what it shows (`home:<n>`, `review:<id>`, or the content's kind); on Home the button opens
 * the review `r-1` through the content's `host`. The review `diff` also shows
 * its Diff tab, the host's `diffView` (Monaco, 0411), below the button. The review `peek` shows a code peek citing
 * both sides of its file (base line 1 and head line 1, head the default, as whiteboard groups them), counts "attempts"
 * in it through `find` (`window.__found`), then posts `reveal` for its base line (`window.__revealed`). The review
 * `verbs` asks the bridge what the 0.5.2 canvas asks: a request by path (`window.__ping`, the status), the verbs
 * `authoringCapabilities`, `openApiReview` and `renderAnimation` (`window.__verbs`), and the diff layout, peek layout
 * and inline line numbers before and after setting the diff layout and the line numbers (`window.__settings`).
 */
const FAKE_CANVAS = `export function mountReviewCanvas(container, content) {
	container.classList.add("review-canvas-root");
	let diff;
	let peek;
	const render = (c) => {
		const b = document.createElement("button");
		b.type = "button";
		b.textContent = c.kind === "home" ? "home:" + c.data.reviews.length : c.kind === "api" ? "review:" + c.reviewId : c.kind;
		if (c.kind === "home") b.addEventListener("click", () => c.host.openReview("r-1"));
		diff?.dispose();
		diff = undefined;
		peek?.dispose();
		peek = undefined;
		container.replaceChildren(b);
		if (c.kind === "api" && c.reviewId === "diff") {
			const tab = document.createElement("div");
			tab.className = "fake-diff-tab";
			container.append(tab);
			diff = c.bridge.diffView.create({ container: tab });
		}
		if (c.kind === "api" && c.reviewId === "verbs") {
			const bridge = c.bridge;
			const layouts = () => [bridge.currentDiffLayout("diff"), bridge.currentDiffLayout("peek"), bridge.currentInlineLineNumbers()];
			(async () => {
				window.__ping = (await bridge.request("/reviews-api/ping")).status;
				const post = (name, args) => bridge.post({ name, args });
				window.__verbs = {
					caps: await post("authoringCapabilities", {}),
					open: await post("openApiReview", { reviewId: "verbs", title: "Verbs" }),
					render: await post("renderAnimation", { reviewId: "verbs", targetId: "a", frame: "f" }),
				};
				const before = layouts();
				await bridge.setDiffLayout("diff", "unified");
				await bridge.setInlineLineNumbers(false);
				window.__settings = { before, after: layouts(), settings: c.settings };
			})();
		}
		if (c.kind === "api" && c.reviewId === "peek") {
			const box = document.createElement("div");
			box.className = "fake-peek";
			container.append(box);
			const ranges = [{ startLine: 1, endLine: 1, side: "base" }, { startLine: 1, endLine: 1 }];
			peek = c.bridge.peeks.create({ container: box, path: "src/retry.ts", title: "retry", side: "head", ranges, heightMode: "content", active: true });
			const query = { text: "attempts", matchCase: false, wholeWord: false, isRegex: false };
			c.bridge.peeks.find({ path: "src/retry.ts", side: "head", ranges }, query)
				.then((found) => { window.__found = found.matchCount; })
				.then(() => c.bridge.post({ name: "reveal", args: { path: "src/retry.ts", startLine: 1, endLine: 1, side: "base" } }))
				.then((reply) => { window.__revealed = reply.ok; });
		}
	};
	render(content);
	return { update: render, focus() {}, showFind() { return false; }, dispose() { diff?.dispose(); peek?.dispose(); container.replaceChildren(); container.classList.remove("review-canvas-root"); } };
}
`;

/** The code peek's file (`src/retry.ts`), its base and head text. */
const FAKE_DIFF = {
	path: "src/retry.ts",
	base: "export const attempts = 3;\n",
	head: "export const attempts = 5;\n",
};

/** The review `diff`'s one changed file: the peek's text under a name with markup in it, which must show as text. */
export const DIFF_PATH = "src/<b>retry</b>.ts";

/**
 * Starts the stand-in on a free loopback port. With `whiteboard`, it also stands in for whiteboard's server and canvas
 * build on its own origin; without, `GET /whiteboard/connection` is a 404 and the canvas says it couldn't start.
 */
export function startPageServer(
	options: { whiteboard?: { reviews: FakeReview[]; brokenScript?: boolean } } = {},
): PageServer {
	const whiteboard = { reviews: options.whiteboard?.reviews ?? [] };
	const fakeWhiteboard = async (url: URL, request: Request, port: number): Promise<Response | undefined> => {
		if (!options.whiteboard) return undefined;
		switch (url.pathname) {
			case "/whiteboard/connection":
				return Response.json({
					url: `http://127.0.0.1:${port}`,
					token: TOKEN,
					version: "0.5.2",
					canvas: {
						// `brokenScript`: a canvas script that isn't there, so the page's import of it fails.
						script: options.whiteboard.brokenScript ? "/fake-whiteboard/missing.js" : "/fake-whiteboard/canvas.js",
						styles: [],
						wasm: "/fake-whiteboard/router.wasm",
					},
					worker: WORKER_PATH,
				});
			case "/fake-whiteboard/canvas.js":
				return new Response(FAKE_CANVAS, { headers: { "content-type": "text/javascript" } });
			case "/reviews-api":
				return Response.json(whiteboard.reviews);
			case WORKER_PATH:
				return monacoWorker();
			case "/reviews-api/diff/diff":
				return Response.json([{ path: DIFF_PATH, status: "modified", additions: 1, deletions: 1 }]);
			case "/reviews-api/diff/file":
			case "/reviews-api/peek/file": {
				const path = url.pathname === "/reviews-api/diff/file" ? DIFF_PATH : FAKE_DIFF.path;
				if (url.searchParams.get("file") !== path) return undefined;
				return Response.json({ text: url.searchParams.get("side") === "base" ? FAKE_DIFF.base : FAKE_DIFF.head });
			}
			case "/reviews-api/ping":
				return request.headers.get("x-review-token") === TOKEN
					? Response.json({ ok: true })
					: Response.json({ error: "token" }, { status: 401 });
			default:
				return undefined;
		}
	};
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		async fetch(request, srv) {
			const url = new URL(request.url);
			const asset = (await fakeWhiteboard(url, request, srv.port ?? 0)) ?? (await uiAsset(url.pathname));
			return pageGuarded(asset ?? new Response("not found", { status: 404 }), url.pathname, srv.port ?? 0);
		},
	});
	return {
		url: `http://127.0.0.1:${server.port}/`,
		whiteboard,
		stop() {
			server.stop(true);
		},
	};
}
