import { rmSync } from "node:fs";
import { join } from "node:path";
import { readJsonObject, writeAtomically } from "./fsx.ts";
import { dataDir } from "./paths.ts";
import { pidAlive } from "./proctree.ts";

/** `<data>/jocasta-service.json` (0254, 0584): where the running service listens, its token and its pid. */
export interface ServiceFile {
	port: number;
	token: string;
	pid: number;
}

/** The file's name before 0584; read when the new one isn't there (a service an older build started). */
const OLD_SERVICE_FILE = "frontier.json";

export function serviceFilePath(dir = dataDir()): string {
	return join(dir, "jocasta-service.json");
}

/**
 * Writes the file atomically, so a second `jocasta` never reads half of it, and private to this user (0600). `dir` is
 * the data folder (the benchmark's layer writes a trial's).
 */
export function writeServiceFile(file: ServiceFile, dir = dataDir()): void {
	writeAtomically(serviceFilePath(dir), `${JSON.stringify(file)}\n`, 0o600);
}

function readAt(path: string): ServiceFile | undefined {
	const object = readJsonObject(path);
	if (!object) return undefined;
	const { port, token, pid } = object;
	if (typeof port !== "number" || typeof token !== "string" || typeof pid !== "number") return undefined;
	return { port, token, pid };
}

/**
 * The file, else the old `frontier.json` (0584), or undefined when neither is one (a stale or torn file just means no
 * server).
 */
export function readServiceFile(): ServiceFile | undefined {
	return readAt(serviceFilePath()) ?? readAt(join(dataDir(), OLD_SERVICE_FILE));
}

/** Removes the file when it still names this server's process. */
export function removeServiceFile(pid: number): void {
	if (readAt(serviceFilePath())?.pid === pid) rmSync(serviceFilePath(), { force: true });
}

/** The service named by `<data>/jocasta-service.json` while its process lives: its port and token. */
export function runningService(): { port: number; token: string } | undefined {
	const file = readServiceFile();
	return file && pidAlive(file.pid) ? { port: file.port, token: file.token } : undefined;
}

/** The one text wherever Jocasta's service isn't running (0581 UX-13). */
export const NOT_RUNNING = "Jocasta isn't running; start it with jocasta.";

/** Whether a fetch to the service failed because nothing listens on its port (Bun names it `ConnectionRefused`). */
export function connectionRefused(error: unknown): boolean {
	const code = error !== null && typeof error === "object" && "code" in error ? error.code : undefined;
	return code === "ConnectionRefused" || code === "ECONNREFUSED";
}
