import { readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { asObject, writeAtomically } from "../../shared/fsx.ts";
import { dataDir } from "../../shared/paths.ts";

/** `<data>/frontier.json` (0254). */
export interface FrontierFile {
	port: number;
	token: string;
	pid: number;
}

export function frontierFilePath(): string {
	return join(dataDir(), "frontier.json");
}

/** Writes the file atomically, so a second `jocasta` never reads half of it. */
export function writeFrontierFile(file: FrontierFile): void {
	writeAtomically(frontierFilePath(), `${JSON.stringify(file)}\n`, 0o600);
}

/** The file, or undefined when it is missing or not one (a stale or torn file just means no server). */
export function readFrontierFile(): FrontierFile | undefined {
	let value: unknown;
	try {
		value = JSON.parse(readFileSync(frontierFilePath(), "utf8"));
	} catch {
		return undefined;
	}
	const object = asObject(value);
	if (!object) return undefined;
	const { port, token, pid } = object;
	if (typeof port !== "number" || typeof token !== "string" || typeof pid !== "number") return undefined;
	return { port, token, pid };
}

/** Removes the file when it still names this server's process. */
export function removeFrontierFile(pid: number): void {
	if (readFrontierFile()?.pid === pid) rmSync(frontierFilePath(), { force: true });
}
