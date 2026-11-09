import type { Server, WebSocketHandler } from "bun";
import { sameToken } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import { isLlmPath } from "./llm-route.ts";

/** One route of the service's server: the paths it answers, and its answer. */
export interface Route {
	matches(path: string): boolean;
	/** Its answer; `undefined` once the request became a WebSocket. */
	handle(request: Request, url: URL, server: Server<undefined>): Response | undefined | Promise<Response | undefined>;
}

/** A route answered among the pages (after the launch code, before the cookie check): always a response. */
export interface PageRoute {
	matches(path: string): boolean;
	handle(request: Request, url: URL): Response | Promise<Response>;
}

/** The Hosts this server answers as (DNS rebinding, SE-3); Graphiti's container may also use its docker name. */
export function hostAllowed(host: string | null, port: number, path: string): boolean {
	return (
		host === `127.0.0.1:${port}` ||
		host === `localhost:${port}` ||
		(host === `host.docker.internal:${port}` && isLlmPath(path))
	);
}

/** The bearer of `request`'s Authorization header; empty when none. */
export function bearerToken(request: Request): string {
	const auth = request.headers.get("authorization") ?? "";
	return auth.startsWith("Bearer ") ? auth.slice(7) : "";
}

/** Whether `request` carries the API token as its bearer (every API call; SEC-NET-4). */
export function hasBearer(request: Request, token: string): boolean {
	return sameToken(bearerToken(request), token);
}

/**
 * The prelude of the API's POST routes: 401 without the API token as the bearer, 405 for another method, each as the
 * tools' `{text, isError}` with `json`, else as plain text; `undefined` when the request goes on.
 */
export function refusedPost(request: Request, token: string, json: boolean): Response | undefined {
	const refuse = (text: string, status: number) =>
		json ? Response.json({ text, isError: true }, { status }) : new Response(text, { status });
	if (!hasBearer(request, token)) return refuse("invalid token", 401);
	if (request.method !== "POST") return refuse("POST only", 405);
	return undefined;
}

/** `value` when it is a plain object (a tool's or command's `args`), else none. */
export function objectArgs(value: unknown): Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

/** The request's JSON body when it is an object, else `{}` (no body, not JSON, or not an object). */
export async function bodyObject(request: Request): Promise<object> {
	try {
		const parsed: unknown = await request.json();
		if (parsed !== null && typeof parsed === "object") return parsed;
	} catch {
		// No body: nothing in it.
	}
	return {};
}

/** What the server answers: the API's routes in order, then the pages; the state feed's socket. */
export interface Served {
	routes: readonly Route[];
	/** Every other request, with the server's port. */
	page(request: Request, url: URL, port: number): Promise<Response>;
	websocket: WebSocketHandler<undefined>;
}

/**
 * The server on 127.0.0.1:`wanted`, or on a free port for this run when that one is busy (another program, or another
 * Jocasta's data dir; 0358). Each request: the host check, then the first route that answers its path, else the page.
 */
export function listen(wanted: number, { routes, page, websocket }: Served): Server<undefined> {
	const serve = (port: number): Server<undefined> =>
		Bun.serve<undefined>({
			hostname: "127.0.0.1",
			port,
			development: false,
			error(error) {
				log("error", "server", error.stack ?? String(error));
				return new Response("Internal error", { status: 500 });
			},
			async fetch(request, server) {
				const url = new URL(request.url);
				const at = server.port ?? port;
				if (!hostAllowed(request.headers.get("host"), at, url.pathname)) {
					return new Response("Forbidden", { status: 403 });
				}
				const route = routes.find((r) => r.matches(url.pathname));
				return route ? route.handle(request, url, server) : page(request, url, at);
			},
			websocket,
		});
	try {
		return serve(wanted);
	} catch {
		const server = serve(0);
		log("warn", "server", `port ${wanted} (service.port) was busy; Jocasta is on port ${server.port} for this run`);
		return server;
	}
}
