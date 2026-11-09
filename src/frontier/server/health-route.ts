import { hostsOn, layerOn } from "../../shared/layer.ts";
import { herdrRevivals } from "./herdr.ts";
import { hasBearer, type PageRoute } from "./routes.ts";

/**
 * `GET /health` with the bearer: `closing` while Jocasta is closing, so a `jocasta` started meanwhile waits instead of
 * attaching to it (LC-3); `herdrRevivals`: a client that exited while it grew lost a crashed herdr server, not him
 * (0534); `layer` and `hosts`: the test-only switches as this service has them, which the benchmarks check (0576).
 */
export function healthRoute(token: string, closing: () => boolean): PageRoute {
	return {
		matches: (path) => path === "/health",
		handle: (request) => {
			if (!hasBearer(request, token)) return new Response("Forbidden", { status: 403 });
			return Response.json({
				pid: process.pid,
				herdrRevivals: herdrRevivals(),
				layer: layerOn() ? "on" : "off",
				hosts: hostsOn() ? "on" : "off",
				...(closing() ? { closing: true } : {}),
			});
		},
	};
}
