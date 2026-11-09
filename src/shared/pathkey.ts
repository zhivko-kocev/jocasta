/**
 * Folders as identity (0103, 0521): the one key form folders are compared and stored by, and the one resolver of a
 * folder's real long name.
 */
import { realpathSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

/**
 * The real, absolute form of a path: links followed and 8.3 short names (`C:\Users\ZHIVKO~1`, what a short `%TEMP%`
 * gives) expanded to long ones. A missing path is expanded through its nearest existing ancestor, the rest kept.
 */
export function realPath(path: string): string {
	const rest: string[] = [];
	let current = resolve(path);
	for (;;) {
		try {
			return join(realpathSync.native(current), ...rest.reverse());
		} catch {
			const parent = dirname(current);
			if (parent === current) return resolve(path);
			rest.push(basename(current));
			current = parent;
		}
	}
}

/**
 * A path as identity: resolved to an absolute path, forward slashes, no trailing slash, and case-folded on Windows
 * (whose file systems ignore case). Links and 8.3 short names are kept as written: key `realPath(path)` when two
 * spellings of one folder must meet.
 */
export function pathKey(path: string): string {
	const forward = resolve(path).replaceAll("\\", "/").replace(/\/+$/, "");
	return process.platform === "win32" ? forward.toLowerCase() : forward;
}

/** Whether `a` and `b` name the same path, whatever their slashes, case (Windows) or trailing separator. */
export function samePath(a: string, b: string): boolean {
	return pathKey(a) === pathKey(b);
}

/** Whether `a` and `b` are the same folder, links and short names followed (`realPath`). */
export function sameDir(a: string, b: string): boolean {
	return pathKey(realPath(a)) === pathKey(realPath(b));
}

/** Whether `path` is `root` or inside it, by key: `<root>-old/x` is not inside `<root>`. */
export function isInside(root: string, path: string): boolean {
	const top = pathKey(root);
	const at = pathKey(path);
	return at === top || at.startsWith(top.endsWith("/") ? top : `${top}/`);
}
