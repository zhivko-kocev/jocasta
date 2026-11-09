import { pidAlive } from "../../shared/proctree.ts";
import type { ServiceFile } from "../../shared/service-file.ts";

/**
 * What the service named by `jocasta-service.json` is doing (0254, 0434): `up` answers as that process; `closing` answers but
 * is closing Jocasta; `busy` doesn't answer in time while its process lives (still starting, or hung, 0534); `down`
 * otherwise (no file, nothing listening, another process answering).
 */
export type ServiceState = "up" | "busy" | "closing" | "down";

/** `/health`'s answer as bare `jocasta` and the benchmarks read it. */
export interface ServiceHealth {
	state: ServiceState;
	/** How many times the service started a stopped herdr server again (0431), while it answers (0534). */
	herdrRevivals?: number;
	/** The test-only switches as the service has them (0576), while it answers: `JOCASTA_LAYER`, `JOCASTA_HOSTS`. */
	layer?: "on" | "off";
	hosts?: "on" | "off";
}

/** "on" or "off" as `/health` says it, else undefined. */
function onOff(value: unknown): "on" | "off" | undefined {
	return value === "on" || value === "off" ? value : undefined;
}

/** Asks the service's `/health` (its token as a bearer, within `timeoutMs`) what it is doing (`ServiceState`). */
export async function serviceHealth(file: ServiceFile | undefined, timeoutMs = 2_000): Promise<ServiceHealth> {
	if (!file) return { state: "down" };
	try {
		const response = await fetch(`http://127.0.0.1:${file.port}/health`, {
			headers: { authorization: `Bearer ${file.token}` },
			signal: AbortSignal.timeout(timeoutMs),
		});
		if (response.status !== 200) return { state: "down" };
		const body: unknown = await response.json();
		if (body === null || typeof body !== "object" || !("pid" in body) || body.pid !== file.pid)
			return { state: "down" };
		const revivals = "herdrRevivals" in body && typeof body.herdrRevivals === "number" ? body.herdrRevivals : undefined;
		const layer = "layer" in body ? onOff(body.layer) : undefined;
		const hosts = "hosts" in body ? onOff(body.hosts) : undefined;
		return {
			state: "closing" in body && body.closing === true ? "closing" : "up",
			...(revivals === undefined ? {} : { herdrRevivals: revivals }),
			...(layer ? { layer } : {}),
			...(hosts ? { hosts } : {}),
		};
	} catch (error) {
		return { state: error instanceof Error && error.name === "TimeoutError" && pidAlive(file.pid) ? "busy" : "down" };
	}
}

/** `serviceHealth`'s state only. */
export async function serviceState(file: ServiceFile | undefined, timeoutMs = 2_000): Promise<ServiceState> {
	return (await serviceHealth(file, timeoutMs)).state;
}

/**
 * Where bare `jocasta` tells the service its herdr client closed (0434, 0549): `POST` with the token as a bearer. The
 * service stops everything, or, with threads at work, asks first in a terminal it opens again.
 */
export const SHUTDOWN_PATH = "/api/shutdown";

/** Tells the service named by `jocasta-service.json` that his client closed; true when it took it. */
export async function askShutdown(file: ServiceFile): Promise<boolean> {
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
