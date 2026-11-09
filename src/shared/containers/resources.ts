import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import dockerfile from "../../../docker/graphiti/Dockerfile" with { type: "text" };
import requirements from "../../../docker/graphiti/requirements.txt" with { type: "text" };
import servicePy from "../../../docker/graphiti/service.py" with { type: "text" };
import { freePort } from "../fsx.ts";
import { Containers, composeFile, composeProject, type Resource, stopDetached } from "./index.ts";

/** His Graphiti's port on 127.0.0.1 (compose project `jocasta`). */
export const GRAPHITI_PORT = 47380;

/** The compose variable carrying Graphiti's per-run token; it is never written to `compose.yaml` (0430). */
export const GRAPHITI_TOKEN_ENV = "JOCASTA_GRAPHITI_TOKEN";

let token: string | undefined;

/** This process's Graphiti token (0430): random per run, passed to the container with the compose environment. */
export function graphitiToken(): string {
	token ??= randomBytes(32).toString("hex");
	return token;
}

let port: number | undefined;

/**
 * Graphiti's port on 127.0.0.1: 47380 for his own compose project `jocasta`; any other (a test's or a smoke's own config
 * dir) gets a free port for the run, so it never takes his (F-04).
 */
export function graphitiPort(): number {
	if (composeProject() === "jocasta") return GRAPHITI_PORT;
	port ??= freePort();
	return port;
}

/**
 * Graphiti (5.9, 0334): FalkorDB (official image, pinned by digest, 0074) and Jocasta's Graphiti service, built from
 * `docker/graphiti/` on `python:3.12-slim` with `graphiti-core[falkordb,sentence-transformers]==0.30.2` and every
 * dependency hash-pinned (0430). FalkorDB is reachable only from the Graphiti container; the Graphiti service takes
 * the run's bearer token from `JOCASTA_GRAPHITI_TOKEN` in the compose environment. A process's first start recreates the
 * Graphiti container when it holds another run's token (Compose recreates a service whose environment changed), and a
 * 401 later (another process recreated it with its token) brings it onto this run's token again; FalkorDB and its
 * volume stay as they are.
 */
export function graphitiResource(): Resource {
	return {
		name: "graphiti",
		idleMs: 15 * 60_000,
		stale: (error) => error instanceof Error && "status" in error && error.status === 401,
		services: [
			{
				name: "falkordb",
				image: "falkordb/falkordb:6.0.2@sha256:107455e862a824e45e8e9f5a7fbfdf5e00a08b14e13eae2577c72fd9e3f59ba6",
				environment: { BROWSER: "0" },
				volumes: ["falkordb-data:/var/lib/falkordb/data"],
				healthcheck: {
					test: ["CMD", "redis-cli", "ping"],
					interval: "5s",
					timeout: "3s",
					retries: 10,
					startPeriod: "10s",
				},
			},
			{
				name: "graphiti",
				build: { Dockerfile: dockerfile, "requirements.txt": requirements, "service.py": servicePy },
				// `python /app/service.py` is the CMD; as PID 1 it ignores SIGTERM and the idle stop ended in a kill (F6).
				init: true,
				environment: {
					GRAPHITI_TELEMETRY_ENABLED: "false",
					FALKORDB_HOST: "falkordb",
					FALKORDB_PORT: "6379",
					// Compose fills it in from the environment of the command; empty refuses every request (0430).
					GRAPHITI_TOKEN: `\${${GRAPHITI_TOKEN_ENV}:-}`,
				},
				ports: [`127.0.0.1:${graphitiPort()}:8000`],
				dependsOn: ["falkordb"],
				healthcheck: {
					test: [
						"CMD",
						"python",
						"-c",
						"import urllib.request; urllib.request.urlopen('http://127.0.0.1:8000/health')",
					],
					interval: "5s",
					timeout: "5s",
					retries: 12,
					startPeriod: "120s",
				},
			},
		],
	};
}

let shared: Containers | undefined;

/** The process's container manager; at exit it stops what this process left running and no other process uses. */
export function jocastaContainers(): Containers {
	shared ??= new Containers([graphitiResource()], {
		onExit: stopDetached,
		env: () => ({ [GRAPHITI_TOKEN_ENV]: graphitiToken() }),
	});
	return shared;
}

/** Graphiti and FalkorDB stop now, their data kept (0434): the service's Stop everything. */
export function stopJocastaContainers(): Promise<void> {
	return jocastaContainers().halt("graphiti");
}

/**
 * The same from bare `jocasta` when no service runs (0434), without waiting: a detached `docker compose stop` that
 * outlives a closing terminal. Nothing when `compose.yaml` was never written.
 */
export function stopJocastaContainersDetached(): void {
	const file = composeFile();
	if (existsSync(file))
		stopDetached(
			file,
			graphitiResource().services.map((s) => s.name),
		);
}
