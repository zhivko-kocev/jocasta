// A stand-in for the Jocasta service, for canvas page checks in headless Edge: it serves the real built page with the
// service's content policy, and with `whiteboard`, stands in for whiteboard's server and canvas build too.
import { uiAsset } from "../../../src/frontier/server/assets.ts";
import { contentPolicy } from "../../../src/frontier/server/server.ts";
import { monacoWorker, WORKER_PATH } from "../../../src/frontier/server/whiteboard.ts";

export interface PageServer {
	url: string;
	/** With `whiteboard`: the stand-in catalog `GET /reviews-api` answers, and how often the tutorial was opened. */
	whiteboard: { reviews: FakeReview[]; tutorialOpens: number };
	stop(): void;
}

export interface FakeReview {
	reviewId: string;
	title: string;
	kind?: "scratchpad";
}

/**
 * A stand-in for whiteboard's canvas build: it marks its container as the canvas root, as the real one does, and draws
 * one button naming what it shows (`home:<n>`, `review:<id>`, or the content's kind). The review `diff` also shows
 * its Diff tab, the host's `diffView` (Monaco, 0411), below the button.
 */
const FAKE_CANVAS = `export function mountReviewCanvas(container, content) {
	container.classList.add("review-canvas-root");
	let diff;
	const render = (c) => {
		const b = document.createElement("button");
		b.type = "button";
		b.textContent = c.kind === "home" ? "home:" + c.reviews.length : c.kind === "api" ? "review:" + c.reviewId : c.kind;
		diff?.dispose();
		diff = undefined;
		container.replaceChildren(b);
		if (c.kind === "api" && c.reviewId === "diff") {
			const tab = document.createElement("div");
			tab.className = "fake-diff-tab";
			container.append(tab);
			diff = c.bridge.diffView.create({ container: tab });
		}
	};
	render(content);
	return { update: render, focus() {}, showFind() { return false; }, dispose() { diff?.dispose(); container.replaceChildren(); container.classList.remove("review-canvas-root"); } };
}
`;

/** The review `diff`'s one changed file, its base and head text. */
const FAKE_DIFF = {
	path: "src/retry.ts",
	base: "export const attempts = 3;\n",
	head: "export const attempts = 5;\n",
};

/**
 * Starts the stand-in on a free loopback port. With `whiteboard`, it also stands in for whiteboard's server and canvas
 * build on its own origin; without, `GET /whiteboard/connection` is a 404 and the canvas says it couldn't start.
 */
export function startPageServer(options: { whiteboard?: { reviews: FakeReview[] } } = {}): PageServer {
	const whiteboard = { reviews: options.whiteboard?.reviews ?? [], tutorialOpens: 0 };
	const fakeWhiteboard = async (url: URL, request: Request, port: number): Promise<Response | undefined> => {
		if (!options.whiteboard) return undefined;
		switch (url.pathname) {
			case "/whiteboard/connection":
				return Response.json({
					url: `http://127.0.0.1:${port}`,
					token: "fake-token",
					version: "0.2.4",
					canvas: { script: "/fake-whiteboard/canvas.js", styles: [], wasm: "/fake-whiteboard/router.wasm" },
					worker: WORKER_PATH,
				});
			case "/fake-whiteboard/canvas.js":
				return new Response(FAKE_CANVAS, { headers: { "content-type": "text/javascript" } });
			case "/reviews-api":
				return Response.json(whiteboard.reviews);
			case WORKER_PATH:
				return monacoWorker();
			case "/reviews-api/diff/diff":
				return Response.json([{ path: FAKE_DIFF.path, status: "modified", additions: 1, deletions: 1 }]);
			case "/reviews-api/diff/file":
				if (url.searchParams.get("file") !== FAKE_DIFF.path) return undefined;
				return Response.json({ text: url.searchParams.get("side") === "base" ? FAKE_DIFF.base : FAKE_DIFF.head });
			case "/tutorial/open":
				if (request.method !== "POST") return undefined;
				whiteboard.tutorialOpens++;
				return Response.json({ reviewUuid: "tutorial" });
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
			const response = asset ?? new Response("not found", { status: 404 });
			response.headers.set("content-security-policy", contentPolicy(srv.port ?? 0));
			response.headers.set("cross-origin-opener-policy", "same-origin");
			response.headers.set("cross-origin-embedder-policy", "require-corp");
			response.headers.set("cross-origin-resource-policy", "same-origin");
			return response;
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
