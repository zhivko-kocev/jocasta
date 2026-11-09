/**
 * Processes (0486): whether one lives and when it started, ending a whole process tree, and finding the processes
 * whose working folder is inside a folder before Jocasta removes it. omp's workers (the memory embedding worker,
 * language servers, MCP servers) otherwise outlive their pane and keep its worktree locked. A pane he closed in herdr
 * has its leftovers found through its omp's remembered process id and start time (0490). One kernel32 binding serves
 * every Windows read.
 */
import { dlopen, FFIType, type Pointer, ptr } from "bun:ffi";
import { readdirSync, readFileSync, readlinkSync } from "node:fs";
import { isInside, realPath } from "./pathkey.ts";
import { waitFor } from "./poll.ts";

/** A process whose working folder is inside the folder asked about. */
export interface FolderProcess {
	pid: number;
	/** Its executable's file name (`bun.exe`, `node`), or `?` when unreadable. */
	name: string;
	cwd: string;
}

/**
 * A process as one run of it: its pid and when it started (`processStart`). The start compares only with other starts
 * read on this machine; a pid the OS gave again later has a later start.
 */
export interface ProcessId {
	pid: number;
	start: number;
}

/** A process in one snapshot of the process list: its parent pid, and its start when it could be read. */
export interface ProcessEntry {
	parent: number;
	start: number | undefined;
}

/** How long ended processes may take to go before `endProcesses` stops waiting. */
const KILL_WAIT_MS = 5_000;

/** This process and its ancestors in `parents`: never in a tree, and never walked through. */
function ownLine(parents: Map<number, number>): Set<number> {
	const own = new Set([process.pid]);
	for (let up = parents.get(process.pid); up !== undefined && up > 0 && !own.has(up); up = parents.get(up)) own.add(up);
	return own;
}

/** Whether process `pid` is alive (signal 0 only checks). */
export function pidAlive(pid: number): boolean {
	if (!Number.isInteger(pid) || pid <= 0) return false;
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		// EPERM: it exists but belongs to someone else.
		return (error as NodeJS.ErrnoException).code === "EPERM";
	}
}

/** FILETIME's epoch (1601-01-01) in Unix epoch milliseconds. */
const FILETIME_EPOCH_MS = 11_644_473_600_000;

/**
 * When process `pid` started, in epoch ms (Windows, from `processStart`'s FILETIME); undefined elsewhere or when it
 * isn't running or can't be read. Compared with a file's time, it tells a reused pid from the process that wrote it
 * (LC-4).
 */
export function processStartedAt(pid: number): number | undefined {
	if (process.platform !== "win32") return undefined;
	const start = processStart(pid);
	return start === undefined ? undefined : Math.floor(start / 10_000) - FILETIME_EPOCH_MS;
}

/**
 * When `pid` started, or undefined when it isn't running or can't be read. Windows: `GetProcessTimes`' creation time.
 * Linux: `/proc/<pid>/stat`'s starttime. macOS: `ps -o lstart=`.
 */
export function processStart(pid: number): number | undefined {
	return processStarts([pid]).get(pid) ?? undefined;
}

/**
 * `processStart` of each of `pids` (one `ps` on macOS): null for one that ended (gone, a zombie, or on Windows ended
 * while a handle keeps it listed), missing when it can't be read (another user's on Windows).
 */
function processStarts(pids: number[]): Map<number, number | null> {
	const starts = new Map<number, number | null>();
	const valid = pids.filter((pid) => Number.isInteger(pid) && pid > 0);
	if (valid.length === 0) return starts;
	if (process.platform === "win32") {
		for (const pid of valid) {
			const start = windows().start(pid);
			if (start !== undefined) starts.set(pid, start);
		}
		return starts;
	}
	if (process.platform === "linux") {
		for (const pid of valid) {
			try {
				// After `comm` (field 2, which may hold spaces): state (field 3), then starttime (field 22, clock ticks).
				const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
				const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
				const start = Number(fields[19]);
				if (fields[0] === "Z" || fields[0] === "X") starts.set(pid, null);
				else if (Number.isFinite(start)) starts.set(pid, start);
			} catch {
				starts.set(pid, null);
			}
		}
		return starts;
	}
	for (const pid of valid) starts.set(pid, null);
	const ps = Bun.spawnSync(["ps", "-o", "pid=,lstart=", "-p", valid.join(",")], {
		stdout: "pipe",
		stderr: "ignore",
		env: { ...process.env, LC_ALL: "C" },
	});
	for (const line of ps.stdout.toString().split("\n")) {
		const match = /^\s*(\d+)\s+(.+?)\s*$/.exec(line);
		if (!match) continue;
		const start = Date.parse(match[2] as string);
		if (Number.isFinite(start)) starts.set(Number(match[1]), start);
		else starts.delete(Number(match[1]));
	}
	return starts;
}

/**
 * The processes left of `root`, a process that may have ended (0490), from one snapshot (`processes`): `root` itself
 * while it is still that same run, and every process under its pid started at or after it, each at or after its own
 * parent. When `root.pid` now belongs to another run, that run and whatever started at or after it are left out, so a
 * reused pid never counts. A process whose start is unknown is left out with what is under it; the walk never enters
 * `skip` (this process and its ancestors).
 */
export function leftovers(root: ProcessId, processes: Map<number, ProcessEntry>, skip = new Set<number>()): number[] {
	if (skip.has(root.pid)) return [];
	const now = processes.get(root.pid);
	const same = now !== undefined && now.start === root.start;
	// The other run holding the pid now: its children can't be told from the root's but by start.
	const other = now !== undefined && !same ? now.start : Number.POSITIVE_INFINITY;
	if (other === undefined) return [];
	const children = new Map<number, number[]>();
	for (const [pid, { parent }] of processes) {
		if (pid === parent) continue;
		const list = children.get(parent);
		if (list) list.push(pid);
		else children.set(parent, [pid]);
	}
	const found = same ? [root.pid] : [];
	const seen = new Set([root.pid]);
	const queue: { pid: number; from: number; before: number }[] = [{ pid: root.pid, from: root.start, before: other }];
	for (let next = queue.shift(); next !== undefined; next = queue.shift()) {
		for (const child of children.get(next.pid) ?? []) {
			const start = processes.get(child)?.start;
			if (seen.has(child) || skip.has(child) || start === undefined) continue;
			if (start < next.from || start >= next.before) continue;
			seen.add(child);
			found.push(child);
			queue.push({ pid: child, from: start, before: Number.POSITIVE_INFINITY });
		}
	}
	return found;
}

/**
 * One snapshot of the running processes at and under `roots`' pids, each with its start (an ended one left out), this
 * process with its ancestors (never in a tree), every process's parent pid (a root's orphaned group member under the
 * root), and on POSIX each one's process group.
 */
function snapshotUnder(roots: ProcessId[]): {
	processes: Map<number, ProcessEntry>;
	own: Set<number>;
	parents: Map<number, number>;
	groups: Map<number, number>;
} {
	const groups = new Map<number, number>();
	const parents = parentPids(groups);
	const own = ownLine(parents);
	// POSIX hands a process whose parent ended to init or a subreaper, so its parent pid no longer leads back. One still
	// in a root's process group is that root's: the pane's root leads its own group, and a child stays in it unless it
	// starts one of its own (0486, 0490). Windows keeps the parent's pid and has no groups here.
	for (const root of roots) {
		for (const [pid, group] of groups) {
			if (group !== root.pid || pid === root.pid || own.has(pid)) continue;
			if (groups.get(parents.get(pid) ?? 0) !== root.pid) parents.set(pid, root.pid);
		}
	}
	// The candidates by raw parent pid; `leftovers` then keeps only the edges whose starts agree.
	const near = new Set<number>();
	for (const root of roots) {
		if (own.has(root.pid)) continue;
		if (parents.has(root.pid)) near.add(root.pid);
		for (const pid of descendants(root.pid, parents, own)) near.add(pid);
	}
	const starts = processStarts([...near]);
	const processes = new Map<number, ProcessEntry>();
	for (const pid of near) {
		const start = starts.get(pid);
		if (start !== null) processes.set(pid, { parent: parents.get(pid) as number, start });
	}
	return { processes, own, parents, groups };
}

/** `leftovers` of all of `roots` together, from one snapshot of the running processes. */
export function leftoversOf(roots: ProcessId[]): number[] {
	const { processes, own } = snapshotUnder(roots);
	return [...new Set(roots.flatMap((root) => leftovers(root, processes, own)))];
}

/** `pid` as the run it is now, or undefined when it isn't running or can't be read. */
export function processIdOf(pid: number): ProcessId | undefined {
	const start = processStart(pid);
	return start === undefined ? undefined : { pid, start };
}

/**
 * A thread pane's omp as Jocasta remembers it (0490), each process by pid and start: `omp` the process herdr started
 * for the pane (on Windows Bun's `omp.exe` shim) and its children (the real omp), `tree` those and everything that ran
 * under them when read.
 */
export interface OmpProcesses {
	omp: ProcessId[];
	tree: ProcessId[];
}

/** What runs now at and under each of `roots` (its `leftovers`), from one snapshot. */
export function ompProcesses(roots: ProcessId[]): OmpProcesses[] {
	const { processes, own } = snapshotUnder(roots);
	return roots.map((root) => {
		const tree = leftovers(root, processes, own).map((pid) => ({ pid, start: processes.get(pid)?.start as number }));
		const omp = tree.filter(({ pid }) => pid === root.pid || processes.get(pid)?.parent === root.pid);
		return { omp, tree };
	});
}

/** Both remembered reads of one pane's omp in one, each process once. */
export function mergeOmp(a: OmpProcesses, b: OmpProcesses): OmpProcesses {
	const merge = (x: ProcessId[], y: ProcessId[]) => {
		const seen = new Set(x.map((id) => `${id.pid}@${id.start}`));
		return [...x, ...y.filter((id) => !seen.has(`${id.pid}@${id.start}`))];
	};
	return { omp: merge(a.omp, b.omp), tree: merge(a.tree, b.tree) };
}

/** Whether any of `ids` still runs as that same run. */
function anyRunning(ids: ProcessId[]): boolean {
	const starts = processStarts(ids.map((id) => id.pid));
	return ids.some((id) => starts.get(id.pid) === id.start);
}

/**
 * Ends what remembered omps left (0490): waits up to `waitMs` for each `omp` process to end (an omp on its way out
 * starts its memory workers about a second before it goes), then ends `leftovers` of every `tree` process at once,
 * the ones still running included. Resolves the pids found.
 */
export async function endOmpLeftovers(sets: OmpProcesses[], waitMs: number): Promise<number[]> {
	const omp = sets.flatMap((set) => set.omp);
	if (waitMs > 0) await waitFor(() => !anyRunning(omp), { timeoutMs: waitMs, intervalMs: 100 });
	return endProcesses(sets.flatMap((set) => set.tree));
}

/**
 * Ends `roots` while each is still that same run, and every process under them by the start-checked walk of `leftovers`
 * (0490): a process counts only when it started at or after its parent, so a live process that names a long-dead parent
 * whose pid Windows gave again is never reached (SRV-A-1; `taskkill /T` walks raw parent pids, so it isn't used). The
 * walk is taken again until nothing is left or 5 s pass: children started meanwhile, and the orphans of a process that
 * ended first (each process found stays a root by its own pid and start). Each process found gets SIGKILL
 * (TerminateProcess on Windows); on POSIX a found group leader's whole group too, unless a spared process or one under
 * it is in that group. `spare` (a process and what runs under it) is never entered or signalled while it is still that
 * same run: the herdr server a hung service started (0582). Never this process or its ancestors. Resolves the pids
 * found.
 */
export async function endProcesses(roots: ProcessId[], spare: ProcessId[] = []): Promise<number[]> {
	const all = new Map<string, ProcessId>();
	const add = (id: ProcessId) => all.set(`${id.pid}@${id.start}`, id);
	for (const id of roots) if (Number.isInteger(id.pid) && id.pid > 0) add(id);
	if (all.size === 0) return [];
	const spared = spare.filter((id) => processStart(id.pid) === id.start).map((id) => id.pid);
	const found = new Set<number>();
	const end = Date.now() + KILL_WAIT_MS;
	for (;;) {
		const ids = [...all.values()];
		const { processes, own, parents, groups } = snapshotUnder(ids);
		const skip = new Set([...own, ...spared]);
		// The groups a spared process or one under it is in: their leaders are ended by pid alone, so a spared process
		// that shares a found leader's group (a child of it that never started its own) lives on.
		const kept = new Set(
			spared.flatMap((pid) => [pid, ...descendants(pid, parents, own)]).flatMap((pid) => groups.get(pid) ?? []),
		);
		const now = [...new Set(ids.flatMap((root) => leftovers(root, processes, skip)))];
		if (now.length === 0 || Date.now() >= end) return [...found];
		for (const pid of now) {
			found.add(pid);
			add({ pid, start: processes.get(pid)?.start as number });
			if (process.platform !== "win32" && groups.get(pid) === pid && !kept.has(pid)) {
				try {
					process.kill(-pid, "SIGKILL");
				} catch {
					// Gone already.
				}
			}
			try {
				process.kill(pid, "SIGKILL");
			} catch {
				// Gone already.
			}
		}
		await Bun.sleep(50);
	}
}

/** Ends `pid` as it runs now with every process under it (`endProcesses`); resolves the pids found in its tree. */
export async function killTree(pid: number): Promise<number[]> {
	const id = processIdOf(pid);
	return id ? endProcesses([id]) : [];
}

/**
 * Each running process's parent pid, from one snapshot of the process list; on POSIX, `groups` gets each one's process
 * group from the same snapshot.
 */
function parentPids(groups?: Map<number, number>): Map<number, number> {
	const parents = new Map<number, number>();
	if (process.platform === "win32") {
		for (const [pid, ppid] of windows().parents()) parents.set(pid, ppid);
		return parents;
	}
	const ps = Bun.spawnSync(["ps", "-A", "-o", "pid=,ppid=,pgid="], { stdout: "pipe", stderr: "ignore" });
	for (const line of ps.stdout.toString().split("\n")) {
		const [pid, ppid, pgid] = line.trim().split(/\s+/).map(Number);
		if (!pid || ppid === undefined || !Number.isInteger(ppid)) continue;
		parents.set(pid, ppid);
		if (groups && pgid !== undefined && Number.isInteger(pgid)) groups.set(pid, pgid);
	}
	return parents;
}

/** Every process under `root` in `parents` (child → parent), nearest first; the walk never enters `skip`. */
export function descendants(root: number, parents: Map<number, number>, skip = new Set<number>()): number[] {
	const children = new Map<number, number[]>();
	for (const [pid, ppid] of parents) {
		if (pid === ppid) continue;
		const list = children.get(ppid);
		if (list) list.push(pid);
		else children.set(ppid, [pid]);
	}
	const found: number[] = [];
	const seen = new Set([root]);
	const queue = [root];
	for (let next = queue.shift(); next !== undefined; next = queue.shift()) {
		for (const child of children.get(next) ?? []) {
			if (seen.has(child) || skip.has(child)) continue;
			seen.add(child);
			found.push(child);
			queue.push(child);
		}
	}
	return found;
}

/**
 * The processes whose working folder is `folder` or inside it, this process excepted. Windows: each process's current
 * directory is read from its PEB (`NtQueryInformationProcess` + `ReadProcessMemory`); a process this user can't open
 * (another user's, an elevated one) is skipped, and a folder recorded with an 8.3 short name (`C:\Users\ZHIVKO~1\…`,
 * what a short `%TEMP%` gives a child) is expanded to its long one before comparing. Linux: `/proc/<pid>/cwd`. macOS:
 * `lsof -d cwd`.
 */
export function processesIn(folder: string): FolderProcess[] {
	const roots = [...new Set([folder, realPath(folder)])];
	const inside = (cwd: string) => {
		const at = process.platform === "win32" && cwd.includes("~") ? realPath(cwd) : cwd;
		return roots.some((root) => isInside(root, at));
	};
	return listCwds().filter((p) => p.pid !== process.pid && inside(p.cwd));
}

function listCwds(): FolderProcess[] {
	if (process.platform === "win32") return windows().cwds();
	if (process.platform === "linux") {
		const found: FolderProcess[] = [];
		for (const entry of readdirSync("/proc")) {
			const pid = Number(entry);
			if (!Number.isInteger(pid) || pid <= 0) continue;
			try {
				const cwd = readlinkSync(`/proc/${pid}/cwd`);
				let name = "?";
				try {
					name = readFileSync(`/proc/${pid}/comm`, "utf8").trim() || "?";
				} catch {
					// Gone meanwhile.
				}
				found.push({ pid, name, cwd });
			} catch {
				// Another user's process, or gone.
			}
		}
		return found;
	}
	// macOS (and other BSDs): lsof's field output, `p<pid>`, `c<command>`, `n<path>` per process.
	const lsof = Bun.spawnSync(["lsof", "-w", "-d", "cwd", "-F", "pcn"], { stdout: "pipe", stderr: "ignore" });
	const found: FolderProcess[] = [];
	let current: { pid: number; name: string } | undefined;
	for (const line of lsof.stdout.toString().split("\n")) {
		const field = line.slice(1);
		if (line.startsWith("p")) current = { pid: Number(field), name: "?" };
		else if (line.startsWith("c") && current) current.name = field;
		else if (line.startsWith("n") && current) found.push({ ...current, cwd: field });
	}
	return found;
}

interface WindowsProcesses {
	parents(): [number, number][];
	cwds(): FolderProcess[];
	/**
	 * A process's creation time (FILETIME: 100 ns units since 1601); null when it ended, undefined when it can't be
	 * opened.
	 */
	start(pid: number): number | null | undefined;
}

let win: WindowsProcesses | undefined;

/** The Windows process list through kernel32 and ntdll, opened on first use. */
function windows(): WindowsProcesses {
	win ??= openWindows();
	return win;
}

function openWindows(): WindowsProcesses {
	const k32 = dlopen("kernel32.dll", {
		CreateToolhelp32Snapshot: { args: [FFIType.u32, FFIType.u32], returns: FFIType.ptr },
		Process32FirstW: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
		Process32NextW: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
		OpenProcess: { args: [FFIType.u32, FFIType.i32, FFIType.u32], returns: FFIType.ptr },
		CloseHandle: { args: [FFIType.ptr], returns: FFIType.i32 },
		GetExitCodeProcess: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
		GetProcessTimes: {
			args: [FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr],
			returns: FFIType.i32,
		},
		ReadProcessMemory: {
			args: [FFIType.ptr, FFIType.u64, FFIType.ptr, FFIType.u64, FFIType.ptr],
			returns: FFIType.i32,
		},
	}).symbols;
	const nt = dlopen("ntdll.dll", {
		NtQueryInformationProcess: {
			args: [FFIType.ptr, FFIType.u32, FFIType.ptr, FFIType.u32, FFIType.ptr],
			returns: FFIType.i32,
		},
	}).symbols;
	const TH32CS_SNAPPROCESS = 0x2;
	const INVALID_HANDLE = 0xffffffffffffffffn;
	const PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
	const STILL_ACTIVE = 259;
	const PROCESS_VM_READ = 0x10;
	// PROCESSENTRY32W (x64): dwSize @0, th32ProcessID @8, th32ParentProcessID @32, szExeFile (WCHAR[260]) @44; 568 bytes.
	const ENTRY_SIZE = 568;

	/** Every process as `[pid, parent pid, exe file name]`, from one Toolhelp snapshot. */
	const snapshot = (): [number, number, string][] => {
		const snap = k32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
		if (snap === null || BigInt(snap) === INVALID_HANDLE) return [];
		const out: [number, number, string][] = [];
		try {
			const entry = new Uint8Array(ENTRY_SIZE);
			const view = new DataView(entry.buffer);
			view.setUint32(0, ENTRY_SIZE, true);
			for (let ok = k32.Process32FirstW(snap, ptr(entry)); ok !== 0; ok = k32.Process32NextW(snap, ptr(entry))) {
				const name = new Uint16Array(entry.buffer, 44, 260);
				const end = name.indexOf(0);
				out.push([
					view.getUint32(8, true),
					view.getUint32(32, true),
					String.fromCharCode(...name.subarray(0, end < 0 ? 260 : end)),
				]);
			}
		} finally {
			k32.CloseHandle(snap);
		}
		return out;
	};

	/** A process handle as bun:ffi returns it. */
	type Handle = Pointer | bigint;
	const read = (handle: Handle, address: bigint, size: number): Uint8Array | undefined => {
		const buffer = new Uint8Array(size);
		return k32.ReadProcessMemory(handle, address, ptr(buffer), BigInt(size), null) === 0 ? undefined : buffer;
	};

	/** A process's current directory from its PEB (x64 layouts), or undefined when it can't be read. */
	const cwdOf = (pid: number): string | undefined => {
		const handle = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
		if (handle === null) return undefined;
		try {
			// PROCESS_BASIC_INFORMATION (48 bytes): PebBaseAddress @8.
			const basic = new Uint8Array(48);
			if (nt.NtQueryInformationProcess(handle, 0, ptr(basic), 48, null) !== 0) return undefined;
			const peb = new DataView(basic.buffer).getBigUint64(8, true);
			if (peb === 0n) return undefined;
			// PEB.ProcessParameters @0x20; RTL_USER_PROCESS_PARAMETERS.CurrentDirectory.DosPath (UNICODE_STRING) @0x38.
			const params = read(handle, peb + 0x20n, 8);
			if (!params) return undefined;
			const at = new DataView(params.buffer).getBigUint64(0, true);
			const dosPath = read(handle, at + 0x38n, 16);
			if (!dosPath) return undefined;
			const dos = new DataView(dosPath.buffer);
			const length = dos.getUint16(0, true);
			const buffer = dos.getBigUint64(8, true);
			if (length === 0 || buffer === 0n) return undefined;
			const text = read(handle, buffer, length);
			return text ? String.fromCharCode(...new Uint16Array(text.buffer, 0, length / 2)) : undefined;
		} finally {
			k32.CloseHandle(handle);
		}
	};

	return {
		parents: () => snapshot().map(([pid, ppid]) => [pid, ppid]),
		cwds: () => {
			const found: FolderProcess[] = [];
			for (const [pid, , name] of snapshot()) {
				if (pid === 0 || pid === 4) continue;
				const cwd = cwdOf(pid);
				if (cwd) found.push({ pid, name: name || "?", cwd });
			}
			return found;
		},
		start: (pid) => {
			const handle = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
			if (handle === null) return undefined;
			try {
				// Ended: an exit code other than STILL_ACTIVE, the test `pidAlive` (libuv) uses, which TerminateProcess sets
				// before the exit time appears; or an exit time (an open handle to an ended process still opens).
				const code = new Uint32Array(1);
				if (k32.GetExitCodeProcess(handle, ptr(code)) !== 0 && code[0] !== STILL_ACTIVE) return null;
				// Creation, exit, kernel and user time: four FILETIMEs.
				const times = new BigUint64Array(4);
				if (k32.GetProcessTimes(handle, ptr(times), ptr(times, 8), ptr(times, 16), ptr(times, 24)) === 0) {
					return undefined;
				}
				if (times[1]) return null;
				return times[0] ? Number(times[0]) : undefined;
			} finally {
				k32.CloseHandle(handle);
			}
		},
	};
}
