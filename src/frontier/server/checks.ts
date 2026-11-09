/**
 * Jocasta's check runner (0542, 0546, 0559): the review's checks, `done_when`, the check on idle and the thread's
 * `check` tool all run here, keyed by the git tree they ran on.
 *
 * - **Key:** the tree git would commit from the worktree as it is (a copy of its index, `git add -A`, `git write-tree`,
 *   the real index untouched), plus the tool versions (`mise current`) and the check's command. A key that already has
 *   a result answers from it, PASS and FAIL alike, so a check on an unchanged tree costs the key computation only.
 * - **Store:** `<data>/checks/<tree 12>/<name>.json` (`{command, versions, exit, ms, at, gates}`) beside `<name>.log`
 *   (stdout and stderr as they interleaved); a failing mise task's own lines also go to `<task>.log` in the same
 *   folder. A name's `/`, `\` and `:` become `-` in its file names. Folders untouched for 14 days are removed once a
 *   day (0582 PERF-11); `hits.log` stays.
 * - **Single flight:** one run per key, across processes: `<name>.lock` taken exclusively with the runner's pid and
 *   start time in it; a caller finding it waits for the result (joins), and a lock whose process is gone (or whose
 *   pid now names a process started at another time), or held past the check's time plus a grace, is taken over. In
 *   one process a second caller joins the first's promise. A run's checks run one after another.
 * - **mise:** the project's check runs as `mise run -c <task>`, so a failing task doesn't abort its siblings; mise's
 *   own artifact cache replays unchanged tasks underneath only when the project's `mise.toml` switches it on. Jocasta
 *   never writes a project's files.
 * - **Gates (0546):** each check (each mise task the run names) is one PASS/FAIL line; a failure gets its failures,
 *   its log and the narrowest rerun: `bun test <the failing files>` when bun's test runner named them, else the
 *   check's own command. Every gate passing says nothing.
 *
 * Shared hits (a result reused, or a run joined, rather than run here) are counted for H8 (K11): one line per hit in
 * `<data>/checks/hits.log`, which {@link readCheckHits} counts.
 */
import {
	appendFileSync,
	closeSync,
	copyFileSync,
	existsSync,
	linkSync,
	mkdirSync,
	openSync,
	readFileSync,
	renameSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { readdir, readFile, stat, writeFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { localDay } from "../../shared/day.ts";
import { readJsonObject, removeTreeAsync, writeAtomically } from "../../shared/fsx.ts";
import { gitAsync } from "../../shared/git.ts";
import { log } from "../../shared/log.ts";
import { onPath, runProgram } from "../../shared/process.ts";
import { killTree, pidAlive, processStart } from "../../shared/proctree.ts";

/** One check to run: its gate name (`verify`, `done_when`, `done_when-2`) and its shell command. */
export interface CheckSpec {
	name: string;
	command: string;
	/** Run through mise (`mise run -c <task>`): its output names each task's lines and failures. */
	mise?: boolean;
}

/** One failing test as the runner named it. */
export interface TestFailure {
	file: string;
	line: number | null;
	test: string;
}

/** One PASS/FAIL line of a gate reply. */
export interface Gate {
	name: string;
	command: string;
	pass: boolean;
	/** The failing tests the runner named; empty when it named none. */
	failures: TestFailure[];
	/** The log to read; set on a failure. */
	log?: string;
	/** The narrowest command that runs the failure again; set on a failure. */
	rerun?: string;
}

/** One check's result for a key, and how it was had. */
export interface CheckResult {
	spec: CheckSpec;
	/** The command's exit code; null when it couldn't run or ran out of time (never stored). */
	exit: number | null;
	log: string;
	ms: number;
	/** `ran` here, `hit` a stored result, `joined` another caller's run. */
	how: "ran" | "hit" | "joined";
	gates: Gate[];
}

/** The checks of one tree. */
export interface CheckRun {
	tree: string;
	results: CheckResult[];
	passed: boolean;
}

export interface CheckRunnerOptions {
	/** `<data>/checks`. */
	dir: string;
	/** The tool versions in the key: `mise current` in `cwd` by default; "" without mise. */
	versions?(cwd: string, env: Record<string, string | undefined>): Promise<string>;
	/** How long one check may run before it is killed (and nothing stored); 30 minutes. */
	timeoutMs?: number;
	/** How often a waiter looks for the running check's result; 250 ms. */
	pollMs?: number;
}

const CHECK_TIMEOUT_MS = 30 * 60_000;
const POLL_MS = 250;
/** How long past the check's own time a waiter gives a live lock holder before taking its lock over. */
const LOCK_GRACE_MS = 60_000;
/** How long one git call of the tree key may take. */
const GIT_TIMEOUT_MS = 30_000;
const DAY_MS = 24 * 60 * 60_000;
/** Days a stored result is kept after its folder was last touched (0582 PERF-11). */
const RESULT_DAYS = 14;
const PRUNE_EVERY_MS = DAY_MS;
/** The file in the checks folder holding the local day the last prune pass ran (once a day, 0582 PERF-11). */
const PRUNED_ON = "pruned-on";
/** Index copies in `tmp` older than this are a crash's leftovers. */
const TMP_MAX_AGE_MS = 60 * 60_000;
/** A result folder's name: the tree's first 12 hex digits. */
const RESULT_FOLDER = /^[0-9a-f]{12}$/;

/** Stored with each result. */
interface Stored {
	command: string;
	versions: string;
	exit: number;
	ms: number;
	at: number;
	/** The result's gates; absent from results stored before they were kept, whose log is read instead. */
	gates?: Gate[];
}

/** The argv that runs one shell command line: cmd.exe on Windows (`/s /c "<line>"`, verbatim), else `sh -c`. */
function shellArgv(command: string): { argv: string[]; verbatim: boolean } {
	if (process.platform === "win32") {
		return { argv: [process.env.ComSpec ?? "cmd.exe", "/d", "/s", "/c", `"${command}"`], verbatim: true };
	}
	return { argv: ["sh", "-c", command], verbatim: false };
}

/** `mise current` in `cwd` (the resolved tool versions), "" when mise isn't there or fails. */
async function miseCurrent(cwd: string, env: Record<string, string | undefined>): Promise<string> {
	const mise = onPath("mise", env);
	if (!mise) return "";
	try {
		const run = await runProgram([mise, "current"], { cwd, env, timeoutMs: 15_000 });
		return run.code === 0 ? run.stdout.trim() : "";
	} catch {
		return "";
	}
}

// biome-ignore lint/suspicious/noControlCharactersInRegex: the escape character is what an ANSI color code starts with.
const ANSI = /\u001b\[[0-9;?]*[ -/]*[@-~]/g;

/** A mise task's output prefix: `[test] …`. */
const TASK_PREFIX = /^\[([\w:./-]+)\] ?(.*)$/;

/**
 * The tasks a `mise run` log names, in order, and which failed: a task's lines start `[<task>] `; mise ends a failed
 * one with `[<task>] ERROR task failed` and sums them up as `[<task>] exited with status <n>`.
 */
export function miseTasks(text: string): { name: string; failed: boolean }[] {
	const tasks = new Map<string, boolean>();
	for (const raw of text.split(/\r?\n/)) {
		const line = raw.replace(ANSI, "");
		for (const match of line.matchAll(/\[([\w:./-]+)\] exited with status \d+/g)) tasks.set(match[1] as string, true);
		const prefixed = TASK_PREFIX.exec(line);
		if (!prefixed) continue;
		const name = prefixed[1] as string;
		if (!tasks.has(name)) tasks.set(name, false);
		if (/^ERROR\b.*\bfailed\b/.test(prefixed[2] ?? "")) tasks.set(name, true);
	}
	return [...tasks].map(([name, failed]) => ({ name, failed }));
}

/** A task's own lines of a mise log, their prefix cut. */
function taskLines(text: string, task: string): string {
	const out: string[] = [];
	for (const raw of text.split(/\r?\n/)) {
		const prefixed = TASK_PREFIX.exec(raw.replace(ANSI, ""));
		if (prefixed && prefixed[1] === task) out.push(prefixed[2] ?? "");
	}
	return `${out.join("\n")}\n`;
}

/** A path as a test runner prints it, made relative to `cwd` with forward slashes. */
function relativeTo(cwd: string, path: string): string {
	const slashed = path.replaceAll("\\", "/");
	const root = `${cwd.replaceAll("\\", "/").replace(/\/$/, "")}/`;
	return slashed.toLowerCase().startsWith(root.toLowerCase()) ? slashed.slice(root.length) : slashed;
}

/**
 * The failing tests bun's test runner names in `text`: each file's results follow a `<file>:` line, a failure is
 * `(fail) <name>`, and its error's stack, printed before it, points at the test file's line. Lines a mise task
 * prefixed are read without the prefix.
 */
export function bunFailures(text: string, cwd: string): TestFailure[] {
	const failures: TestFailure[] = [];
	let file: string | undefined;
	let line: number | null = null;
	for (const raw of text.split(/\r?\n/)) {
		let row = raw.replace(ANSI, "");
		const prefixed = TASK_PREFIX.exec(row);
		if (prefixed) row = prefixed[2] ?? "";
		const header = /^(\S.*\.(?:test|spec|_test|_spec)\.[cm]?[jt]sx?):$/.exec(row.trim());
		if (header) {
			file = relativeTo(cwd, header[1] as string);
			line = null;
			continue;
		}
		if (file) {
			const name = file.split("/").pop() as string;
			for (const at of row.matchAll(/([^\s()]+):(\d+):\d+\)?\s*$/g)) {
				if ((at[1] as string).replaceAll("\\", "/").endsWith(name)) line = Number(at[2]);
			}
		}
		const failed = /^\s*\(fail\) (.+?)(?: \[[\d.]+m?s\])?\s*$/.exec(row);
		if (failed && file) {
			failures.push({ file, line, test: failed[1] as string });
			line = null;
		} else if (/^\s*\(pass\) /.test(row)) line = null;
	}
	return failures;
}

/** The gates of one check's log: one per mise task it names, else the check itself. */
export function gatesOf(spec: CheckSpec, exit: number | null, text: string, logFile: string, cwd: string): Gate[] {
	const gate = (name: string, command: string, pass: boolean, lines: string, logPath: string): Gate => {
		const failures = pass ? [] : bunFailures(lines, cwd);
		const files = [...new Set(failures.map((f) => f.file))];
		const rerun = files.map((file) => (file.includes(" ") ? `"${file}"` : file)).join(" ");
		return {
			name,
			command,
			pass,
			failures,
			...(pass ? {} : { log: logPath, rerun: files.length > 0 ? `bun test ${rerun}` : command }),
		};
	};
	const tasks = spec.mise ? miseTasks(text) : [];
	const named = tasks.filter((task) => task.name !== spec.name || tasks.length === 1);
	if (named.length > 1 || (named.length === 1 && named[0]?.name !== spec.name)) {
		const anyFailed = named.some((task) => task.failed);
		return named.map((task) => {
			// A failing run whose log names no failed task: every task it named is said as failing.
			const pass = exit === 0 || (anyFailed && !task.failed);
			const taskLog = join(logFile, "..", `${fileName(task.name)}.log`);
			return gate(task.name, `mise run ${task.name}`, pass, text, pass ? logFile : taskLog);
		});
	}
	return [gate(spec.name, spec.command, exit === 0, text, logFile)];
}

/** `3f9a1c` of a tree. */
export const shortTree = (tree: string): string => tree.slice(0, 6);

/**
 * 0546's gate reply for a run; "" when every gate passes. Lines: `checks: FAIL (tree 3f9a1c)`, then per gate
 * `  PASS  <name padded>  <command>`; a failing gate adds its failures, `log:` and `rerun:` lines under it.
 */
export function gateReply(run: CheckRun): string {
	const gates = run.results.flatMap((result) => result.gates);
	if (run.passed) return "";
	const width = Math.max(...gates.map((gate) => gate.name.length));
	const lines = [`checks: FAIL (tree ${shortTree(run.tree)})`];
	for (const gate of gates) {
		lines.push(`  ${gate.pass ? "PASS" : "FAIL"}  ${gate.name.padEnd(width)}  ${gate.command}`);
		if (gate.pass) continue;
		const pad = " ".repeat(8);
		if (gate.failures.length > 0) {
			const said = gate.failures.map((f) => `${f.file}${f.line === null ? "" : `:${f.line}`} "${f.test}"`);
			lines.push(`${pad}${gate.failures.length} failed: ${said.join(", ")}`);
		}
		if (gate.log) lines.push(`${pad}log: ${gate.log}`);
		if (gate.rerun) lines.push(`${pad}rerun: ${gate.rerun}`);
	}
	return lines.join("\n");
}

/** How many shared check hits `<data>/checks/hits.log` holds (H8, K11). */
export function readCheckHits(checksDir: string): number {
	try {
		return readFileSync(join(checksDir, "hits.log"), "utf8").split("\n").filter(Boolean).length;
	} catch {
		return 0;
	}
}

/** The checks' folder under a data dir. */
export const checksDir = (data: string): string => join(data, "checks");

export class CheckRunner {
	/** Results reused or runs joined, rather than run here (K11's shared check hits). */
	hits = 0;
	/** Checks run here. */
	runs = 0;
	/** The day's prune pass, started with the runner unless one already ran today (0582 PERF-11). */
	readonly pruning: Promise<void>;
	private readonly inFlight = new Map<string, Promise<CheckResult>>();
	/** The checks running here, by pid: `stop` ends their trees. */
	private readonly children = new Set<number>();
	private stopped = false;
	private readonly pruneTimer: Timer;
	private readonly options: Required<Pick<CheckRunnerOptions, "dir" | "timeoutMs" | "pollMs" | "versions">>;

	constructor(options: CheckRunnerOptions) {
		this.options = {
			dir: options.dir,
			timeoutMs: options.timeoutMs ?? CHECK_TIMEOUT_MS,
			pollMs: options.pollMs ?? POLL_MS,
			versions: options.versions ?? miseCurrent,
		};
		// Stored results age out (0582 PERF-11): once a day, at the service's first start that day and daily while it runs.
		this.pruning = this.pruneDaily();
		this.pruneTimer = setInterval(() => void this.pruneDaily(), PRUNE_EVERY_MS);
		this.pruneTimer.unref();
	}

	/** Ends every check running here (their results aren't stored); later runs answer from stored results only. */
	async stop(): Promise<void> {
		this.stopped = true;
		clearInterval(this.pruneTimer);
		await Promise.allSettled([...this.children].map((pid) => killTree(pid)));
		await Promise.allSettled([...this.inFlight.values()]);
	}

	/** `prune`, unless a pass already ran today; the day is kept in `<dir>/pruned-on`. */
	private async pruneDaily(): Promise<void> {
		const stamp = join(this.options.dir, PRUNED_ON);
		const today = localDay(Date.now());
		try {
			if ((await readFile(stamp, "utf8")).trim() === today) return;
		} catch {
			// Never pruned here, or the stamp can't be read: prune.
		}
		await this.prune();
		if (this.stopped) return;
		try {
			await writeFile(stamp, `${today}\n`);
		} catch (error) {
			// No checks folder yet means nothing was stored to prune.
			if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
				log("warn", "checks", `couldn't note today's prune: ${(error as Error).message}`);
			}
		}
	}

	/**
	 * Removes the result folders untouched for 14 days (the newest mtime of the folder and its files) and `tmp`
	 * entries older than an hour; `hits.log` stays (H8, K11). One log line when something was removed (0582 PERF-11).
	 * Never throws: a failure is logged.
	 */
	async prune(now = Date.now()): Promise<void> {
		let names: string[];
		try {
			names = await readdir(this.options.dir);
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
				log("warn", "checks", `couldn't prune stored results: ${(error as Error).message}`);
			}
			return;
		}
		let removed = 0;
		let bytes = 0;
		for (const name of names) {
			if (this.stopped) return;
			if (!RESULT_FOLDER.test(name)) continue;
			const folder = join(this.options.dir, name);
			try {
				const age = await folderAge(folder);
				if (!age || now - age.newest < RESULT_DAYS * DAY_MS) continue;
				await removeTreeAsync(folder);
				removed++;
				bytes += age.bytes;
			} catch (error) {
				log("warn", "checks", `couldn't remove stored results ${folder}: ${(error as Error).message}`);
			}
		}
		await this.pruneTmp(now);
		if (removed > 0) {
			log("info", "checks", `removed ${removed} stored results older than ${RESULT_DAYS} days (${sizeSaid(bytes)})`);
		}
	}

	/** Removes `tmp` entries (index copies a crash left) older than an hour. */
	private async pruneTmp(now: number): Promise<void> {
		const tmpDir = join(this.options.dir, "tmp");
		let names: string[];
		try {
			names = await readdir(tmpDir);
		} catch {
			return;
		}
		for (const name of names) {
			const entry = join(tmpDir, name);
			try {
				if (now - (await stat(entry)).mtimeMs >= TMP_MAX_AGE_MS) await removeTreeAsync(entry);
			} catch (error) {
				if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
					log("warn", "checks", `couldn't remove ${entry}: ${(error as Error).message}`);
				}
			}
		}
	}

	/**
	 * The tree git would commit from `cwd` as it is now: tracked and untracked files not ignored, edits included,
	 * written through a copy of the worktree's index so the real one is never touched. Undefined outside git, or when
	 * git fails or runs past its time.
	 */
	async treeKey(cwd: string): Promise<string | undefined> {
		const indexPath = await gitAsync(cwd, ["rev-parse", "--git-path", "index"], GIT_TIMEOUT_MS);
		if (indexPath.code !== 0) return undefined;
		const index = indexPath.stdout.trim();
		const real = isAbsolute(index) ? index : join(cwd, index);
		const tmpDir = join(this.options.dir, "tmp");
		mkdirSync(tmpDir, { recursive: true });
		const temp = join(tmpDir, `index-${process.pid}-${crypto.randomUUID().slice(0, 8)}`);
		try {
			if (existsSync(real)) copyFileSync(real, temp);
			const env = { ...process.env, GIT_INDEX_FILE: temp };
			const add = await gitAsync(cwd, ["add", "-A"], GIT_TIMEOUT_MS, { env });
			if (add.code !== 0) {
				log("warn", "checks", `git add -A for the tree key failed in ${cwd}: ${add.stderr || `exit ${add.code}`}`);
				return undefined;
			}
			const write = await gitAsync(cwd, ["write-tree"], GIT_TIMEOUT_MS, { env });
			const tree = write.stdout.trim();
			return write.code === 0 && /^[0-9a-f]{40,64}$/.test(tree) ? tree : undefined;
		} finally {
			rmSync(temp, { force: true });
		}
	}

	/**
	 * Runs `specs` on `cwd`'s tree with `env`, each answered from its stored result when the key has one; undefined
	 * outside git or without specs. `tree` is the caller's own {@link treeKey} of `cwd`, when it has one. The specs run
	 * one after another: a project's tests and a `done_when` in one worktree may share ports, temp and build files,
	 * and a clash would be stored as the tree's FAIL (0559).
	 */
	async run(
		cwd: string,
		specs: CheckSpec[],
		env: Record<string, string | undefined>,
		tree?: string,
	): Promise<CheckRun | undefined> {
		if (specs.length === 0) return undefined;
		const key = tree ?? (await this.treeKey(cwd));
		if (!key) return undefined;
		const versions = await this.options.versions(cwd, env);
		const results: CheckResult[] = [];
		for (const spec of specs) results.push(await this.one(cwd, key, versions, spec, env));
		return { tree: key, results, passed: results.every((r) => r.gates.every((g) => g.pass)) };
	}

	private one(
		cwd: string,
		tree: string,
		versions: string,
		spec: CheckSpec,
		env: Record<string, string | undefined>,
	): Promise<CheckResult> {
		const key = `${tree}\u0000${spec.name}\u0000${spec.command}\u0000${versions}`;
		const running = this.inFlight.get(key);
		if (running) {
			this.hit();
			return running.then((result) => ({ ...result, how: "joined" as const }));
		}
		const started = this.keyed(cwd, tree, versions, spec, env).finally(() => this.inFlight.delete(key));
		this.inFlight.set(key, started);
		return started;
	}

	private hit(): void {
		this.hits++;
		try {
			mkdirSync(this.options.dir, { recursive: true });
			appendFileSync(join(this.options.dir, "hits.log"), `${Date.now()}\n`);
		} catch (error) {
			log("warn", "checks", `couldn't count a shared hit: ${(error as Error).message}`);
		}
	}

	private stored(file: string, spec: CheckSpec, versions: string): Stored | undefined {
		const value = readJsonObject(file);
		if (!value || value.command !== spec.command || value.versions !== versions) return undefined;
		if (typeof value.exit !== "number") return undefined;
		return value as unknown as Stored;
	}

	/** A stored result as this caller's; its gates as stored, or read from its log when stored without them. */
	private answer(cwd: string, folder: string, spec: CheckSpec, stored: Stored, how: CheckResult["how"]): CheckResult {
		const logFile = join(folder, `${fileName(spec.name)}.log`);
		const gates = Array.isArray(stored.gates)
			? stored.gates
			: gatesOf(spec, stored.exit, existsSync(logFile) ? readFileSync(logFile, "utf8") : "", logFile, cwd);
		return { spec, exit: stored.exit, log: logFile, ms: stored.ms, how, gates };
	}

	private async keyed(
		cwd: string,
		tree: string,
		versions: string,
		spec: CheckSpec,
		env: Record<string, string | undefined>,
	): Promise<CheckResult> {
		const folder = join(this.options.dir, tree.slice(0, 12));
		mkdirSync(folder, { recursive: true });
		const resultFile = join(folder, `${fileName(spec.name)}.json`);
		const lockFile = join(folder, `${fileName(spec.name)}.lock`);
		for (;;) {
			const done = this.stored(resultFile, spec, versions);
			if (done) {
				this.hit();
				return this.answer(cwd, folder, spec, done, "hit");
			}
			if (this.stopped) throw new Error("the check runner has stopped");
			let fd: number | undefined;
			try {
				fd = openSync(lockFile, "wx");
			} catch {
				// Someone runs this key: wait for its result, or take the lock over once its process is gone.
				const held = readText(lockFile);
				if (held !== undefined && holderGone(held)) {
					const pid = lockHolder(held).pid;
					log("info", "checks", `taking over ${spec.name}'s lock on tree ${shortTree(tree)}: process ${pid} is gone`);
					takeOver(lockFile, held);
					continue;
				}
				const waited = await this.waitFor(resultFile, lockFile, spec, versions);
				if (waited.stored) {
					this.hit();
					return this.answer(cwd, folder, spec, waited.stored, "joined");
				}
				if (waited.overdue !== undefined) {
					const pid = lockHolder(waited.overdue).pid;
					log(
						"info",
						"checks",
						`taking over ${spec.name}'s lock on tree ${shortTree(tree)}: process ${pid} held it past the check's time`,
					);
					takeOver(lockFile, waited.overdue);
				}
				continue;
			}
			const own = lockToken();
			try {
				writeFileSync(fd, own);
				closeSync(fd);
				return await this.execute(cwd, folder, spec, versions, env);
			} finally {
				// Only our own lock: one taken over past its time is another runner's now.
				if (readText(lockFile) === own) rmSync(lockFile, { force: true });
			}
		}
	}

	/**
	 * Waits while the lock is held by a live process. `stored` once the result is there (or the lock went without
	 * one: undefined); `overdue` with the lock's text once it was held past the check's time plus a grace; neither
	 * once the holder is gone or this runner stopped.
	 */
	private async waitFor(
		resultFile: string,
		lockFile: string,
		spec: CheckSpec,
		versions: string,
	): Promise<{ stored?: Stored; overdue?: string }> {
		const deadline = Date.now() + this.options.timeoutMs + LOCK_GRACE_MS;
		for (;;) {
			const done = this.stored(resultFile, spec, versions);
			if (done) return { stored: done };
			if (this.stopped) return {};
			const held = readText(lockFile);
			if (held === undefined) return { stored: this.stored(resultFile, spec, versions) };
			if (holderGone(held)) return {};
			if (Date.now() >= deadline) return { overdue: held };
			await Bun.sleep(this.options.pollMs);
		}
	}

	private async execute(
		cwd: string,
		folder: string,
		spec: CheckSpec,
		versions: string,
		env: Record<string, string | undefined>,
	): Promise<CheckResult> {
		this.runs++;
		if (this.stopped) throw new Error("the check runner has stopped");
		const logFile = join(folder, `${fileName(spec.name)}.log`);
		const out = openSync(logFile, "w");
		const started = Date.now();
		let exit: number | null = null;
		let timedOut = false;
		try {
			const { argv, verbatim } = shellArgv(spec.command);
			const proc = Bun.spawn(argv, {
				cwd,
				env: { ...env, NO_COLOR: "1", CLICOLOR: "0" },
				stdin: "ignore",
				stdout: out,
				stderr: out,
				windowsHide: true,
				windowsVerbatimArguments: verbatim,
			});
			this.children.add(proc.pid);
			const timer = setTimeout(() => {
				timedOut = true;
				void killTree(proc.pid);
			}, this.options.timeoutMs);
			try {
				exit = await proc.exited;
			} finally {
				clearTimeout(timer);
				this.children.delete(proc.pid);
			}
			// Ended by `stop`: not the check's own result.
			if (this.stopped) timedOut = true;
		} catch (error) {
			writeFileSync(out, `\njocasta: couldn't run \`${spec.command}\`: ${(error as Error).message}\n`);
		} finally {
			closeSync(out);
		}
		const ms = Date.now() - started;
		if (timedOut) {
			const why = this.stopped ? "Jocasta stopped" : `${Math.round(this.options.timeoutMs / 60_000)} minutes`;
			appendFileSync(logFile, `\njocasta: killed (${why})\n`);
			exit = null;
		}
		const text = readFileSync(logFile, "utf8");
		if (spec.mise) {
			for (const task of miseTasks(text)) {
				if (task.failed && task.name !== spec.name)
					writeFileSync(join(folder, `${fileName(task.name)}.log`), taskLines(text, task.name));
			}
		}
		const gates = gatesOf(spec, exit, text, logFile, cwd);
		if (exit !== null) {
			// The gates go with the result, so a hit answers without reading the log again.
			const stored: Stored = { command: spec.command, versions, exit, ms, at: Date.now(), gates };
			writeAtomically(join(folder, `${fileName(spec.name)}.json`), `${JSON.stringify(stored)}\n`);
		}
		return { spec, exit, log: logFile, ms, how: "ran", gates };
	}
}

/** This process's lock text: its pid and start time (`processStart`), so a reused pid doesn't pass for it. */
function lockToken(): string {
	const start = processStart(process.pid);
	return start === undefined ? String(process.pid) : `${process.pid} ${start}`;
}

/** The holder a lock's text names: `<pid>` or `<pid> <start>`. */
function lockHolder(text: string): { pid: number; start?: number } {
	const [pid, start] = text.trim().split(/\s+/);
	const started = start === undefined ? Number.NaN : Number(start);
	return { pid: Number.parseInt(pid ?? "", 10), ...(Number.isFinite(started) ? { start: started } : {}) };
}

/**
 * Whether a lock's holder is gone: its process isn't running, or the pid now belongs to a process that started at
 * another time. A lock still being written (no pid yet) has a holder.
 */
function holderGone(text: string): boolean {
	const holder = lockHolder(text);
	if (!Number.isInteger(holder.pid) || holder.pid <= 0) return false;
	if (!pidAlive(holder.pid)) return true;
	return holder.start !== undefined && processStart(holder.pid) !== holder.start;
}

/**
 * Takes a stale lock over: renames it aside (only one of two takers can) and removes it. When what was renamed isn't
 * the lock judged stale (another taker's new lock in between), it is put back unless a lock is there again.
 */
function takeOver(lockFile: string, stale: string): void {
	const aside = `${lockFile}.${process.pid}-${crypto.randomUUID().slice(0, 8)}.stale`;
	try {
		renameSync(lockFile, aside);
	} catch {
		// Gone already, or another taker had it first: the caller looks again.
		return;
	}
	try {
		if (readText(aside) !== stale) linkSync(aside, lockFile);
	} catch {
		// A new lock is there already.
	} finally {
		rmSync(aside, { force: true });
	}
}

/** A name as a file name: `/`, `\` and `:` (a mise task's `ci:lint`, `test/unit`) become `-`. */
function fileName(name: string): string {
	return name.replaceAll(/[/\\:]/g, "-");
}

/** The newest mtime of `folder` and its files, and their total size; undefined when it isn't a folder. */
async function folderAge(folder: string): Promise<{ newest: number; bytes: number } | undefined> {
	const own = await stat(folder);
	if (!own.isDirectory()) return undefined;
	let newest = own.mtimeMs;
	let bytes = 0;
	for (const name of await readdir(folder)) {
		const entry = await stat(join(folder, name));
		newest = Math.max(newest, entry.mtimeMs);
		bytes += entry.size;
	}
	return { newest, bytes };
}

/** A byte count as `512 B`, `4.2 KB`, `380 MB`: whole numbers from 10 up, one decimal below. */
export function sizeSaid(bytes: number): string {
	const units = ["B", "KB", "MB", "GB"];
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	const said = unit === 0 || value >= 10 ? String(Math.round(value)) : value.toFixed(1);
	return `${said} ${units[unit]}`;
}

function readText(file: string): string | undefined {
	try {
		return readFileSync(file, "utf8");
	} catch {
		return undefined;
	}
}
