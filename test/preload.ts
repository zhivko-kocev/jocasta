/**
 * Loaded before any test file (bunfig.toml `[test] preload`): points Jocasta's config and data dirs, and omp's agent
 * and config dirs, at a sandbox made for this test run, so nothing a test runs (a test without `useTempJocasta`, a
 * write that lands after a test's own dir was restored) can reach his real folders. `useTempJocasta` restores these
 * values after each test. The sandbox is removed after the last test: a preload's `afterAll` runs once the run's
 * files (`--parallel`: the file's) ended, while bun test never emits the process's `exit`.
 *
 * The repo's own `.git/config` holds the identity its commits are made under (0564); a test's git run that lands in
 * the repo (a relative path from a half-made fixture once set it to the tests' address) is put back and fails the run.
 */
import { afterAll } from "bun:test";
import { mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, relative } from "node:path";
import { removeTree } from "../src/shared/fsx.ts";

const root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-sandbox-")));
process.env.JOCASTA_CODING_AGENT_DIR = join(root, "config");
process.env.JOCASTA_DATA_DIR = join(root, "data");
process.env.PI_CODING_AGENT_DIR = join(root, "omp", "agent");
// omp reads PI_CONFIG_DIR relative to the home folder (it renames `.omp`).
process.env.PI_CONFIG_DIR = relative(homedir(), join(root, "omp"));

const repoConfig = join(import.meta.dir, "..", ".git", "config");
const readRepoConfig = () => {
	try {
		return readFileSync(repoConfig, "utf8");
	} catch {
		// A checkout without its own .git folder (a submodule, a CI copy): nothing to keep.
		return undefined;
	}
};
const repoConfigBefore = readRepoConfig();

afterAll(() => {
	removeTree(root);
	const now = readRepoConfig();
	if (repoConfigBefore === undefined || now === repoConfigBefore) return;
	writeFileSync(repoConfig, repoConfigBefore);
	throw new Error(`a test changed the repo's own .git/config (put back): ${Bun.main}`);
});
