/** Small file helpers shared by the service, setup and the CLI (one copy each). */
import { timingSafeEqual } from "node:crypto";
import {
	chmodSync,
	lstatSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	readlinkSync,
	realpathSync,
	renameSync,
	rmdirSync,
	rmSync,
	type Stats,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { chmod, lstat, readdir, rmdir, unlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

/**
 * The file a write to `file` lands in: a symlink's target (a dotfile manager's link stays a link), else `file`; the
 * existing file's permission bits, so a private `config.yml` or `mcp.json` stays private (SEC-LOC-9).
 */
function writeTarget(file: string): { path: string; mode: number | undefined } {
	let stat: Stats;
	try {
		stat = lstatSync(file);
	} catch {
		return { path: file, mode: undefined };
	}
	if (!stat.isSymbolicLink()) return { path: file, mode: stat.mode & 0o777 };
	let path: string;
	try {
		path = realpathSync(file);
	} catch {
		// A dangling link: its target is made.
		return { path: resolve(dirname(file), readlinkSync(file)), mode: undefined };
	}
	try {
		return { path, mode: lstatSync(path).mode & 0o777 };
	} catch {
		return { path, mode: undefined };
	}
}

/** Errors Windows gives a rename over a file another process has open (omp reading its overlay, herdr `hud.txt`). */
const RENAME_BUSY: Record<string, true> = { EPERM: true, EBUSY: true, EACCES: true };

/** How often, and how far apart, a refused rename is tried again (0534): 10 times, 100 ms apart. */
const RENAME_TRIES = 10;
const RENAME_RETRY_MS = 100;

/**
 * Writes `file` through a temp file in the same folder and a rename, so a crash or a full disk never leaves it torn.
 * A symlinked `file` writes its target; an existing file keeps its permission bits. `mode` applies to a new file
 * (POSIX only; Windows ignores it). A rename Windows refuses while another process has the file open is tried again
 * for about a second (0534).
 */
export function writeAtomically(file: string, data: string | Uint8Array, mode?: number): void {
	const target = writeTarget(file);
	mkdirSync(dirname(target.path), { recursive: true });
	const temp = `${target.path}.${process.pid}.tmp`;
	try {
		writeFileSync(temp, data, mode === undefined ? undefined : { mode });
		// Past the umask: the bits the file had.
		if (mode === undefined && target.mode !== undefined && process.platform !== "win32") chmodSync(temp, target.mode);
		for (let attempt = 1; ; attempt++) {
			try {
				renameSync(temp, target.path);
				break;
			} catch (error) {
				const busy = process.platform === "win32" && RENAME_BUSY[(error as NodeJS.ErrnoException).code ?? ""];
				if (!busy || attempt >= RENAME_TRIES) throw error;
				Bun.sleepSync(RENAME_RETRY_MS);
			}
		}
	} catch (error) {
		rmSync(temp, { force: true });
		throw error;
	}
}

/** Errors a delete on Windows meets while another process still has the entry open, or while it is read-only. */
const RETRIED: Record<string, true> = { EBUSY: true, EPERM: true, EACCES: true, ENOTEMPTY: true };

/** How many times a busy entry's delete is tried, and how far apart. */
const DELETE_TRIES = 5;
const DELETE_RETRY_MS = 200;

/** A failed delete's `error` on try `attempt`: `gone` when the entry already is, `again` while busy; else it throws. */
function deleteFailed(error: unknown, attempt: number): "gone" | "again" {
	const code = (error as NodeJS.ErrnoException).code ?? "";
	if (code === "ENOENT") return "gone";
	if (attempt >= DELETE_TRIES || !RETRIED[code]) throw error;
	return "again";
}

/** `remove()`, tried again a few times while the entry is busy; an entry already gone counts as removed. */
function retried(remove: () => void): void {
	for (let attempt = 1; ; attempt++) {
		try {
			remove();
			return;
		} catch (error) {
			if (deleteFailed(error, attempt) === "gone") return;
			Bun.sleepSync(DELETE_RETRY_MS);
		}
	}
}

/** `retried` without holding the event loop: the waits between tries are timers (DUP-13). */
async function retriedAsync(remove: () => Promise<void>): Promise<void> {
	for (let attempt = 1; ; attempt++) {
		try {
			await remove();
			return;
		} catch (error) {
			if (deleteFailed(error, attempt) === "gone") return;
			await Bun.sleep(DELETE_RETRY_MS);
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
		// An entry written after the listing (a git still finishing in a worktree) leaves the folder not empty: each try
		// lists it again.
		retried(() => {
			try {
				rmdirSync(path);
			} catch (error) {
				if ((error as NodeJS.ErrnoException).code !== "ENOTEMPTY") throw error;
				for (const name of readdirSync(path)) removeTree(join(path, name));
				rmdirSync(path);
			}
		});
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
 * `removeTree` for the service's event loop (DUP-13): the same removal, links never followed and read-only files made
 * writable first, with the file system asked asynchronously and the waits between a busy entry's tries as timers. The
 * CLI, setup and tests keep the synchronous one.
 */
export async function removeTreeAsync(path: string): Promise<void> {
	let stat: Stats;
	try {
		stat = await lstat(path);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
		throw error;
	}
	if (stat.isSymbolicLink()) {
		await retriedAsync(async () => {
			try {
				await unlink(path);
			} catch (error) {
				const code = (error as NodeJS.ErrnoException).code;
				if (code !== "EPERM" && code !== "EISDIR") throw error;
				await rmdir(path);
			}
		});
		return;
	}
	if (stat.isDirectory()) {
		for (const name of await readdir(path)) await removeTreeAsync(join(path, name));
		await retriedAsync(async () => {
			try {
				await rmdir(path);
			} catch (error) {
				if ((error as NodeJS.ErrnoException).code !== "ENOTEMPTY") throw error;
				for (const name of await readdir(path)) await removeTreeAsync(join(path, name));
				await rmdir(path);
			}
		});
		return;
	}
	await retriedAsync(async () => {
		try {
			await unlink(path);
		} catch (error) {
			const code = (error as NodeJS.ErrnoException).code;
			if (code !== "EPERM" && code !== "EACCES") throw error;
			await chmod(path, 0o666);
			await unlink(path);
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

/** `file` parsed as a JSON object; undefined when it is missing, unreadable, not JSON or not an object. */
export function readJsonObject(file: string): Record<string, unknown> | undefined {
	try {
		return asObject(JSON.parse(readFileSync(file, "utf8")));
	} catch {
		return undefined;
	}
}

/** Constant-time token comparison; tokens of different UTF-8 lengths differ. */
export function sameToken(given: string | null | undefined, token: string): boolean {
	if (!given || Buffer.byteLength(given) !== Buffer.byteLength(token)) return false;
	return timingSafeEqual(Buffer.from(given), Buffer.from(token));
}

/** A loopback port the OS says is free now. */
export function freePort(): number {
	const probe = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
	const { port } = probe;
	probe.stop(true);
	return port;
}
