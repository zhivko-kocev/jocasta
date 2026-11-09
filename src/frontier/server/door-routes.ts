import type { Server } from "bun";
import { CLI_PATH } from "../../shared/thread-cli.ts";
import { SHUTDOWN_PATH } from "./launch.ts";
import { objectArgs, type Route, refusedPost } from "./routes.ts";
import type { JocastaService } from "./service.ts";

/** Where `jocasta mcp` calls its tools (`src/mcp/server.ts`). */
export const MCP_PATH = "/api/mcp/";

/**
 * A door's call (`jocasta mcp`, `jocasta thread|worktree`): the API token as the bearer, POST, a JSON body; answered
 * `{text, isError?}`. A call may wait for his Allow up to 2 minutes (0484), a merge for its hooks, so it has no idle
 * timeout.
 */
function doorCall(
	token: string,
	answer: (body: Record<string, unknown>, url: URL) => Promise<Response>,
): Route["handle"] {
	return async (request, url, server: Server<undefined>) => {
		const refused = refusedPost(request, token, true);
		if (refused) return refused;
		server.timeout(request, 0);
		let body: Record<string, unknown>;
		try {
			body = (await request.json()) as Record<string, unknown>;
		} catch {
			return Response.json({ text: "body is not JSON", isError: true }, { status: 400 });
		}
		return answer(body, url);
	};
}

/** What the doors need. */
export interface DoorParts {
	token: string;
	service: Pick<JocastaService, "callTool" | "cliCall">;
	/** His client closed (`POST /api/shutdown`): the close, asking first. */
	clientClosed: () => void;
}

/** The doors into the service: `jocasta mcp`'s tools, `jocasta thread|worktree` and bare `jocasta`'s shutdown. */
export function doorRoutes({ token, service, clientClosed }: DoorParts): Route[] {
	return [
		{
			// `POST /api/mcp/<tool>`: body `{args, paneKey?, bossKey?}` (the pane's `JOCASTA_PANE_KEY`, the boss's key; 0578).
			matches: (path) => path.startsWith(MCP_PATH),
			handle: doorCall(token, async (body, url) => {
				const tool = decodeURIComponent(url.pathname.slice(MCP_PATH.length));
				return Response.json(
					await service.callTool(tool, objectArgs(body.args), {
						...(typeof body.paneKey === "string" ? { paneKey: body.paneKey } : {}),
						...(typeof body.bossKey === "string" ? { bossKey: body.bossKey } : {}),
					}),
				);
			}),
		},
		{
			// `POST /api/cli` (0544): body `{command, args}`.
			matches: (path) => path === CLI_PATH,
			handle: doorCall(token, async (body) => {
				if (typeof body.command !== "string") {
					return Response.json({ text: "command is required", isError: true }, { status: 400 });
				}
				return Response.json(await service.cliCall(body.command, objectArgs(body.args)));
			}),
		},
		{
			// `POST /api/shutdown` from bare `jocasta` (0434, 0549): his client closed. Answered at once, since it may come
			// from a closing terminal with seconds left.
			matches: (path) => path === SHUTDOWN_PATH,
			handle: (request) => {
				const refused = refusedPost(request, token, false);
				if (refused) return refused;
				// After the answer has gone out, as it may come from a closing terminal.
				setTimeout(clientClosed, 100);
				return new Response(null, { status: 202 });
			},
		},
	];
}
