import { rmSync } from "node:fs";

/** How many times a busy entry's delete is tried, and how far apart. */
const DELETE_TRIES = 5;
const DELETE_RETRY_MS = 200;
const BUSY = new Set(["EBUSY", "EPERM", "EACCES", "ENOTEMPTY"]);

/**
 * Removes `path` and everything under it, links never followed (a symlink or Windows junction is removed itself, its
 * target untouched). Bun's `rmSync` ignores `maxRetries`, so an entry a short-lived git or omp still holds (EBUSY on
 * Windows) is tried again here; a path already gone counts as removed.
 */
export function removeTree(path: string): void {
	for (let attempt = 1; ; attempt++) {
		try {
			rmSync(path, { recursive: true, force: true });
			return;
		} catch (error) {
			const code = (error as NodeJS.ErrnoException).code ?? "";
			if (attempt >= DELETE_TRIES || !BUSY.has(code)) throw error;
			Bun.sleepSync(DELETE_RETRY_MS);
		}
	}
}
