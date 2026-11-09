import type { Server, WebSocketHandler } from "bun";
import { type ClientMessage, type ServerMessage, WS_PATH } from "../protocol.ts";
import { hasBearer, type Route } from "./routes.ts";
import type { JocastaService } from "./service.ts";

/** The pub/sub topic every client of the state feed subscribes to. */
const CLIENTS = "clients";

/** Sends `message` to every client of the state feed on `server`. */
export function publish(server: Server<undefined> | undefined, message: ServerMessage): void {
	server?.publish(CLIENTS, JSON.stringify(message));
}

/**
 * The state feed's route (`/ws`): the bearer only, no browser page opens it (SEC-NET-4), and no other origin; its
 * refusals carry the page's headers (`guard`).
 */
export function feedRoute(token: string, guard: (response: Response, port: number) => Response): Route {
	return {
		matches: (path) => path === WS_PATH,
		handle: (request, _url, server: Server<undefined>) => {
			const at = server.port as number;
			if (!hasBearer(request, token)) return guard(new Response("Forbidden", { status: 403 }), at);
			const origin = request.headers.get("origin");
			if (origin !== null && origin !== `http://127.0.0.1:${server.port}`)
				return guard(new Response("Forbidden", { status: 403 }), at);
			return server.upgrade(request, { data: undefined })
				? undefined
				: guard(new Response("Upgrade failed", { status: 400 }), at);
		},
	};
}

/** The state feed: the service's snapshot first, then its messages; each client message goes to the service. */
export function feedSocket(service: Pick<JocastaService, "snapshot" | "handle">): WebSocketHandler<undefined> {
	return {
		open(ws) {
			ws.subscribe(CLIENTS);
			ws.send(JSON.stringify(service.snapshot() satisfies ServerMessage));
		},
		async message(ws, data: string | Buffer) {
			const reply = (message: ServerMessage) => ws.send(JSON.stringify(message));
			let message: ClientMessage;
			try {
				message = JSON.parse(String(data)) as ClientMessage;
			} catch {
				reply({ type: "error", message: "not JSON" });
				return;
			}
			await service.handle(message, reply);
		},
	};
}
