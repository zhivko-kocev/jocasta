import { afterEach, beforeEach } from "bun:test";
import { mkdtempSync, realpathSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, relative } from "node:path";
import { removeTree } from "./remove.ts";

/**
 * Points Jocasta's config and data dirs, and omp's agent and config dirs, at a fresh temp dir for each test (fixture
 * gate, 0026): nothing reads his own `~/.omp` (its model cache, its host registry).
 * `env.root` is the temp dir; `env.config` and `env.data` sit under it. `env.defer(fn)` runs `fn` before the
 * temp dir is removed, even when the test failed, so open databases never hold it (EBUSY on Windows). After each test
 * the variables go back to the process's sandbox (test/preload.ts), never unset: a write that lands late stays there.
 */
export function useTempJocasta(): { root: string; config: string; data: string; defer: (fn: () => void) => void } {
	const deferred: (() => void)[] = [];
	const env = { root: "", config: "", data: "", defer: (fn: () => void) => void deferred.push(fn) };
	const names = ["JOCASTA_CODING_AGENT_DIR", "JOCASTA_DATA_DIR", "PI_CODING_AGENT_DIR", "PI_CONFIG_DIR"] as const;
	const sandbox = names.map((name) => {
		const value = process.env[name];
		if (value === undefined) throw new Error(`${name} isn't set: run the tests with bun test (test/preload.ts)`);
		return value;
	});
	beforeEach(() => {
		env.root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-")));
		env.config = join(env.root, "config");
		env.data = join(env.root, "data");
		process.env.JOCASTA_CODING_AGENT_DIR = env.config;
		process.env.JOCASTA_DATA_DIR = env.data;
		process.env.PI_CODING_AGENT_DIR = join(env.root, "omp", "agent");
		// omp reads PI_CONFIG_DIR relative to the home folder (it renames `.omp`).
		process.env.PI_CONFIG_DIR = relative(homedir(), join(env.root, "omp"));
	});
	afterEach(() => {
		for (const fn of deferred.splice(0)) fn();
		names.forEach((name, i) => {
			process.env[name] = sandbox[i];
		});
		// Bun's rmSync ignores maxRetries; removeTree retries an entry a short-lived git or omp still holds (EBUSY).
		removeTree(env.root);
	});
	return env;
}
