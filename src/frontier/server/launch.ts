import { pidAlive } from "../../shared/fsx.ts";
import { TOKEN_COOKIE } from "../protocol.ts";
import type { FrontierFile } from "./frontier-file.ts";

/**
 * What the service named by `frontier.json` is doing (0254, 0434): `up` answers as that process; `closing` answers but
 * is closing Jocasta; `busy` doesn't answer in time while its process lives (still starting, or hung, 0534); `down`
 * otherwise (no file, nothing listening, another process answering).
 */
export type ServiceState = "up" | "busy" | "closing" | "down";

/** `/health`'s answer as bare `jocasta` reads it. */
export interface ServiceHealth {
	state: ServiceState;
	/** How many times the service started a stopped herdr server again (0431), while it answers (0534). */
	herdrRevivals?: number;
}

/** Asks the service's `/health` (its token, 2 s) what it is doing (`ServiceState`). */
export async function serviceHealth(file: FrontierFile | undefined): Promise<ServiceHealth> {
	if (!file) return { state: "down" };
	try {
		const response = await fetch(`http://127.0.0.1:${file.port}/health`, {
			headers: { cookie: `${TOKEN_COOKIE}=${file.token}` },
			signal: AbortSignal.timeout(2_000),
		});
		if (response.status !== 200) return { state: "down" };
		const body: unknown = await response.json();
		if (body === null || typeof body !== "object" || !("pid" in body) || body.pid !== file.pid)
			return { state: "down" };
		const revivals = "herdrRevivals" in body && typeof body.herdrRevivals === "number" ? body.herdrRevivals : undefined;
		return {
			state: "closing" in body && body.closing === true ? "closing" : "up",
			...(revivals === undefined ? {} : { herdrRevivals: revivals }),
		};
	} catch (error) {
		return { state: error instanceof Error && error.name === "TimeoutError" && pidAlive(file.pid) ? "busy" : "down" };
	}
}

/** `serviceHealth`'s state only. */
export async function serviceState(file: FrontierFile | undefined): Promise<ServiceState> {
	return (await serviceHealth(file)).state;
}

/**
 * Where bare `jocasta` tells the service its herdr client closed (0434, 0549): `POST` with the token as a bearer. The
 * service stops everything, or, with threads at work, asks first in a terminal it opens again.
 */
export const SHUTDOWN_PATH = "/api/shutdown";

/** Tells the service named by `frontier.json` that his client closed; true when it took it. */
export async function askShutdown(file: FrontierFile): Promise<boolean> {
	try {
		const response = await fetch(`http://127.0.0.1:${file.port}${SHUTDOWN_PATH}`, {
			method: "POST",
			headers: { authorization: `Bearer ${file.token}` },
			signal: AbortSignal.timeout(2_000),
		});
		return response.ok;
	} catch {
		return false;
	}
}
