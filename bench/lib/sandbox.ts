import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { serviceHealth } from "../../src/frontier/server/launch.ts";
import { HOSTS_ENV, LAYER_ENV } from "../../src/shared/layer.ts";
import { defaultDataDir } from "../../src/shared/paths.ts";
import { readServiceFile } from "../../src/shared/service-file.ts";
import { BenchUsageError } from "./args.ts";

/**
 * The live benchmarks (H2–H6, H8) act on a running Jocasta: they kill its processes, start threads and plant facts.
 * They run only against a sandbox (the 0441 walkthrough's, or 0481's VM) with its own data folder, herdr session and
 * config folder (0502, 0576): `JOCASTA_DATA_DIR` set and not the OS default, `JOCASTA_HERDR_SESSION` naming a session
 * that isn't his own (`jocasta`, or `jarvis` before the cutover), and `JOCASTA_CODING_AGENT_DIR` set and not
 * `~/.jocasta/agent`: the config folder picks the projects, herdr's config and the Graphiti compose project
 * (`composeProject()` is `jocasta` for his), so a sandbox on it would recreate his Graphiti container with its token.
 */

const HIS_SESSIONS: Record<string, true> = { jocasta: true, jarvis: true };

/** The bench herdr session the competitors' panes run in (H5, H6, the task benchmark's plan-review turns). */
export const BENCH_PTY_SESSION = "bench-pty";

/** 0481's VM: a throwaway Windows 11 machine, the only one the scripts that run commands or uninstall touch. */
export const VM_MACHINE = "JOCASTA-CLEAN";

/** Refuses unless this machine is the VM (`what` names what would run). */
export function requireVm(machine: string, what: string): void {
	if (machine.toUpperCase() !== VM_MACHINE)
		throw new BenchUsageError(`refusing to ${what} on ${machine}: only on the VM ${VM_MACHINE}`);
}

/** Whether `session` is one of his own herdr sessions. */
export function isHisSession(session: string): boolean {
	return HIS_SESSIONS[session.toLowerCase()] === true;
}

/** Refuses his own herdr session for a bench flag or variable (`what` names it in the refusal). */
export function refuseHisSession(session: string, what: string): void {
	if (isHisSession(session)) throw new BenchUsageError(`${what} can't be ${session}`);
}

export interface Sandbox {
	dataDir: string;
	herdrSession: string;
	configDir: string;
}

/** A path as compared: resolved, a leading `~` as the home folder, case-insensitive on Windows. */
function pathKey(path: string, platform: NodeJS.Platform): string {
	const expanded = path === "~" ? homedir() : /^~[/\\]/.test(path) ? join(homedir(), path.slice(2)) : path;
	const resolved = resolve(expanded);
	return platform === "win32" ? resolved.toLowerCase() : resolved;
}

export function requireSandbox(
	env: Record<string, string | undefined> = process.env,
	platform: NodeJS.Platform = process.platform,
	hisDataDir: string = defaultDataDir(),
): Sandbox {
	const dataDir = env.JOCASTA_DATA_DIR;
	const herdrSession = env.JOCASTA_HERDR_SESSION;
	const configDir = env.JOCASTA_CODING_AGENT_DIR;
	if (!dataDir)
		throw new BenchUsageError(
			"set JOCASTA_DATA_DIR to the sandbox's data folder; this benchmark acts on a running Jocasta",
		);
	if (pathKey(dataDir, platform) === pathKey(hisDataDir, platform))
		throw new BenchUsageError(`set JOCASTA_DATA_DIR to the sandbox's data folder, not his own (${hisDataDir})`);
	if (!herdrSession || isHisSession(herdrSession))
		throw new BenchUsageError("set JOCASTA_HERDR_SESSION to a sandbox herdr session (not jocasta or jarvis)");
	if (!configDir || pathKey(configDir, platform) === pathKey(join(homedir(), ".jocasta", "agent"), platform))
		throw new BenchUsageError(
			"set JOCASTA_CODING_AGENT_DIR to the sandbox's config folder; with ~/.jocasta/agent the sandbox uses his projects, herdr config and Graphiti containers",
		);
	return { dataDir, herdrSession, configDir };
}

export interface ToolReply {
	text: string;
	isError?: boolean;
}

/** The running service `<data>/jocasta-service.json` names (`JOCASTA_DATA_DIR` picks the sandbox's). */
function service(): { port: number; token: string } {
	const running = readServiceFile();
	if (!running) throw new Error("no running service: <data>/jocasta-service.json is missing");
	return running;
}

/**
 * One call to the running service's MCP endpoint, as `jocasta mcp` makes it (`POST /api/mcp/<tool>`, bearer token from
 * `<data>/jocasta-service.json`, body `{args, paneKey?, bossKey?}`): a pane's tools name the pane by its key (0578), which
 * the service keeps in `<data>/panes.json`.
 */
export async function callTool(
	tool: string,
	args: Record<string, unknown>,
	caller: { paneKey?: string; bossKey?: string } = {},
): Promise<ToolReply> {
	const { port, token } = service();
	const response = await fetch(`http://127.0.0.1:${port}/api/mcp/${encodeURIComponent(tool)}`, {
		method: "POST",
		headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
		body: JSON.stringify({ args, ...caller }),
		timeout: false,
	});
	return (await response.json()) as ToolReply;
}

/** The two test-only switches a sandbox service reports in `/health` (0576), with the variable that turns each off. */
const SWITCHES = {
	layer: { env: LAYER_ENV, what: "the layer", off: "--layer off", on: "--layer on" },
	hosts: { env: HOSTS_ENV, what: "omp's host registry", off: "--condition herdr", on: "--condition host" },
} as const;

/**
 * Refuses a run whose flag doesn't match the sandbox service's switch as `/health` reports it (0576): H8's `--layer`
 * against `layer`, H2's `--condition` against `hosts`. The flag can't switch the service; the operator starts it with
 * the variable.
 */
export async function requireServiceSwitch(key: keyof typeof SWITCHES, wanted: "on" | "off"): Promise<void> {
	const health = await serviceHealth(readServiceFile());
	if (health.state !== "up") throw new BenchUsageError("no running sandbox service answers /health");
	const actual = health[key];
	const sw = SWITCHES[key];
	if (actual === undefined)
		throw new BenchUsageError(
			`the sandbox service's /health doesn't say whether ${sw.what} is on (a build before 0576)`,
		);
	if (actual !== wanted)
		throw new BenchUsageError(
			`${wanted === "off" ? sw.off : sw.on} needs ${sw.what} ${wanted}, but the sandbox service's /health says ${actual}; start the service with ${sw.env}=off for ${sw.off}, without it for ${sw.on}`,
		);
}
