import { afterEach, beforeEach } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";

/** Runs git in `dir` with a throwaway identity; throws on failure. */
export function sh(dir: string, ...args: string[]): string {
	const result = spawnSync("git", ["-c", "user.name=Test", "-c", "user.email=test@example.com", ...args], {
		cwd: dir,
		encoding: "utf8",
		windowsHide: true,
	});
	if (result.status !== 0) throw new Error(`git ${args.join(" ")}: ${result.stderr}`);
	return result.stdout.trim();
}

/** A git repo with one commit on `branch`, optionally with an origin URL (never fetched). */
export function makeRepo(dir: string, options: { origin?: string; branch?: string } = {}): string {
	mkdirSync(dir, { recursive: true });
	sh(dir, "init", "-q", "-b", options.branch ?? "main");
	writeFileSync(join(dir, "README.md"), "demo\n");
	sh(dir, "add", ".");
	// The folder name keeps two fixture repos made in the same second from sharing a root commit, which would make them
	// one project (0103's `root:<sha>` identity).
	sh(dir, "commit", "-q", "-m", `init ${basename(dir)}`);
	if (options.origin) sh(dir, "remote", "add", "origin", options.origin);
	return realpathSync.native(dir);
}

/**
 * Points Jocasta's config and data dirs at a fresh temp dir for each test (fixture gate, 0026).
 * `env.root` is the temp dir; `env.config` and `env.data` sit under it. `env.defer(fn)` runs `fn` before the
 * temp dir is removed, even when the test failed, so open databases never hold it (EBUSY on Windows).
 */
export function useTempJocasta(): { root: string; config: string; data: string; defer: (fn: () => void) => void } {
	const deferred: (() => void)[] = [];
	const env = { root: "", config: "", data: "", defer: (fn: () => void) => void deferred.push(fn) };
	const saved = { config: process.env.JOCASTA_CODING_AGENT_DIR, data: process.env.JOCASTA_DATA_DIR };
	beforeEach(() => {
		env.root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-")));
		env.config = join(env.root, "config");
		env.data = join(env.root, "data");
		process.env.JOCASTA_CODING_AGENT_DIR = env.config;
		process.env.JOCASTA_DATA_DIR = env.data;
	});
	afterEach(() => {
		for (const fn of deferred.splice(0)) fn();
		process.env.JOCASTA_CODING_AGENT_DIR = saved.config;
		process.env.JOCASTA_DATA_DIR = saved.data;
		if (saved.config === undefined) delete process.env.JOCASTA_CODING_AGENT_DIR;
		if (saved.data === undefined) delete process.env.JOCASTA_DATA_DIR;
		// Bun's rmSync ignores maxRetries; removeTree retries an entry a short-lived git or omp still holds (EBUSY).
		removeTree(env.root);
	});
	return env;
}

/** Per-test timeout for tests that spawn many git processes; a loaded Windows machine can take seconds per test. */
export const GIT_TEST_TIMEOUT = 30_000;

/** Writes `projects.yaml` in the temp config dir. */
export function writeProjectsYaml(configDir: string, text: string): string {
	mkdirSync(configDir, { recursive: true });
	const file = join(configDir, "projects.yaml");
	writeFileSync(file, text);
	return file;
}
