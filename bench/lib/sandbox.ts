import { readFrontierFile } from "../../src/frontier/server/frontier-file.ts";
import { BenchUsageError } from "./args.ts";

/**
 * The live benchmarks (H2–H4, H8) act on a running Jocasta: they kill its processes, start threads and plant facts.
 * They run only against a sandbox (the 0441 walkthrough's, or 0481's VM): `JOCASTA_DATA_DIR` set and a herdr session
 * named in `JOCASTA_HERDR_SESSION` that isn't his own (`jocasta`, or `jarvis` before the cutover).
 */

const HIS_SESSIONS: Record<string, true> = { jocasta: true, jarvis: true };

export interface Sandbox {
	dataDir: string;
	herdrSession: string;
}

export function requireSandbox(env: Record<string, string | undefined> = process.env): Sandbox {
	const dataDir = env.JOCASTA_DATA_DIR;
	const herdrSession = env.JOCASTA_HERDR_SESSION;
	if (!dataDir)
		throw new BenchUsageError(
			"set JOCASTA_DATA_DIR to the sandbox's data folder; this benchmark acts on a running Jocasta",
		);
	if (!herdrSession || HIS_SESSIONS[herdrSession.toLowerCase()])
		throw new BenchUsageError("set JOCASTA_HERDR_SESSION to a sandbox herdr session (not jocasta or jarvis)");
	return { dataDir, herdrSession };
}

export interface ToolReply {
	text: string;
	isError?: boolean;
}

/**
 * One call to the running service's MCP endpoint, as `jocasta mcp` makes it (`POST /api/mcp/<tool>`, bearer token from
 * `<data>/frontier.json`, body `{args, paneId?, bossKey?}`). The boss's tools need the boss key (0484), which only the
 * boss pane's environment has (`JOCASTA_BOSS_KEY`).
 */
export async function callTool(
	tool: string,
	args: Record<string, unknown>,
	caller: { paneId?: string; bossKey?: string } = {},
): Promise<ToolReply> {
	const frontier = readFrontierFile();
	if (!frontier) throw new Error("no running companion: <data>/frontier.json is missing");
	const response = await fetch(`http://127.0.0.1:${frontier.port}/api/mcp/${encodeURIComponent(tool)}`, {
		method: "POST",
		headers: { authorization: `Bearer ${frontier.token}`, "content-type": "application/json" },
		body: JSON.stringify({ args, ...caller }),
	});
	return (await response.json()) as ToolReply;
}
