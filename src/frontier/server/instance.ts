// One Jocasta service at a time (0362, 0549): `<data>/service.lock` names the service that holds it by pid and start
// time, and the herdr server that service started, if any (0582). A start that finds it held by a live service that
// answers leaves; one that is closing is waited for; one that doesn't answer within a bound is hung: its process tree,
// all but herdr's server and what runs under it, is ended and the lock taken over (0534, 0582), so a hung service never
// blocks every later start, as a hung service did (0530), and the threads keep working.
import { closeSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync, writeSync } from "node:fs";
import { dirname } from "node:path";
import { asObject } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import { type ProcessId, processIdOf } from "../../shared/proctree.ts";
import type { ServiceState } from "./launch.ts";

/** How long a live holder may go without answering before it counts as hung (0534). */
export const HUNG_MS = 15_000;
/** How long a holder that says it is closing is waited for before it counts as hung. */
export const CLOSING_MS = 60_000;

export interface InstanceOptions {
	/** `<data>/service.lock`. */
	lockFile: string;
	/** What the service `jocasta-service.json` names is doing (`serviceState`). */
	state(): Promise<ServiceState>;
	/** This process (test seam): `process.pid` as it runs now. */
	self?: ProcessId;
	/** Ends a hung holder with every process under it but `spare` and what runs under it (`endProcesses`). */
	kill(holder: ProcessId, spare: ProcessId[]): Promise<unknown>;
	hungMs?: number;
	closingMs?: number;
	sleep?(ms: number): Promise<void>;
}

export interface Instance {
	/** Gives the lock up (the service has stopped); only while it still names this process. */
	release(): void;
	/** Records the herdr server this service started beside the lock (0582), while the lock names this process. */
	noteHerdr(server: ProcessId): void;
}

/** What the lock says: its holder and the herdr server that holder started. */
interface Holder extends ProcessId {
	herdr?: ProcessId;
}

function processId(value: unknown): ProcessId | undefined {
	const object = asObject(value);
	return typeof object?.pid === "number" && typeof object.start === "number"
		? { pid: object.pid, start: object.start }
		: undefined;
}

function readHolder(file: string): Holder | undefined | "unreadable" {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch {
		return undefined;
	}
	try {
		const value = asObject(JSON.parse(text));
		const holder = processId(value);
		const herdr = processId(value?.herdr);
		if (holder) return herdr ? { ...holder, herdr } : holder;
	} catch {
		// Being written, or torn.
	}
	return "unreadable";
}

/** The lock, created only when there is none (`wx`); false when another holds it. */
function create(file: string, self: ProcessId): boolean {
	mkdirSync(dirname(file), { recursive: true });
	let fd: number;
	try {
		fd = openSync(file, "wx", 0o600);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "EEXIST") return false;
		throw error;
	}
	try {
		writeSync(fd, `${JSON.stringify(self)}\n`);
	} finally {
		closeSync(fd);
	}
	return true;
}

/**
 * Takes the single-instance lock (0362, 0534): resolves the held lock, or undefined when another service holds it and
 * answers (this start then leaves). A dead holder's lock (crash; Windows reuses pids, so pid and start time both
 * count) is taken at once; a closing one is waited for up to `closingMs`; a live holder that doesn't answer for
 * `hungMs` is ended with its whole tree but the herdr server it recorded (0582), and its lock taken.
 */
export async function takeInstance(options: InstanceOptions): Promise<Instance | undefined> {
	const self = options.self ?? processIdOf(process.pid);
	if (!self) throw new Error("this process's start time can't be read");
	const sleep = options.sleep ?? Bun.sleep;
	const file = options.lockFile;
	let silentSince: number | undefined;
	let closingSince: number | undefined;
	const mine = () => {
		const holder = readHolder(file);
		return typeof holder === "object" && holder.pid === self.pid && holder.start === self.start;
	};
	const instance: Instance = {
		release: () => {
			if (mine()) rmSync(file, { force: true });
		},
		noteHerdr: (server) => {
			if (mine()) writeFileSync(file, `${JSON.stringify({ ...self, herdr: server })}\n`, { mode: 0o600 });
		},
	};
	for (;;) {
		if (create(file, self)) return instance;
		const holder = readHolder(file);
		if (holder === undefined) continue;
		if (holder !== "unreadable" && processIdOf(holder.pid)?.start !== holder.start) {
			rmSync(file, { force: true });
			continue;
		}
		const state = holder === "unreadable" ? "busy" : await options.state();
		if (state === "up") return undefined;
		const now = Date.now();
		if (state === "closing") {
			silentSince = undefined;
			closingSince ??= now;
		} else {
			closingSince = undefined;
			silentSince ??= now;
		}
		const hung =
			(closingSince !== undefined && now - closingSince >= (options.closingMs ?? CLOSING_MS)) ||
			(silentSince !== undefined && now - silentSince >= (options.hungMs ?? HUNG_MS));
		if (hung && holder !== "unreadable") {
			log(
				"warn",
				"instance",
				`the service ${holder.pid} holds the lock but doesn't answer; ending it (herdr's session keeps running) and starting`,
			);
			await options.kill({ pid: holder.pid, start: holder.start }, holder.herdr ? [holder.herdr] : []);
			rmSync(file, { force: true });
			silentSince = closingSince = undefined;
			continue;
		}
		if (hung) rmSync(file, { force: true });
		await sleep(250);
	}
}
