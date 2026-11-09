import { randomBytes } from "node:crypto";
import { sameToken } from "../../shared/fsx.ts";
import { TOKEN_QUERY, tokenCookie } from "../protocol.ts";
import { uiAsset } from "./assets.ts";
import { hasBearer, type PageRoute } from "./routes.ts";
import type { ServiceOptions } from "./service.ts";
import {
	CANVAS_PATH,
	CONNECTION_PATH,
	canvasAsset,
	canvasDir,
	canvasFiles,
	monacoWorker,
	WHITEBOARD_VERSION,
	type Whiteboard,
	WORKER_PATH,
} from "./whiteboard.ts";

/** How long a page URL's one-time launch code may wait for the browser to open it (SEC-LOC-11). */
const LAUNCH_CODE_MS = 60_000;

/** One-time launch codes of the page URLs handed out, with when each expires (SEC-LOC-11). */
export class LaunchCodes {
	private readonly codes = new Map<string, number>();

	constructor(private readonly now: () => number = Date.now) {}

	/** A fresh code, live for a minute. */
	mint(): string {
		const now = this.now();
		for (const [code, until] of this.codes) if (until <= now) this.codes.delete(code);
		const code = randomBytes(32).toString("hex");
		this.codes.set(code, now + LAUNCH_CODE_MS);
		return code;
	}

	/** Takes `given` as a live launch code once. */
	take(given: string): boolean {
		const now = this.now();
		for (const [code, until] of this.codes) {
			if (!sameToken(given, code)) continue;
			this.codes.delete(code);
			return until > now;
		}
		return false;
	}
}

/** The value of `cookie` in the request's Cookie header. */
export function cookieValue(request: Request, cookie: string): string | undefined {
	const header = request.headers.get("cookie") ?? "";
	for (const part of header.split(";")) {
		const [name, ...value] = part.trim().split("=");
		if (name === cookie) return value.join("=");
	}
	return undefined;
}

/**
 * What a browser that opens a page without a valid cookie sees (0581, UX-9): the page is from an earlier run (every
 * run has its own page token), and what brings a fresh one. No token is in it.
 */
const EARLIER_RUN_PAGE = `<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Jocasta</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#1a1b26;color:#c0caf5;font:16px/1.5 system-ui,sans-serif}p{max-width:36em;padding:0 1.5em}</style>
</head>
<body><p>This Jocasta page is from an earlier run. Ask the boss to show the review again; it opens a fresh page.</p></body>
</html>
`;

/** A refused request: the earlier-run page for a browser's navigation, else a plain 403. */
export function refusedPage(request: Request): Response {
	return (request.headers.get("accept") ?? "").includes("text/html")
		? new Response(EARLIER_RUN_PAGE, { status: 403, headers: { "content-type": "text/html; charset=utf-8" } })
		: new Response("Forbidden", { status: 403 });
}

/**
 * What the canvas page mounts whiteboard's canvas with (0393): the server's url and token, once it answers, and the
 * canvas build's files; 503 with the reason when either is missing, or when the server stopped answering since (its
 * process died, B-3): the page then shows its own failed state, not whiteboard's.
 */
export async function whiteboardConnection(whiteboard: Whiteboard | undefined): Promise<Response> {
	const dir = canvasDir();
	if (!whiteboard || !dir) {
		return Response.json(
			{ error: whiteboard ? "whiteboard's canvas isn't built: run `bun run build`" : "whiteboard is off" },
			{ status: 503 },
		);
	}
	try {
		const { url, token: wbToken } = await whiteboard.ready;
		if ((await whiteboard.check()).state !== "up") {
			return Response.json({ error: `whiteboard's server at ${url} stopped answering` }, { status: 503 });
		}
		return Response.json({
			url,
			token: wbToken,
			version: WHITEBOARD_VERSION,
			canvas: canvasFiles(dir),
			worker: WORKER_PATH,
		});
	} catch (error) {
		return Response.json({ error: (error as Error).message }, { status: 503 });
	}
}

/**
 * The boss's `show` with whiteboard (0552): it checks the review exists first (whiteboard's `GET /reviews-api/:id`,
 * 404 when none), then opens it in his browser on this server's canvas page with a fresh one-time launch code.
 */
export function whiteboardReviews(
	whiteboard: Whiteboard,
	open: (query: string) => void,
): Required<Pick<ServiceOptions, "findReview" | "openReview">> {
	return {
		findReview: async (id: string) => {
			const { url, token: wbToken } = await whiteboard.ready;
			const response = await fetch(`${url}/reviews-api/${encodeURIComponent(id)}?full=true`, {
				headers: { "x-review-token": wbToken },
				signal: AbortSignal.timeout(10_000),
			});
			if (response.status === 404) return null;
			if (!response.ok) throw new Error(`whiteboard answered ${response.status}`);
			const body: unknown = await response.json();
			const title = body && typeof body === "object" && "title" in body ? body.title : undefined;
			return { title: typeof title === "string" && title ? title : id };
		},
		openReview: (id: string) => open(`&review=${encodeURIComponent(id)}`),
	};
}

/** What the pages need. */
export interface PageParts {
	token: string;
	/** The browser's own token (SEC-NET-4): its cookie authorizes the pages and whiteboard's connection only. */
	pageToken: string;
	codes: LaunchCodes;
	/** The server's port, for the cookie's name. */
	port: () => number;
	whiteboard: Whiteboard | undefined;
	/** The bearer's routes answered here, after the launch code: `/health` and his questions. */
	bearerRoutes: readonly PageRoute[];
}

/**
 * The bearer's routes (`/health`, his questions) and the browser's pages: `/?t=<code>` swaps a live launch code for
 * the page cookie; the pages and their files need the cookie (or the bearer).
 */
export function pageResponder(parts: PageParts): (request: Request, url: URL) => Promise<Response> {
	const { token, pageToken, codes, whiteboard } = parts;
	/** The page token as this port's cookie (the browser's pages and whiteboard's connection only; SEC-NET-4). */
	const pageCookie = (request: Request) => sameToken(cookieValue(request, tokenCookie(parts.port())), pageToken);
	return async (request, url) => {
		const fromQuery = url.searchParams.get(TOKEN_QUERY);
		if (fromQuery !== null) {
			if (!codes.take(fromQuery)) return refusedPage(request);
			const rest = new URLSearchParams(url.searchParams);
			rest.delete(TOKEN_QUERY);
			const query = rest.toString();
			return new Response(null, {
				status: 302,
				headers: {
					location: `/${query ? `?${query}` : ""}`,
					"set-cookie": `${tokenCookie(parts.port())}=${pageToken}; Path=/; HttpOnly; SameSite=Strict`,
				},
			});
		}
		const route = parts.bearerRoutes.find((r) => r.matches(url.pathname));
		if (route) return route.handle(request, url);
		if (!hasBearer(request, token) && !pageCookie(request)) return refusedPage(request);
		if (request.method !== "GET") return new Response("Not found", { status: 404 });
		if (url.pathname === CONNECTION_PATH) return whiteboardConnection(whiteboard);
		if (url.pathname === WORKER_PATH) return (await monacoWorker()) ?? new Response("Not found", { status: 404 });
		const canvas = url.pathname.startsWith(CANVAS_PATH) ? canvasDir() : undefined;
		const served = canvas ? canvasAsset(canvas, url.pathname) : await uiAsset(url.pathname);
		return served ?? new Response("Not found", { status: 404 });
	};
}
