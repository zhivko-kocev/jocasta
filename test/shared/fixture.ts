import { afterEach, beforeEach } from "bun:test";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import { isInside } from "../../src/shared/pathkey.ts";

/** This repo's root: no test's git may run in it. */
const REPO_ROOT = resolve(import.meta.dir, "..", "..");

/**
 * Runs git in `dir` with a throwaway identity; throws on failure. An empty `dir` (a fixture path read before its
 * `beforeEach` set it) or one inside this repo is refused before git runs: git would act on the repo itself, as a
 * test once did when it set the repo's commit address to the tests' own.
 */
export function sh(dir: string, ...args: string[]): string {
	if (dir === "" || isInside(REPO_ROOT, dir)) {
		throw new Error(`git ${args.join(" ")}: refused in ${dir === "" ? "an empty folder (this repo)" : dir}`);
	}
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

/** Template repos by name and options, made once per test process and removed when it exits. */
const templates = new Map<string, string>();
let templateRoot = "";

/**
 * `makeRepo(dir, options)`'s repo, copied from a template made once per process for `dir`'s folder name and the
 * options: a copy costs one folder copy instead of three git spawns. Copies of one template share their root commit,
 * which 0103 makes one project, so tests about distinct repos or identity call makeRepo instead.
 */
export function copyRepo(dir: string, options: { origin?: string; branch?: string } = {}): string {
	const key = JSON.stringify([basename(dir), options.origin, options.branch]);
	let template = templates.get(key);
	if (template === undefined) {
		if (templateRoot === "") {
			templateRoot = mkdtempSync(join(tmpdir(), "jocasta-template-"));
			const root = templateRoot;
			process.on("exit", () => removeTree(root));
		}
		template = makeRepo(join(templateRoot, String(templates.size), basename(dir)), options);
		templates.set(key, template);
	}
	mkdirSync(dirname(dir), { recursive: true });
	cpSync(template, dir, { recursive: true });
	return realpathSync.native(dir);
}

/**
 * A fresh temp dir per call (`mkdtemp` of `prefix` under `parent`, the OS temp dir by default); every dir made during
 * a test is removed after it with `removeTree` (Bun's rmSync ignores maxRetries: EBUSY on Windows, 0572).
 */
export function useTempDirs(prefix: string): (parent?: string) => string {
	const made: string[] = [];
	afterEach(() => {
		for (const dir of made.splice(0)) removeTree(dir);
	});
	return (parent = tmpdir()) => {
		const dir = mkdtempSync(join(parent, prefix));
		made.push(dir);
		return dir;
	};
}

/** Writes each `path: content` under `root`, making the folders on the way. */
export function writeFiles(root: string, files: Record<string, string>): void {
	for (const [path, content] of Object.entries(files)) {
		mkdirSync(dirname(join(root, path)), { recursive: true });
		writeFileSync(join(root, path), content);
	}
}

/** A parsed JSON row as a test reads it: untyped, like `JSON.parse`'s result. */
// biome-ignore lint/suspicious/noExplicitAny: tests read rows field by field, as they did with JSON.parse.
export type JsonRow = any;

/** The JSON rows of a JSONL file, blank lines skipped; a line that isn't JSON throws (a test wants to see it). */
export function readJsonl<T = JsonRow>(file: string): T[] {
	return readFileSync(file, "utf8")
		.split("\n")
		.filter((line) => line.trim() !== "")
		.map((line) => JSON.parse(line) as T);
}

/**
 * The fake harnesses' call log (test/evals/fixtures: each run appends one JSON line to `JOCASTA_TEST_OMP_LOG`). The
 * returned function points the variable at a fresh file and returns the reader of its rows; the variable is restored
 * after each test.
 */
export function useOmpLog<T = JsonRow>(): () => { file: string; calls: () => T[] } {
	const saved = process.env.JOCASTA_TEST_OMP_LOG;
	const tempDir = useTempDirs("jocasta-omp-log-");
	afterEach(() => {
		if (saved === undefined) delete process.env.JOCASTA_TEST_OMP_LOG;
		else process.env.JOCASTA_TEST_OMP_LOG = saved;
	});
	return () => {
		const file = join(tempDir(), "omp.jsonl");
		process.env.JOCASTA_TEST_OMP_LOG = file;
		return { file, calls: () => readJsonl<T>(file) };
	};
}

/** How long `poll` looks and how often; `what` names the condition, which makes a timeout throw. */
export interface PollOptions {
	timeoutMs?: number;
	intervalMs?: number;
	what?: string;
}

/**
 * Checks `check` every `intervalMs` (20 ms) until it holds or `timeoutMs` (10 s) passed, on the OS clock (real
 * processes and sockets don't follow a fake one). Resolves whether it held; with `what`, a timeout throws
 * `timed out waiting for <what>` instead.
 */
export async function poll(
	check: () => boolean | Promise<boolean>,
	{ timeoutMs = 10_000, intervalMs = 20, what }: PollOptions = {},
): Promise<boolean> {
	const end = Date.now() + timeoutMs;
	while (!(await check())) {
		if (Date.now() >= end) {
			if (what !== undefined) throw new Error(`timed out waiting for ${what}`);
			return false;
		}
		await Bun.sleep(intervalMs);
	}
	return true;
}

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

/** Per-test timeout for tests that spawn many git processes; a loaded Windows machine can take seconds per test. */
export const GIT_TEST_TIMEOUT = 30_000;

/** Writes `projects.yaml` in the temp config dir. */
export function writeProjectsYaml(configDir: string, text: string): string {
	mkdirSync(configDir, { recursive: true });
	const file = join(configDir, "projects.yaml");
	writeFileSync(file, text);
	return file;
}
