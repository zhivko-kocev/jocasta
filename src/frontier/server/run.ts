// `jocasta service` (0549, 0552): the Jocasta service as a plain Bun process, started detached by bare `jocasta`. It
// takes the single-instance lock (taking over a hung holder, 0534, sparing the herdr server it started, 0582), starts
// the server and the push-to-talk hook, and exits once Jocasta has closed.
import { join } from "node:path";
import { log } from "../../shared/log.ts";
import { dataDir } from "../../shared/paths.ts";
import { endProcesses } from "../../shared/proctree.ts";
import { readServiceFile } from "../../shared/service-file.ts";
import { takeInstance } from "./instance.ts";
import { serviceState } from "./launch.ts";
import { startPushToTalk } from "./ptt.ts";
import { type JocastaServer, startServer } from "./server.ts";

/** Runs the service until Jocasta closes; resolves the exit code. */
export async function runService(): Promise<number> {
	const instance = await takeInstance({
		lockFile: join(dataDir(), "service.lock"),
		state: () => serviceState(readServiceFile()),
		kill: (holder, spare) => endProcesses([holder], spare),
	});
	if (!instance) {
		log("info", "service", "another Jocasta service is running; this one leaves");
		return 0;
	}
	let server: JocastaServer;
	try {
		// The herdr server it starts is noted beside the lock: a takeover of this service, hung, spares it (0582).
		server = await startServer({ onHerdrServer: (herdr) => instance.noteHerdr(herdr) });
	} catch (error) {
		log("error", "service", `didn't start: ${error instanceof Error ? error.message : String(error)}`);
		instance.release();
		return 1;
	}
	let code = 0;
	// A start that fails after the server listened stops what it started (herdr's connection, whiteboard,
	// `jocasta-service.json`, LC-10).
	server.ready.catch(() => {
		code = 1;
		void server.stop();
	});
	// Right Ctrl toggles omp's dictation in the boss pane (0556).
	const ptt = startPushToTalk(() => server.service.toggleDictation());
	// Asked to end by the system (logoff, a kill that lets it): Jocasta closes as one, unasked.
	for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
		process.on(signal, () => void server.shutdown({ ask: false }));
	}
	await server.stopped;
	await ptt?.stop();
	instance.release();
	return code;
}
