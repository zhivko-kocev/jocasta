import { connectHerdr, type Herdr } from "../../src/frontier/server/herdr.ts";
import { type ServiceState, serviceState } from "../../src/frontier/server/launch.ts";
import { readServiceFile, type ServiceFile } from "../../src/shared/service-file.ts";
import { type JocastaRoots, type ProcessInfo, surfacePids } from "./proctree.ts";
import { requireSandbox, type Sandbox } from "./sandbox.ts";

/**
 * The sandbox Jocasta as the live benchmarks read it, through the product's own helpers (BEN-18): herdr's panes from
 * the service's client, the service's state from `/health` as bare `jocasta` reads it, and the roots of Jocasta's
 * process trees.
 */
export interface SandboxJocasta {
	sandbox: Sandbox;
	/**
	 * The product's client for the sandbox herdr session, connected on the first call that finds its server answering
	 * and kept from then on. It never starts a server (`start: false`, so no request revives one either): the service
	 * does, and H3's herdr fault depends on nobody else doing it. Throws while no client is connected and the server
	 * doesn't answer; a connected client's requests throw while the server is down and work again once it is back.
	 */
	herdr(): Promise<Herdr>;
	/** The service's state (`serviceState`), the one `file` names (by default the one `<data>/jocasta-service.json` names). */
	health(file?: ServiceFile): Promise<ServiceState>;
	/**
	 * The roots of Jocasta's process trees in `processes` (0552): the service `jocasta-service.json` names, herdr's server
	 * (`herdrServerOf` the given pane roots, else of each pane's process as herdr lists them) and the attached surfaces.
	 */
	processes(processes: readonly ProcessInfo[], paneRoots?: Iterable<number>): Promise<JocastaRoots>;
}

/** How long one `pane.process_info` read may take: what the bench's own reads waited before BEN-18. */
const PANE_ROOT_MS = 10_000;

/**
 * herdr's server among `processes`: the parent of the first of `paneRoots` (the processes herdr started for its panes,
 * `Herdr.paneRoot`) that is running; undefined when none is.
 */
export function herdrServerOf(paneRoots: Iterable<number>, processes: readonly ProcessInfo[]): number | undefined {
	for (const root of paneRoots) {
		const server = processes.find((info) => info.pid === root)?.ppid;
		if (server) return server;
	}
	return undefined;
}

/**
 * The sandbox (`requireSandbox`, which refuses anything else) and its Jocasta; `herdrTimeoutMs` bounds each request of
 * its herdr client.
 */
export function openSandbox(options: { herdrTimeoutMs?: number } = {}): SandboxJocasta {
	const sandbox = requireSandbox();
	let client: Promise<Herdr> | undefined;
	const herdr = (): Promise<Herdr> => {
		if (!client) {
			client = connectHerdr({
				session: sandbox.herdrSession,
				start: false,
				timeoutMs: options.herdrTimeoutMs ?? PANE_ROOT_MS,
			});
			// A failed connect is tried again on the next call, once the server may answer.
			client.catch(() => {
				client = undefined;
			});
		}
		return client;
	};
	const herdrServer = async (processes: readonly ProcessInfo[]): Promise<number | undefined> => {
		const herdrClient = await herdr();
		// One pane at a time, stopping at the first whose process is running: the lookup reads no more panes than it needs.
		for (const pane of await herdrClient.panes()) {
			const root = await herdrClient.paneRoot(pane.paneId, PANE_ROOT_MS).catch(() => undefined);
			const server = root === undefined ? undefined : herdrServerOf([root], processes);
			if (server) return server;
		}
		return undefined;
	};
	return {
		sandbox,
		herdr,
		health: (file = readServiceFile()) => serviceState(file),
		async processes(processes, paneRoots) {
			return {
				service: readServiceFile()?.pid,
				herdrServer: paneRoots ? herdrServerOf(paneRoots, processes) : await herdrServer(processes),
				surfaces: surfacePids(sandbox.dataDir),
			};
		},
	};
}
