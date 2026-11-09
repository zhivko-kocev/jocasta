import { spawnSync } from "node:child_process";
import { join } from "node:path";

/**
 * Process trees for H3, H5 and H6: one snapshot of every process (pid, parent, name, command line, working set and
 * private bytes), a tree under a root, and the tree's memory. Windows reads `Win32_Process` through PowerShell's CIM;
 * POSIX reads `ps` (RSS only; private bytes are unknown there).
 */

export interface ProcessInfo {
	pid: number;
	ppid: number;
	name: string;
	commandLine: string;
	/** Bytes. */
	workingSet: number;
	/** Bytes; null where the OS doesn't report them. */
	privateBytes: number | null;
}

const POWERSHELL = join(
	process.env.SystemRoot ?? "C:\\Windows",
	"System32",
	"WindowsPowerShell",
	"v1.0",
	"powershell.exe",
);

/** Parses `Get-CimInstance Win32_Process | Select … | ConvertTo-Json` (an object for one process, else an array). */
export function parseWindowsProcesses(json: string): ProcessInfo[] {
	if (json.trim() === "") return [];
	const parsed = JSON.parse(json) as unknown;
	const rows = (Array.isArray(parsed) ? parsed : [parsed]) as Record<string, unknown>[];
	return rows.map((row) => ({
		pid: Number(row.ProcessId),
		ppid: Number(row.ParentProcessId),
		name: String(row.Name ?? ""),
		commandLine: String(row.CommandLine ?? ""),
		workingSet: Number(row.WorkingSetSize ?? 0),
		// Win32_Process.PrivatePageCount is the private bytes.
		privateBytes:
			row.PrivatePageCount === undefined || row.PrivatePageCount === null ? null : Number(row.PrivatePageCount),
	}));
}

/** Parses `ps -A -o pid=,ppid=,rss=,comm=,args=` (RSS in KiB). */
export function parsePsProcesses(text: string): ProcessInfo[] {
	const rows: ProcessInfo[] = [];
	for (const line of text.split("\n")) {
		const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s*(.*)$/.exec(line);
		if (!match) continue;
		rows.push({
			pid: Number(match[1]),
			ppid: Number(match[2]),
			workingSet: Number(match[3]) * 1024,
			name: match[4] ?? "",
			commandLine: match[5] ?? "",
			privateBytes: null,
		});
	}
	return rows;
}

/** Every running process, once. */
export function snapshotProcesses(): ProcessInfo[] {
	if (process.platform === "win32") {
		const result = spawnSync(
			POWERSHELL,
			[
				"-NoProfile",
				"-NonInteractive",
				"-Command",
				"Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine,WorkingSetSize,PrivatePageCount | ConvertTo-Json -Compress",
			],
			{ encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024, timeout: 60_000 },
		);
		if (result.status !== 0) throw new Error(`process snapshot failed: ${result.stderr || result.error?.message}`);
		return parseWindowsProcesses(result.stdout);
	}
	const result = spawnSync("ps", ["-A", "-o", "pid=,ppid=,rss=,comm=,args="], { encoding: "utf8" });
	if (result.status !== 0) throw new Error(`ps failed: ${result.stderr || result.error?.message}`);
	return parsePsProcesses(result.stdout);
}

/** The root and all its descendants in `processes`, root first (empty when the root isn't there). */
export function treeOf(rootPid: number, processes: readonly ProcessInfo[]): ProcessInfo[] {
	const byParent = new Map<number, ProcessInfo[]>();
	for (const info of processes) {
		if (info.pid === info.ppid) continue;
		byParent.set(info.ppid, [...(byParent.get(info.ppid) ?? []), info]);
	}
	const root = processes.find((info) => info.pid === rootPid);
	if (!root) return [];
	const tree: ProcessInfo[] = [root];
	const seen = new Set([rootPid]);
	for (let i = 0; i < tree.length; i++) {
		for (const child of byParent.get(tree[i]?.pid ?? -1) ?? []) {
			if (seen.has(child.pid)) continue;
			seen.add(child.pid);
			tree.push(child);
		}
	}
	return tree;
}

/** A tree's memory: working set and private bytes summed (private null when any member lacks it). */
export function treeMemory(tree: readonly ProcessInfo[]): {
	processes: number;
	workingSet: number;
	privateBytes: number | null;
} {
	let workingSet = 0;
	let privateBytes: number | null = 0;
	for (const info of tree) {
		workingSet += info.workingSet;
		privateBytes = privateBytes === null || info.privateBytes === null ? null : privateBytes + info.privateBytes;
	}
	return { processes: tree.length, workingSet, privateBytes };
}

/** Kills a process and its tree: `taskkill /T /F` on Windows, SIGKILL to the pid (and its group) on POSIX. */
export function killTree(pid: number): void {
	if (process.platform === "win32") {
		spawnSync(
			join(process.env.SystemRoot ?? "C:\\Windows", "System32", "taskkill.exe"),
			["/F", "/T", "/PID", String(pid)],
			{
				stdio: "ignore",
				windowsHide: true,
			},
		);
		return;
	}
	for (const target of [-pid, pid]) {
		try {
			process.kill(target, "SIGKILL");
		} catch {
			// Not a group leader, or already gone.
		}
	}
}

export function isAlive(pid: number): boolean {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
}
