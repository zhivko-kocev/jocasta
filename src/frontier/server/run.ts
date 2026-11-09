// `jocasta service` (0549, 0552): the Jocasta service as a plain Bun process, started detached by bare `jocasta`. It
// takes the single-instance lock (taking over a hung holder, 0534), starts the server and the push-to-talk hook, and
// exits once Jocasta has closed.
import { join } from "node:path";
import { log } from "../../shared/log.ts";
import { dataDir } from "../../shared/paths.ts";
import { readFrontierFile } from "./frontier-file.ts";
import { takeInstance } from "./instance.ts";
import { serviceState } from "./launch.ts";
import { killTree } from "./proctree.ts";
import { startPushToTalk } from "./ptt.ts";
import { type FrontierServer, startServer } from "./server.ts";

/** Runs the service until Jocasta closes; resolves the exit code. */
export async function runService(): Promise<number> {
	const instance = await takeInstance({
		lockFile: join(dataDir(), "service.lock"),
		state: () => serviceState(readFrontierFile()),
		kill: killTree,
	});
	if (!instance) {
		log("info", "service", "another Jocasta service is running; this one leaves");
		return 0;
	}
	let frontier: FrontierServer;
	try {
		frontier = await startServer();
	} catch (error) {
		log("error", "service", `didn't start: ${error instanceof Error ? error.message : String(error)}`);
		instance.release();
		return 1;
	}
	let code = 0;
	// A start that fails after the server listened stops what it started (herdr's connection, whiteboard,
	// `frontier.json`, LC-10).
	frontier.ready.catch(() => {
		code = 1;
		void frontier.stop();
	});
	// Right Ctrl toggles omp's dictation in the boss pane (0556).
	const ptt = startPushToTalk(() => frontier.service.toggleDictation());
	// Asked to end by the system (logoff, a kill that lets it): Jocasta closes as one, unasked.
	for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
		process.on(signal, () => void frontier.shutdown({ ask: false }));
	}
	await frontier.stopped;
	await ptt?.stop();
	instance.release();
	return code;
}
