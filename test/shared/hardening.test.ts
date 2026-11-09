import { afterEach, beforeEach, expect, test } from "bun:test";
import {
	chmodSync,
	existsSync,
	lstatSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	realpathSync,
	rmSync,
	statSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { withoutJocastaKeys } from "../../src/frontier/server/service.ts";
import { privateDir, removeTree } from "../../src/shared/fsx.ts";
import { configDir, dataDir } from "../../src/shared/paths.ts";

const REPO = resolve(import.meta.dir, "..", "..");
const posix = process.platform !== "win32";
let root = "";

beforeEach(() => {
	root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-hardening-")));
});
afterEach(() => {
	removeTree(root);
});

/** A folder link at `link` to `target`: a junction on Windows (no admin needed), a symlink elsewhere. */
function folderLink(target: string, link: string): void {
	symlinkSync(target, link, process.platform === "win32" ? "junction" : "dir");
}

test("removeTree removes a link inside the tree, never what it points at (0484)", () => {
	const victim = join(root, "victim");
	mkdirSync(join(victim, "deep"), { recursive: true });
	writeFileSync(join(victim, "keep.txt"), "his");
	writeFileSync(join(victim, "deep", "keep.txt"), "his");
	const tree = join(root, "worktree");
	mkdirSync(join(tree, "node_modules"), { recursive: true });
	writeFileSync(join(tree, "a.txt"), "a");
	folderLink(victim, join(tree, "node_modules", "linked"));
	expect(lstatSync(join(tree, "node_modules", "linked")).isSymbolicLink()).toBe(true);

	removeTree(tree);

	expect(existsSync(tree)).toBe(false);
	expect(readFileSync(join(victim, "keep.txt"), "utf8")).toBe("his");
	expect(readFileSync(join(victim, "deep", "keep.txt"), "utf8")).toBe("his");
});

test("removeTree given a link itself removes only the link; a dangling one too", () => {
	const victim = join(root, "victim");
	mkdirSync(victim);
	writeFileSync(join(victim, "keep.txt"), "his");
	const link = join(root, "link");
	folderLink(victim, link);
	removeTree(link);
	expect(existsSync(link)).toBe(false);
	expect(existsSync(join(victim, "keep.txt"))).toBe(true);

	const gone = join(root, "gone");
	mkdirSync(gone);
	const dangling = join(root, "dangling");
	folderLink(gone, dangling);
	rmSync(gone, { recursive: true });
	removeTree(dangling);
	expect(() => lstatSync(dangling)).toThrow();
});

test("removeTree removes read-only files (git's objects) and is quiet about a path already gone", () => {
	const tree = join(root, "tree");
	mkdirSync(join(tree, ".git", "objects"), { recursive: true });
	const object = join(tree, ".git", "objects", "ab");
	writeFileSync(object, "blob");
	chmodSync(object, 0o444);
	removeTree(tree);
	expect(existsSync(tree)).toBe(false);
	removeTree(tree);
	removeTree(join(root, "never"));
});

test.skipIf(!posix)("Jocasta's data and config folders are 0700 on Linux and macOS, made or tightened (0484)", () => {
	const data = join(root, "data");
	const config = join(root, "config");
	mkdirSync(config, { mode: 0o755 });
	chmodSync(config, 0o755);
	const saved = { data: process.env.JOCASTA_DATA_DIR, config: process.env.JOCASTA_CODING_AGENT_DIR };
	process.env.JOCASTA_DATA_DIR = data;
	process.env.JOCASTA_CODING_AGENT_DIR = config;
	try {
		expect(dataDir()).toBe(data);
		expect(configDir()).toBe(config);
	} finally {
		process.env.JOCASTA_DATA_DIR = saved.data;
		process.env.JOCASTA_CODING_AGENT_DIR = saved.config;
		if (saved.data === undefined) delete process.env.JOCASTA_DATA_DIR;
		if (saved.config === undefined) delete process.env.JOCASTA_CODING_AGENT_DIR;
	}
	expect(statSync(data).mode & 0o777).toBe(0o700);
	expect(statSync(config).mode & 0o777).toBe(0o700);
});

test("privateDir makes the folder and its parents; 0700 where the OS has modes", () => {
	const dir = join(root, "a", "b");
	privateDir(dir);
	expect(statSync(dir).isDirectory()).toBe(true);
	if (posix) expect(statSync(dir).mode & 0o777).toBe(0o700);
});

test("on Windows dataDir and configDir only name the folders; nothing is made", () => {
	if (posix) return;
	const saved = process.env.JOCASTA_DATA_DIR;
	process.env.JOCASTA_DATA_DIR = join(root, "unmade");
	try {
		expect(dataDir()).toBe(join(root, "unmade"));
	} finally {
		process.env.JOCASTA_DATA_DIR = saved;
		if (saved === undefined) delete process.env.JOCASTA_DATA_DIR;
	}
	expect(existsSync(join(root, "unmade"))).toBe(false);
});

test("mise's env can't override herdr's, Jocasta's or omp's variables in a pane, in any case (0484)", () => {
	expect(
		withoutJocastaKeys({
			PATH: "C:\\mise\\bin",
			API_PORT: "3010",
			HERDR_PANE_ID: "w1-1",
			herdr_session: "x",
			JOCASTA_DATA_DIR: "C:\\elsewhere",
			Jocasta_Boss_Key: "k",
			PI_CODING_AGENT_DIR: "C:\\other",
			PIP_INDEX_URL: "https://example.org/simple",
		}),
	).toEqual({ PATH: "C:\\mise\\bin", API_PORT: "3010", PIP_INDEX_URL: "https://example.org/simple" });
});

test("the whiteboard bridge sets no HTML (0484)", () => {
	const bridge = readFileSync(join(REPO, "src", "frontier", "canvas", "bridge.ts"), "utf8");
	expect(bridge).not.toContain("innerHTML");
	expect(bridge).not.toContain("outerHTML");
	expect(bridge).not.toContain("insertAdjacentHTML");
});
