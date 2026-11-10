/**
 * omp's local host registry (0547, 0561): every omp hosting a collab room (Jocasta's panes, through its relay) has
 * `<home>/.omp/run/collab-hosts/<entry>.json` (`{version, instanceId, pid, endpoint, createdAt, token}`) and answers
 * one JSON line on that endpoint, a named pipe on Windows: `{"v":1,"token":…,"op":"snapshot"}` →
 * `{ok, v, snapshot: {pid, cwd, busy, inputRequired, relayConnected, …}}` (omp 18.18.6 `src/collab/registry.ts`,
 * protocol v1, documented for dashboards). Read directly: `omp collab list` takes about 0.7 s a call, a pipe 0–5 ms.
 */
import { readdirSync, readFileSync, rmSync } from "node:fs";
import { connect } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { asObject } from "../../shared/fsx.ts";
import { pidAlive } from "../../shared/proctree.ts";

/** The registry protocol Jocasta speaks (`COLLAB_REGISTRY_VERSION`). */
const REGISTRY_VERSION = 1;

/** How long one host may take to answer before it counts as not answering. */
const QUERY_TIMEOUT_MS = 1_000;

/** Answers beyond this are no snapshot (omp's own bound is 64 KiB). */
const MAX_ANSWER = 64 * 1024;

/** `~/.omp/run/collab-hosts` (omp's `collabHostsRuntimeDir`: `PI_CONFIG_DIR` renames `.omp`; no profile in it). */
export function hostsDir(): string {
	return join(homedir(), process.env.PI_CONFIG_DIR || ".omp", "run", "collab-hosts");
}

/** One host's state as omp reports it. */
export interface HostState {
	pid: number;
	cwd: string;
	/** A turn running; null when the host doesn't say (an older omp): unknown, never idle. */
	busy: boolean | null;
	/** A dialog of the host's (a question, an approval) waiting for an answer. */
	inputRequired: boolean;
	relayConnected: boolean;
}

interface Entry {
	pid: number;
	endpoint: string;
	token: string;
}

function entryOf(text: string): Entry | undefined {
	let raw: Record<string, unknown> | undefined;
	try {
		raw = asObject(JSON.parse(text));
	} catch {
		return undefined;
	}
	if (!raw || raw.version !== REGISTRY_VERSION) return undefined;
	const { pid, endpoint, token } = raw;
	if (typeof pid !== "number" || typeof endpoint !== "string" || typeof token !== "string") return undefined;
	return { pid, endpoint, token };
}

function stateOf(answer: string): HostState | undefined {
	let raw: Record<string, unknown> | undefined;
	try {
		raw = asObject(JSON.parse(answer));
	} catch {
		return undefined;
	}
	const snapshot = raw?.ok === true ? asObject(raw.snapshot) : undefined;
	if (!snapshot) return undefined;
	const { pid, cwd, busy, inputRequired, relayConnected } = snapshot;
	if (typeof pid !== "number" || typeof cwd !== "string") return undefined;
	return {
		pid,
		cwd,
		busy: typeof busy === "boolean" ? busy : null,
		inputRequired: inputRequired === true,
		relayConnected: relayConnected === true,
	};
}

/** One host's snapshot, or undefined when it doesn't answer one in time. */
function query(entry: Entry, timeoutMs: number): Promise<HostState | undefined> {
	const { promise, resolve } = Promise.withResolvers<HostState | undefined>();
	const socket = connect(entry.endpoint);
	let answer = "";
	const done = (state: HostState | undefined) => {
		clearTimeout(timer);
		socket.destroy();
		resolve(state);
	};
	const timer = setTimeout(() => done(undefined), timeoutMs);
	socket.setEncoding("utf8");
	socket.on("connect", () => {
		socket.write(`${JSON.stringify({ v: REGISTRY_VERSION, token: entry.token, op: "snapshot" })}\n`);
	});
	socket.on("data", (chunk: string) => {
		answer += chunk;
		const end = answer.indexOf("\n");
		if (end >= 0) done(stateOf(answer.slice(0, end)));
		else if (answer.length > MAX_ANSWER) done(undefined);
	});
	socket.on("end", () => done(stateOf(answer)));
	socket.on("error", () => done(undefined));
	return promise;
}

/**
 * The hosts in `dir` whose process is one of `pids`, each as it answers now; one that doesn't answer is left out.
 * Other omps (his own, outside Jocasta) are never asked. omp's own listing prunes stale entries; this never deletes.
 */
export async function readHosts(
	pids: ReadonlySet<number>,
	dir = hostsDir(),
	timeoutMs = QUERY_TIMEOUT_MS,
): Promise<HostState[]> {
	if (pids.size === 0) return [];
	let names: string[];
	try {
		names = readdirSync(dir).filter((name) => name.endsWith(".json"));
	} catch {
		return [];
	}
	const entries: Entry[] = [];
	for (const name of names) {
		let text: string;
		try {
			text = readFileSync(join(dir, name), "utf8");
		} catch {
			continue;
		}
		const entry = entryOf(text);
		if (entry && pids.has(entry.pid)) entries.push(entry);
	}
	const states = await Promise.all(entries.map((entry) => query(entry, timeoutMs)));
	return states.filter((state): state is HostState => state !== undefined && pids.has(state.pid));
}

/**
 * Removes the entries in `dir` whose process is one of `pids` and no longer runs (0547): Stop everything's omps, which
 * omp's own listing would prune only later. An entry whose pid runs, or that names another pid, stays. Returns how
 * many went.
 */
export function removeHostEntries(pids: ReadonlySet<number>, dir = hostsDir()): number {
	let names: string[];
	try {
		names = readdirSync(dir).filter((name) => name.endsWith(".json"));
	} catch {
		return 0;
	}
	let removed = 0;
	for (const name of names) {
		const file = join(dir, name);
		let entry: Entry | undefined;
		try {
			entry = entryOf(readFileSync(file, "utf8"));
		} catch {
			continue;
		}
		if (!entry || !pids.has(entry.pid) || pidAlive(entry.pid)) continue;
		try {
			rmSync(file, { force: true });
			removed++;
		} catch {
			// Gone already, or held: omp's own listing prunes it later.
		}
	}
	return removed;
}
