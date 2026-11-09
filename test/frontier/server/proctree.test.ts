import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
	descendants,
	endOmpLeftovers,
	endProcesses,
	killTree,
	leftovers,
	leftoversOf,
	ompProcesses,
	type ProcessEntry,
	processesIn,
	processIdOf,
	processStart,
	processTree,
} from "../../../src/frontier/server/proctree.ts";
import { pidAlive } from "../../../src/shared/fsx.ts";
import { type SleeperTree, settles, shimTree, sleeperTree } from "./sleepers.ts";

const cleanup: (() => void | Promise<void>)[] = [];
afterEach(async () => {
	for (const fn of cleanup.splice(0).reverse()) await fn();
});

/** A temp dir with a sleeper tree working in its `wt` folder (`outlive`: its workers outlive the root). */
async function treeIn(outlive = false): Promise<{ dir: string; folder: string; tree: SleeperTree }> {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-proctree-"));
	cleanup.push(() => rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }));
	const folder = join(dir, "wt");
	mkdirSync(folder, { recursive: true });
	const tree = await sleeperTree(folder, outlive);
	cleanup.push(tree.stop);
	return { dir, folder, tree };
}

test("descendants walks a parent map to every level, ignoring a pid that is its own parent and never entering skip", () => {
	const parents = new Map([
		[10, 1],
		[11, 10],
		[12, 11],
		[13, 11],
		[14, 13],
		[20, 1],
		[0, 0],
	]);
	expect(descendants(10, parents)).toEqual([11, 12, 13, 14]);
	expect(descendants(10, parents, new Set([13]))).toEqual([11, 12]);
	expect(descendants(20, parents)).toEqual([]);
});

/** A snapshot from `[pid, parent, start]` rows. */
const snapshot = (rows: [number, number, number | undefined][]) =>
	new Map(rows.map(([pid, parent, start]): [number, ProcessEntry] => [pid, { parent, start }]));

test("leftovers of a root that ended: its orphans and their children, never one started before it", () => {
	const processes = snapshot([
		[11, 10, 105], // the dead root's child (a worker)
		[12, 11, 110], // its grandchild
		[13, 10, 90], // started before the root: an earlier process of that pid had it
		[14, 13, 120], // under that one
		[20, 1, 50], // unrelated
	]);
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([11, 12]);
	// A grandchild started before its own parent hangs under an earlier run of that parent's pid.
	processes.set(15, { parent: 11, start: 104 });
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([11, 12]);
	// A process whose start can't be read is left out with what is under it; the walk never enters skip.
	processes.set(11, { parent: 10, start: undefined });
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([]);
	processes.set(11, { parent: 10, start: 105 });
	expect(leftovers({ pid: 10, start: 100 }, processes, new Set([11]))).toEqual([]);
	expect(leftovers({ pid: 10, start: 100 }, processes, new Set([10]))).toEqual([]);
});

test("leftovers of a root still running are the root and its tree", () => {
	const processes = snapshot([
		[10, 1, 100],
		[11, 10, 100],
		[12, 11, 130],
	]);
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([10, 11, 12]);
});

test("a reused pid never counts: the process that has it now and what it started are left out", () => {
	const processes = snapshot([
		[10, 1, 200], // the pid, given to another process after the root ended
		[11, 10, 150], // the dead root's orphan
		[12, 11, 160],
		[15, 10, 210], // the new process's child
		[16, 15, 220],
	]);
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([11, 12]);
	// The new process's start unknown: its children can't be told from the root's, so nothing counts.
	processes.set(10, { parent: 1, start: undefined });
	expect(leftovers({ pid: 10, start: 100 }, processes)).toEqual([]);
});

test("processesIn finds every process of a real tree working in a folder; killTree ends them all", async () => {
	const { dir, folder, tree } = await treeIn();
	expect(tree.all).toHaveLength(3);
	expect(tree.all).toContain(tree.root);

	const inUse = processesIn(folder);
	for (const pid of tree.all) expect(inUse.map((p) => p.pid)).toContain(pid);
	expect(inUse.find((p) => p.pid === tree.root)?.name).toMatch(/^bun/);
	// The folder's parent holds it too; a sibling whose name starts the same doesn't.
	expect(processesIn(dir).map((p) => p.pid)).toContain(tree.root);
	expect(processesIn(`${folder}2`)).toEqual([]);

	const ended = await killTree(tree.root);
	expect(ended.toSorted()).toEqual(tree.all.toSorted());
	expect(await settles(() => tree.all.every((pid) => !pidAlive(pid)))).toBe(true);
	expect(await settles(() => processesIn(folder).length === 0)).toBe(true);
}, 30_000);

test("endProcesses ends a listed tree whose root went first", async () => {
	const { folder, tree } = await treeIn();
	const listed = processTree([tree.root]);
	expect(listed.toSorted()).toEqual(tree.all.toSorted());
	process.kill(tree.root, "SIGKILL");
	expect(await settles(() => !pidAlive(tree.root))).toBe(true);
	await endProcesses(listed);
	expect(tree.all.filter(pidAlive)).toEqual([]);
	expect(await settles(() => processesIn(folder).length === 0)).toBe(true);
}, 30_000);

test("a real tree whose root ended: leftoversOf finds the orphans by the root's pid and start, endProcesses ends them", async () => {
	const { folder, tree } = await treeIn(true);
	const start = processStart(tree.root);
	expect(start).toBeNumber();
	const root = { pid: tree.root, start: start as number };
	expect(leftoversOf([root]).toSorted()).toEqual(tree.all.toSorted());
	// Remembered with an earlier start, the pid's process now is another run of it: none of the tree counts.
	expect(leftoversOf([{ pid: tree.root, start: (start as number) / 2 }])).toEqual([]);
	process.kill(tree.root, "SIGKILL");
	expect(await settles(() => !pidAlive(tree.root))).toBe(true);
	expect(processStart(tree.root)).toBeUndefined();
	const orphans = tree.all.filter((pid) => pid !== tree.root);
	expect(leftoversOf([root]).toSorted()).toEqual(orphans.toSorted());
	await endProcesses(leftoversOf([root]));
	expect(tree.all.filter(pidAlive)).toEqual([]);
	expect(await settles(() => processesIn(folder).length === 0)).toBe(true);
	expect(leftoversOf([root])).toEqual([]);
}, 30_000);

test("an omp behind a shim: the workers it starts on its way out, after the last read, end once shim and omp went (0490)", async () => {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-proctree-"));
	cleanup.push(() => rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }));
	const tree = await shimTree(dir);
	cleanup.push(tree.stop);
	const shim = processIdOf(tree.shim);
	expect(shim).toBeDefined();
	const [read] = ompProcesses([shim as NonNullable<typeof shim>]);
	expect(read?.omp.map((id) => id.pid).toSorted()).toEqual([tree.shim, tree.omp() as number].toSorted());
	// herdr closes the pane: the shim goes at once; omp starts its workers, then goes.
	tree.exit();
	process.kill(tree.shim, "SIGKILL");
	expect(await settles(() => !pidAlive(tree.shim))).toBe(true);
	const found = await endOmpLeftovers(read ? [read] : [], 10_000);
	expect(tree.workers()).toHaveLength(2);
	expect(found.toSorted()).toEqual(tree.workers().toSorted());
	expect(tree.workers().filter(pidAlive)).toEqual([]);
	// The shim alone leads nowhere once the real omp is gone.
	expect(leftoversOf([shim as NonNullable<typeof shim>])).toEqual([]);
}, 30_000);

test.if(process.platform === "win32")(
	"processesIn finds a process whose working folder is the 8.3 short name of the folder asked about",
	async () => {
		// A short %TEMP% (`C:\Users\ZHIVKO~1\…`) gives children a short working folder; seen on 2026-10-24 when two omp
		// workers held a folder that processesIn, asked with the long name, didn't see.
		const dir = mkdtempSync(join(tmpdir(), "jocasta-proctree-"));
		cleanup.push(() => rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }));
		const folder = join(dir, "A Long Folder Name");
		mkdirSync(folder);
		const fso = `(New-Object -ComObject Scripting.FileSystemObject).GetFolder('${folder}').ShortPath`;
		const short = Bun.spawnSync(["powershell", "-NoProfile", "-Command", fso]).stdout.toString().trim();
		if (short === "" || short.toLowerCase() === folder.toLowerCase()) return; // This volume makes no 8.3 names.
		expect(short).toContain("~");
		const tree = await sleeperTree(short);
		cleanup.push(tree.stop);
		expect(processesIn(folder).map((p) => p.pid)).toContain(tree.root);
	},
	30_000,
);

test("a process that is gone, or this process, is never in a tree and ends nothing", async () => {
	const gone = Bun.spawn([process.execPath, "-e", "0"], { stdio: ["ignore", "ignore", "ignore"] });
	await gone.exited;
	expect(await killTree(gone.pid)).toEqual([]);
	expect(processTree([process.pid, process.ppid])).toEqual([]);
	expect(await killTree(process.pid)).toEqual([]);
	expect(pidAlive(process.pid)).toBe(true);
});
