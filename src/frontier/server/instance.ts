// One Jocasta service at a time (0362, 0549): `<data>/service.lock` names the service that holds it by pid and start
// time. A start that finds it held by a live service that answers leaves; one that is closing is waited for; one that
// doesn't answer within a bound is hung: its process tree is ended and the lock taken over (0534), so a hung service
// never blocks every later start, as a hung service did (0530).
import { closeSync, mkdirSync, openSync, readFileSync, rmSync, writeSync } from "node:fs";
import { dirname } from "node:path";
import { asObject } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import type { ServiceState } from "./launch.ts";
import { type ProcessId, processIdOf } from "./proctree.ts";

/** How long a live holder may go without answering before it counts as hung (0534). */
export const HUNG_MS = 15_000;
/** How long a holder that says it is closing is waited for before it counts as hung. */
export const CLOSING_MS = 60_000;

export interface InstanceOptions {
	/** `<data>/service.lock`. */
	lockFile: string;
	/** What the service `frontier.json` names is doing (`serviceState`). */
	state(): Promise<ServiceState>;
	/** This process (test seam): `process.pid` as it runs now. */
	self?: ProcessId;
	/** Ends a hung holder with every process under it (`killTree`). */
	kill(pid: number): Promise<unknown>;
	hungMs?: number;
	closingMs?: number;
	sleep?(ms: number): Promise<void>;
}

export interface Instance {
	/** Gives the lock up (the service has stopped); only while it still names this process. */
	release(): void;
}

function readHolder(file: string): ProcessId | undefined | "unreadable" {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch {
		return undefined;
	}
	try {
		const value = asObject(JSON.parse(text));
		if (typeof value?.pid === "number" && typeof value.start === "number")
			return { pid: value.pid, start: value.start };
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
 * `hungMs` is ended with its whole tree and its lock taken.
 */
export async function takeInstance(options: InstanceOptions): Promise<Instance | undefined> {
	const self = options.self ?? processIdOf(process.pid);
	if (!self) throw new Error("this process's start time can't be read");
	const sleep = options.sleep ?? Bun.sleep;
	const file = options.lockFile;
	let silentSince: number | undefined;
	let closingSince: number | undefined;
	const release = () => {
		const holder = readHolder(file);
		if (typeof holder === "object" && holder.pid === self.pid && holder.start === self.start)
			rmSync(file, { force: true });
	};
	for (;;) {
		if (create(file, self)) return { release };
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
			log("warn", "instance", `the service ${holder.pid} holds the lock but doesn't answer; ending it and starting`);
			await options.kill(holder.pid);
			rmSync(file, { force: true });
			silentSince = closingSince = undefined;
			continue;
		}
		if (hung) rmSync(file, { force: true });
		await sleep(250);
	}
}
