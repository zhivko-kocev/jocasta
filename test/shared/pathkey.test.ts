import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, realpathSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import { isInside, pathKey, realPath, sameDir, samePath } from "../../src/shared/pathkey.ts";

const dirs: string[] = [];
afterEach(() => {
	for (const dir of dirs.splice(0)) removeTree(dir);
});

function tempDir(): string {
	const dir = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-pathkey-")));
	dirs.push(dir);
	return dir;
}

const windows = process.platform === "win32";

test("pathKey: resolved, forward slashes, no trailing slash, case folded only on Windows", () => {
	const at = resolve("Acme", "App");
	const key = pathKey(`${at}${windows ? "\\" : "/"}`);
	expect(key).toBe((windows ? at.toLowerCase() : at).replaceAll("\\", "/"));
	expect(pathKey("Acme/App")).toBe(key);
	expect(samePath(at, at.toUpperCase())).toBe(windows);
	expect(samePath(join(at, "x", ".."), at)).toBe(true);
});

test("isInside: the folder itself and what is under it, never a sibling sharing its prefix", () => {
	const root = resolve("acme.wt");
	expect(isInside(root, root)).toBe(true);
	expect(isInside(root, join(root, "x", "y"))).toBe(true);
	expect(isInside(root, resolve("acme.wt-old", "x"))).toBe(false);
	expect(isInside(join(root, "x"), root)).toBe(false);
});

test("realPath: links followed; a missing path expanded through its nearest existing ancestor", () => {
	const dir = tempDir();
	const real = join(dir, "real");
	mkdirSync(real);
	const link = join(dir, "link");
	symlinkSync(real, link, "junction");
	expect(realPath(link)).toBe(real);
	expect(realPath(join(link, "missing", "deeper"))).toBe(join(real, "missing", "deeper"));
	expect(sameDir(link, real)).toBe(true);
	expect(samePath(link, real)).toBe(false);
});
