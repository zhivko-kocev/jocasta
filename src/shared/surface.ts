/** Bare `jocasta`'s work surface (0387, 0434, 0549): starting the service, attaching herdr's client and closing. */
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { ServiceHealth, ServiceState } from "../frontier/server/launch.ts";
import { UserError } from "./errors.ts";
import { herdrSession } from "./herdr-session.ts";
import { LOG_FILE } from "./log.ts";
import { pidAlive, processStartedAt } from "./proctree.ts";
import type { ServiceFile } from "./service-file.ts";

export interface SurfaceDeps {
	/**
	 * What the service `<data>/jocasta-service.json` names is doing (`serviceHealth`): its `/health` with the file's token and
	 * pid, not merely whether that pid is alive, since a stale file's pid may belong to another program (CR-15).
	 */
	serviceHealth(): Promise<ServiceHealth>;
	/**
	 * Starts the service (`jocasta service`) as a plain Bun process, detached from this terminal (0549, 0552). It takes
	 * the single-instance lock itself: one that finds another service answering leaves, and one that finds a hung one
	 * ends it and starts (0534). `exited` resolves its exit code when it ends.
	 */
	startService(): { exited: Promise<number> };
	/** The service's last ERROR line in Jocasta's log since `since` (epoch ms), its message only (`lastLoggedError`). */
	lastError(since: number): string | undefined;
	/** The herdr binary, or undefined. */
	herdr: string | undefined;
	/** `~/.jocasta/agent/herdr.toml`. */
	herdrConfig: string;
	/** Runs herdr's client in this terminal; resolves its exit code. */
	attach(argv: string[], env: Record<string, string>): Promise<number>;
	print(line: string): void;
	/**
	 * Waits this long for a service it started to answer before attaching. The service answers at once and makes the
	 * boss pane while it starts, so the pane may appear shortly after attaching.
	 */
	waitMs?: number;
	sleep?(ms: number): Promise<void>;
	/** Marks this `jocasta` as attached to the session (`attachedSurface`); `leave` says whether it was the last. */
	enterSurface(): { leave(): boolean };
	/**
	 * Calls `handler` when this terminal closes (the Windows console close event, which Bun gives as SIGHUP, with a few
	 * seconds left); returns the function that stops listening.
	 */
	onTerminalClose(handler: () => void): () => void;
	/**
	 * Tells the running service his client closed (`POST /api/shutdown`, 0549): it stops everything, or asks first
	 * while threads are at work. True when it took it, false when no service answers.
	 */
	tellServiceClosed(): Promise<boolean>;
	/** No service: stops herdr session `jocasta`'s server (only that session), never starting it. */
	stopHerdr(): Promise<void>;
	/** No service: stops Graphiti and FalkorDB without waiting (data kept). */
	stopContainers(): void;
	/** Ends this process once Jocasta was told to close after its terminal closed. */
	exit(): void;
	/**
	 * Notes which service `jocasta-service.json` names as herdr's client starts; the returned check says whether that service
	 * has since stopped herdr itself (`serviceStopped`, 0491).
	 */
	watchServiceStop(): () => Promise<boolean>;
	/** Whether herdr's server for the session answers a ping; never starts it (0518). */
	herdrUp(): Promise<boolean>;
}

/** How long bare `jocasta` waits for the service to start a dead herdr server again before closing (0518). */
const HERDR_BACK_MS = 30_000;
/** How long bare `jocasta` waits for a service it started: a hung one it replaces takes `HUNG_MS` first (0534). */
const SERVICE_START_MS = 90_000;
/** A started service is asked every 50 ms for its first 3 s, then every 250 ms (PERF-7). */
const FAST_POLL_MS = 3_000;

/** A line of Jocasta's log (log.ts): `<ISO time> <LEVEL> [<scope>] <message>`. */
const LOG_LINE = /^(\S+) ERROR \[[^\]]*\] (.*)$/;

/**
 * The message of the last ERROR line in Jocasta's log folder `dir` written at or after `since` (epoch ms), from the
 * newest two day files; undefined when there is none. The service's "didn't start: " lead is left out (SHR-5).
 */
export function lastLoggedError(dir: string, since: number): string | undefined {
	let names: string[];
	try {
		names = readdirSync(dir).filter((name) => LOG_FILE.test(name));
	} catch {
		return undefined;
	}
	let last: string | undefined;
	for (const name of names.sort().slice(-2)) {
		for (const line of readFileSync(join(dir, name), "utf8").split(/\r?\n/)) {
			const found = LOG_LINE.exec(line);
			if (found?.[1] && found[2] !== undefined && Date.parse(found[1]) >= since - 1_000) last = found[2];
		}
	}
	return last?.replace(/^didn't start: /, "");
}

/**
 * `<dir>/<pid>`: one file per bare `jocasta` attached to herdr's session `jocasta` (0434). Leaving removes this one's and
 * answers whether no other live `jocasta` is still attached, so a client that exits while another `jocasta` holds the
 * session (a second terminal, a re-attach) never closes Jocasta. A file counts only while its process lives and
 * started before the file was written (LC-4: Windows reuses pids); the others are removed.
 */
export function attachedSurface(
	dir: string,
	pid = process.pid,
	alive = pidAlive,
	startedAt = processStartedAt,
): { leave(): boolean } {
	mkdirSync(dir, { recursive: true });
	const own = join(dir, String(pid));
	writeFileSync(own, "");
	return {
		leave() {
			rmSync(own, { force: true });
			let others = false;
			for (const name of readdirSync(dir)) {
				const other = Number(name);
				if (!Number.isInteger(other) || other === pid) continue;
				const file = join(dir, name);
				if (alive(other)) {
					const started = startedAt(other);
					const written = statSync(file, { throwIfNoEntry: false })?.mtimeMs ?? 0;
					if (started === undefined || started <= written + 1_000) {
						others = true;
						continue;
					}
				}
				rmSync(file, { force: true });
			}
			return !others;
		},
	};
}

/**
 * Whether the service `before` named (as herdr's client started) stopped herdr's server itself (0491): it is closing
 * Jocasta (`/health`'s `closing`, set before its Stop everything stops herdr), or its close already ended by removing
 * `jocasta-service.json` (`now` no longer names it). A service that died leaves its file and answers nothing: not a stop.
 */
export async function serviceStopped(
	before: ServiceFile | undefined,
	now: ServiceFile | undefined,
	state: (file: ServiceFile) => Promise<ServiceState>,
): Promise<boolean> {
	if (!before) return false;
	if (now?.pid !== before.pid || now.token !== before.token) return true;
	return (await state(now)) === "closing";
}

/**
 * Bare `jocasta` (0387, 0392, 0404, 0434, 0549, 0552): starts the service when none answers and waits for it (it makes
 * workspace "Jocasta" with its boss pane first), then attaches herdr's client to session `jocasta` in this terminal
 * with Jocasta's herdr config. A service that is closing is waited for first. When the client exits because herdr's
 * server crashed (it stopped and the service started it again, or starts it within 30 s: 0518, 0534), the client is
 * attached again. Otherwise, when the client exits (quit or detach) or the terminal closes, and no other `jocasta` is
 * attached, the service is told: it stops everything, or with threads at work opens a terminal on the session again
 * and asks first (0549). With no service, the containers and herdr session `jocasta` stop here. Resolves herdr's exit
 * code, or 0 when the client exited because the service stopped herdr itself (Stop everything; 0491).
 */
export async function openSurface(deps: SurfaceDeps): Promise<number> {
	if (!deps.herdr) throw new UserError("herdr isn't installed: install it from https://herdr.dev", 1);
	const session = herdrSession();
	const sleep = deps.sleep ?? Bun.sleep;
	let state = (await deps.serviceHealth()).state;
	if (state === "closing") {
		deps.print("jocasta: Jocasta is closing; it starts again once it has.");
		const end = Date.now() + 60_000;
		while (state === "closing") {
			if (Date.now() >= end) {
				throw new UserError("Jocasta is still closing after a minute; run jocasta again in a moment.", 1);
			}
			await sleep(250);
			state = (await deps.serviceHealth()).state;
		}
	}
	if (state !== "up") {
		// Down, or not answering (hung, 0534): a new service takes over or leaves on its own.
		const started = Date.now();
		let exited: number | undefined;
		void deps.startService().exited.then((code) => {
			exited = code;
		});
		const end = started + (deps.waitMs ?? SERVICE_START_MS);
		let up = false;
		while (!up && Date.now() < end) {
			await sleep(Date.now() - started < FAST_POLL_MS ? 50 : 250);
			up = (await deps.serviceHealth()).state === "up";
			// It ended before answering: unless another service answers now (it left for that one), it failed to start,
			// and attaching would show a work surface without Jocasta (0582).
			if (!up && exited !== undefined) {
				up = (await deps.serviceHealth()).state === "up";
				if (up) break;
				const why = deps.lastError(started);
				deps.print(`jocasta: the service stopped while starting${why ? `: ${why}` : ` (exit code ${exited})`}`);
				deps.print("jocasta: run `jocasta diagnose` for the full log.");
				return 1;
			}
		}
		if (!up) deps.print("jocasta: the service is still starting; the boss's workspace appears when it is up.");
	}
	const env: Record<string, string> = {};
	if (existsSync(deps.herdrConfig)) env.HERDR_CONFIG_PATH = deps.herdrConfig;
	else deps.print(`jocasta: ${deps.herdrConfig} is missing; run \`jocasta setup\` for Jocasta's herdr look.`);
	const surface = deps.enterSurface();
	let closing: Promise<void> | undefined;
	const close = (trigger: "client" | "terminal") => {
		closing ??= (async () => {
			if (!surface.leave()) return;
			if (await deps.tellServiceClosed()) {
				if (trigger === "client") {
					deps.print("jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.");
				}
				return;
			}
			// The containers' stop goes out first: a closing console leaves seconds (LC-7).
			deps.stopContainers();
			try {
				await deps.stopHerdr();
			} catch (error) {
				deps.print(`jocasta: herdr session ${session} didn't stop: ${(error as Error).message}`);
			}
		})();
		return closing;
	};
	/**
	 * After the client exited: whether herdr's server crashed under it and is back (0518, 0534). Its server running
	 * means he quit the client, unless the service started a stopped server again since the client attached
	 * (`revivals` grew): then the crash and the restart both came before this look. A service that doesn't answer, or
	 * is closing, starts nothing; and herdr not back within 30 s is a close as before. The ping never starts herdr
	 * itself: the service does (0431).
	 */
	const herdrBack = async (revivals: number | undefined): Promise<boolean> => {
		if (await deps.herdrUp()) {
			const now = (await deps.serviceHealth()).herdrRevivals;
			return revivals !== undefined && now !== undefined && now > revivals;
		}
		const answers = async () => {
			const now = (await deps.serviceHealth()).state;
			return now === "up" || now === "busy";
		};
		if (!(await answers())) return false;
		deps.print(`jocasta: herdr session ${session} stopped; waiting for the service to start it again.`);
		for (let waited = 0; waited < HERDR_BACK_MS; waited += 250) {
			await sleep(250);
			// The terminal closed meanwhile: Jocasta is closing, nothing to attach to.
			if (closing) return false;
			if (await deps.herdrUp()) return true;
			if (!(await answers())) return false;
		}
		deps.print(`jocasta: herdr session ${session} isn't back after 30 s.`);
		return false;
	};
	// The terminal closing ends herdr's client too; whichever comes first closes Jocasta, once.
	const unlisten = deps.onTerminalClose(() => void close("terminal").finally(() => deps.exit()));
	try {
		for (;;) {
			const serviceStop = deps.watchServiceStop();
			const revivals = (await deps.serviceHealth()).herdrRevivals;
			const code = await deps.attach([deps.herdr, "--session", session], env);
			// Checked before closing: the service's close is still under way, or has just removed its file. Its own
			// stop (Stop everything) already ended herdr and the containers: nothing to close or say (0491).
			if (code !== 0 && (await serviceStop())) return 0;
			// A crashed server isn't a close: once the service has started herdr again, its client is attached again
			// in this terminal, as the first time (0518, 0534).
			if (await herdrBack(revivals)) continue;
			await close("client");
			return code;
		}
	} finally {
		unlisten();
		// A client that couldn't start closes nothing, but this `jocasta` is no longer attached.
		if (!closing) surface.leave();
	}
}
