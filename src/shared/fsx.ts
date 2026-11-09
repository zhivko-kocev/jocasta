/** Small file and process helpers shared by the service, setup and the CLI (one copy each). */
import { dlopen, FFIType, type Library, ptr } from "bun:ffi";
import { timingSafeEqual } from "node:crypto";
import {
	chmodSync,
	lstatSync,
	mkdirSync,
	readdirSync,
	renameSync,
	rmdirSync,
	rmSync,
	type Stats,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";

/**
 * Writes `file` through a temp file in the same folder and a rename, so a crash or a full disk never leaves it torn.
 * `mode` applies to the new file (POSIX only; Windows ignores it).
 */
export function writeAtomically(file: string, data: string | Uint8Array, mode?: number): void {
	mkdirSync(dirname(file), { recursive: true });
	const temp = `${file}.${process.pid}.tmp`;
	try {
		writeFileSync(temp, data, mode === undefined ? undefined : { mode });
		renameSync(temp, file);
	} catch (error) {
		rmSync(temp, { force: true });
		throw error;
	}
}

/** Errors a delete on Windows meets while another process still has the entry open, or while it is read-only. */
const RETRIED: Record<string, true> = { EBUSY: true, EPERM: true, EACCES: true, ENOTEMPTY: true };

/** `remove()`, tried again a few times while the entry is busy; an entry already gone counts as removed. */
function retried(remove: () => void, attempts = 5): void {
	for (let attempt = 1; ; attempt++) {
		try {
			remove();
			return;
		} catch (error) {
			const code = (error as NodeJS.ErrnoException).code ?? "";
			if (code === "ENOENT") return;
			if (attempt >= attempts || !RETRIED[code]) throw error;
			Bun.sleepSync(200);
		}
	}
}

/**
 * Removes `path` and everything under it without following links (0484): a symlink or a Windows junction is removed
 * itself and its target is never touched. Read-only files (git's objects) are made writable first.
 */
export function removeTree(path: string): void {
	let stat: Stats;
	try {
		stat = lstatSync(path);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
		throw error;
	}
	if (stat.isSymbolicLink()) {
		// A link to a folder on Windows (junction or directory symlink) may want rmdir rather than unlink.
		retried(() => {
			try {
				unlinkSync(path);
			} catch (error) {
				const code = (error as NodeJS.ErrnoException).code;
				if (code !== "EPERM" && code !== "EISDIR") throw error;
				rmdirSync(path);
			}
		});
		return;
	}
	if (stat.isDirectory()) {
		for (const name of readdirSync(path)) removeTree(join(path, name));
		retried(() => rmdirSync(path));
		return;
	}
	retried(() => {
		try {
			unlinkSync(path);
		} catch (error) {
			const code = (error as NodeJS.ErrnoException).code;
			if (code !== "EPERM" && code !== "EACCES") throw error;
			chmodSync(path, 0o666);
			unlinkSync(path);
		}
	});
}

/**
 * Makes folder `path` (and its missing parents) and, on Linux and macOS, sets it to 0700 so other local users can't
 * read what is in it (0484). Windows keeps the folder's inherited ACL (`%LOCALAPPDATA%` and the profile are private).
 */
export function privateDir(path: string, posix = process.platform !== "win32"): void {
	if (!posix) {
		mkdirSync(path, { recursive: true });
		return;
	}
	mkdirSync(path, { recursive: true, mode: 0o700 });
	chmodSync(path, 0o700);
}

/** `value` when it is a plain JSON object. */
export function asObject(value: unknown): Record<string, unknown> | undefined {
	return value !== null && typeof value === "object" && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: undefined;
}

/** Whether process `pid` is alive (signal 0 only checks). */
export function pidAlive(pid: number): boolean {
	if (!Number.isInteger(pid) || pid <= 0) return false;
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		// EPERM: it exists but belongs to someone else.
		return (error as NodeJS.ErrnoException).code === "EPERM";
	}
}

/** Constant-time token comparison; tokens of different UTF-8 lengths differ. */
export function sameToken(given: string | null | undefined, token: string): boolean {
	if (!given || Buffer.byteLength(given) !== Buffer.byteLength(token)) return false;
	return timingSafeEqual(Buffer.from(given), Buffer.from(token));
}

const KERNEL32 = {
	OpenProcess: { args: [FFIType.u32, FFIType.i32, FFIType.u32], returns: FFIType.ptr },
	GetProcessTimes: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
	CloseHandle: { args: [FFIType.ptr], returns: FFIType.i32 },
} as const;
/** kernel32, opened on the first `processStartedAt` (Windows only). */
let kernel32: Library<typeof KERNEL32> | undefined;

/** `PROCESS_QUERY_LIMITED_INFORMATION`: enough for `GetProcessTimes` on another user's process too. */
const QUERY_LIMITED = 0x1000;
/** FILETIME's epoch (1601-01-01) in Unix epoch milliseconds. */
const FILETIME_EPOCH_MS = 11_644_473_600_000;

/**
 * When process `pid` started, in epoch ms (Windows, from `GetProcessTimes`); undefined elsewhere or when the process
 * can't be opened. Tells a reused pid from the process that wrote a pid file (LC-4).
 */
export function processStartedAt(pid: number): number | undefined {
	if (process.platform !== "win32" || !Number.isInteger(pid) || pid <= 0) return undefined;
	try {
		kernel32 ??= dlopen("kernel32.dll", KERNEL32);
		const handle = kernel32.symbols.OpenProcess(QUERY_LIMITED, 0, pid);
		if (!handle) return undefined;
		try {
			const times = new BigUint64Array(4);
			const at = (i: number) => ptr(times, i * 8);
			if (!kernel32.symbols.GetProcessTimes(handle, at(0), at(1), at(2), at(3))) return undefined;
			return Number((times[0] as bigint) / 10_000n) - FILETIME_EPOCH_MS;
		} finally {
			kernel32.symbols.CloseHandle(handle);
		}
	} catch {
		return undefined;
	}
}

/** A loopback port the OS says is free now. */
export function freePort(): number {
	const probe = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
	const { port } = probe;
	probe.stop(true);
	return port;
}
