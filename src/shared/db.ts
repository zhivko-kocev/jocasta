import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

/**
 * A DB that can't be opened, is newer than this Jocasta, or stays locked (0101).
 * `message` is the one-line text after `jocasta: `; the caller prints it and exits 1.
 */
export class DbError extends Error {
	override name = "DbError";
}

/** RFC3339 UTC with milliseconds (0100). */
export function nowIso(): string {
	return new Date().toISOString();
}

const BUSY_TIMEOUT_MS = 5000;

/**
 * Opens a Jocasta DB file: the connection settings in 0099's order, then the numbered migrations (0100).
 * `migrations[i]` takes the file from `user_version` i to i + 1, each in its own transaction.
 * Never deletes, recreates or downgrades a file (0101). `busyTimeoutMs` is for tests only.
 */
export function openDb(
	path: string,
	migrations: readonly string[],
	{ busyTimeoutMs = BUSY_TIMEOUT_MS }: { busyTimeoutMs?: number } = {},
): Database {
	mkdirSync(dirname(path), { recursive: true });
	let db: Database | undefined;
	try {
		db = new Database(path, { create: true, strict: true });
		db.exec(`PRAGMA busy_timeout = ${busyTimeoutMs}`);
		// The version is read before anything writes, so a newer file is refused untouched (0101).
		const row = db.query<{ user_version: number }, []>("PRAGMA user_version").get();
		const version = row?.user_version ?? 0;
		if (version > migrations.length) {
			throw new DbError(`${path} is schema ${version}; this Jocasta knows up to ${migrations.length}. Update Jocasta.`);
		}
		db.exec("PRAGMA journal_mode = WAL");
		db.exec("PRAGMA synchronous = NORMAL");
		db.exec("PRAGMA foreign_keys = ON");
		for (let next = version; next < migrations.length; next++) {
			const sql = migrations[next] as string;
			const handle = db;
			handle.transaction(() => {
				handle.exec(sql);
				handle.exec(`PRAGMA user_version = ${next + 1}`);
			})();
		}
		return db;
	} catch (error) {
		db?.close();
		throw dbFailure(path, error, busyTimeoutMs);
	}
}

/** Maps a driver error to 0101's messages; anything else passes through. */
function dbFailure(path: string, error: unknown, busyTimeoutMs: number): unknown {
	if (error instanceof DbError) return error;
	if (error instanceof Error && "code" in error) {
		const code = String(error.code ?? "");
		if (code.startsWith("SQLITE_BUSY") || code.startsWith("SQLITE_LOCKED")) {
			return new DbError(`${path} is locked by another Jocasta process (waited ${busyTimeoutMs / 1000} s).`);
		}
		if (code.startsWith("SQLITE_")) {
			return new DbError(`${path} can't be opened: ${error.message}. It was left as is.`);
		}
	}
	return error;
}
