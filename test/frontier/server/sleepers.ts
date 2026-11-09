import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pidAlive } from "../../../src/shared/fsx.ts";

/** Waits until `check` holds, at most `ms`: real processes start and end on the OS clock, not a fake one. */
export async function settles(check: () => boolean, ms = 10_000): Promise<boolean> {
	const end = Date.now() + ms;
	while (!check()) {
		if (Date.now() >= end) return false;
		await Bun.sleep(50);
	}
	return true;
}

/** Kills each pid that is still running (a test's cleanup). */
export function killAll(pids: number[]): void {
	for (const pid of pids) {
		try {
			process.kill(pid, "SIGKILL");
		} catch {
			// Gone.
		}
	}
}

/**
 * `killAll`, then waits until none of `pids` runs, so a cleanup can delete the folder they worked in (Windows refuses
 * while a process has it as its working folder).
 */
export async function endAll(pids: number[]): Promise<void> {
	killAll(pids);
	await settles(() => !pids.some(pidAlive));
}

/**
 * The command of a real tree of `bun` sleepers, as a pane's omp with its workers: the root starts a child, which
 * starts a grandchild detached (its own group and session on POSIX, as a language server may). `outlive` starts the
 * child detached too, so both outlive the root, as omp's workers outlive omp (on Windows Bun ends a child started
 * attached with its parent). Each writes its pid to `<folder>/pids/`; `started` lists them.
 */
export function sleeperCommand(folder: string, outlive = false): { command: string[]; started: () => number[] } {
	const pids = join(folder, "pids");
	mkdirSync(pids, { recursive: true });
	const sleeper = join(folder, "sleeper.ts");
	writeFileSync(
		sleeper,
		[
			'import { writeFileSync } from "node:fs";',
			'import { join } from "node:path";',
			"const depth = Number(process.argv[2]);",
			`writeFileSync(join(${JSON.stringify(pids)}, String(process.pid)), String(depth));`,
			"if (depth < 2) {",
			"\tconst child = Bun.spawn([process.execPath, import.meta.path, String(depth + 1)], {",
			'\t\tstdio: ["ignore", "ignore", "ignore"],',
			`\t\tdetached: depth === 1 || ${outlive},`,
			"\t});",
			"\tchild.unref();",
			"}",
			"setInterval(() => {}, 1000);",
			"",
		].join("\n"),
	);
	return { command: [process.execPath, sleeper, "0"], started: () => readdirSync(pids).map(Number) };
}

export interface SleeperTree {
	root: number;
	/** Every pid of the tree, the root included. */
	all: number[];
	/** Kills whatever of the tree is left and waits until it is gone (a test's cleanup). */
	stop(): Promise<void>;
}

/** A sleeper tree (`sleeperCommand`) this process starts, working in `folder`, once all three run. */
export async function sleeperTree(folder: string, outlive = false): Promise<SleeperTree> {
	const { command, started } = sleeperCommand(folder, outlive);
	const root = Bun.spawn(command, { cwd: folder, stdio: ["ignore", "ignore", "ignore"] });
	if (!(await settles(() => started().length === 3))) {
		killAll([root.pid, ...started()]);
		throw new Error("the sleeper tree didn't start");
	}
	const all = started();
	return { root: root.pid, all, stop: () => endAll(all) };
}

export interface ShimTree {
	/** The process herdr would start for the pane (on Windows Bun's `omp.exe` shim). */
	shim: number;
	/** The real omp, the shim's child, once it runs. */
	omp(): number | undefined;
	/** With `ompLater`: the shim starts omp now; resolves once omp runs. */
	startOmp(): Promise<void>;
	/** The workers omp started on its way out, once it did. */
	workers(): number[];
	/** omp starts two workers, waits until they run, then exits, as omp does when its pane closes. */
	exit(): void;
	/** Kills whatever of the tree is left and waits until it is gone (a test's cleanup). */
	stop(): Promise<void>;
}

/**
 * A real tree like a pane's omp on Windows (0490): a shim whose child is the real omp, each started detached so either
 * outlives the other. omp starts its workers (detached) only on `exit()`, then exits. `ompLater`: the shim starts omp
 * only on `startOmp()`, as a shim whose omp isn't up yet. Works in `folder`.
 */
export async function shimTree(folder: string, ompLater = false): Promise<ShimTree> {
	const pids = join(folder, "pids");
	const exitFile = join(folder, "exit");
	const goFile = join(folder, "go");
	mkdirSync(pids, { recursive: true });
	if (!ompLater) writeFileSync(goFile, "");
	const script = join(folder, "shim.ts");
	writeFileSync(
		script,
		[
			'import { existsSync, readdirSync, writeFileSync } from "node:fs";',
			'import { join } from "node:path";',
			`const pids = ${JSON.stringify(pids)};`,
			"const role = process.argv[2];",
			'writeFileSync(join(pids, role + "-" + process.pid), "");',
			"const start = (r) =>",
			'\tBun.spawn([process.execPath, import.meta.path, r], { stdio: ["ignore", "ignore", "ignore"], detached: true }).unref();',
			'if (role === "shim") {',
			"\tconst go = setInterval(() => {",
			`\t\tif (!existsSync(${JSON.stringify(goFile)})) return;`,
			"\t\tclearInterval(go);",
			'\t\tstart("omp");',
			"\t}, 20);",
			"}",
			'if (role === "omp") {',
			"\tconst poll = setInterval(() => {",
			`\t\tif (!existsSync(${JSON.stringify(exitFile)})) return;`,
			"\t\tclearInterval(poll);",
			'\t\tstart("worker");',
			'\t\tstart("worker");',
			"\t\tsetInterval(() => {",
			'\t\t\tif (readdirSync(pids).filter((n) => n.startsWith("worker-")).length >= 2) process.exit(0);',
			"\t\t}, 20);",
			"\t}, 20);",
			"}",
			"setInterval(() => {}, 1000);",
			"",
		].join("\n"),
	);
	const of = (role: string) =>
		readdirSync(pids)
			.filter((name) => name.startsWith(`${role}-`))
			.map((name) => Number(name.slice(role.length + 1)));
	const root = Bun.spawn([process.execPath, script, "shim"], { cwd: folder, stdio: ["ignore", "ignore", "ignore"] });
	const all = () => [...of("shim"), ...of("omp"), ...of("worker")];
	const ompRuns = () => of("omp").length === 1;
	if (!(await settles(() => of("shim").length === 1 && (ompLater || ompRuns())))) {
		killAll([root.pid, ...all()]);
		throw new Error("the shim tree didn't start");
	}
	return {
		shim: root.pid,
		omp: () => of("omp")[0],
		startOmp: async () => {
			if (!existsSync(goFile)) writeFileSync(goFile, "");
			if (!(await settles(ompRuns))) throw new Error("the shim's omp didn't start");
		},
		workers: () => of("worker"),
		exit: () => {
			if (!existsSync(exitFile)) writeFileSync(exitFile, "");
		},
		stop: () => endAll(all()),
	};
}
