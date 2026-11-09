// H9 (0482): install and uninstall footprint on Windows, in 0481's VM.
//
//   bun bench/h9-footprint.ts --out <dir> --install "<command>" --uninstall "<command>" [--between "<command>"]
//                             [--roots <dir,…>] [--runs 1] [--dry-run]
//
// Snapshots the file system under `--roots` (default: %LOCALAPPDATA%, %APPDATA%, %USERPROFILE%\.jocasta,
// %USERPROFILE%\.omp, %USERPROFILE%\.herdr, %USERPROFILE%\.local, %ProgramData%) and the registry (`reg export` of HKCU
// and HKLM\SOFTWARE), runs `--install` (timed), snapshots again, runs `--between` when given (setup and use: 0481's
// run does setup, a first run and a thread here) and snapshots again, runs `--uninstall` (timed) and snapshots a last
// time. Reports what install added, what setup and use added, and what uninstall left behind (files and registry
// values), all of it and the part naming jocasta, herdr, omp or mise. Prerequisites and minutes to the first task are
// noted by hand in the run's notes (README). Writes h9-footprint.json, h9-install-added.jsonl, h9-use-added.jsonl and
// h9-uninstall-left.jsonl.
import { spawnSync } from "node:child_process";
import { type Dirent, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BenchUsageError, flag, listFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "./lib/output.ts";

/** What a snapshot holds: file paths with size, and registry lines `key\value=data`. */
export interface Snapshot {
	files: Map<string, number>;
	registry: Set<string>;
}

/** Parses `reg export`'s text (UTF-16LE with a BOM on disk) into `[key]\name=data` lines; a key alone is `[key]`. */
export function parseRegExport(text: string): Set<string> {
	const lines = new Set<string>();
	let key = "";
	let pending = "";
	for (const raw of text.replace(/^\uFEFF/, "").split(/\r?\n/)) {
		// A value continues on the next line after a trailing backslash.
		const line = pending + raw.trim();
		if (line.endsWith("\\")) {
			pending = line.slice(0, -1);
			continue;
		}
		pending = "";
		if (line === "" || line.startsWith("Windows Registry Editor")) continue;
		if (line.startsWith("[") && line.endsWith("]")) {
			key = line;
			lines.add(key);
			continue;
		}
		lines.add(`${key}\\${line}`);
	}
	return lines;
}

/** Added and removed entries between two sets. */
export function diffSets<T>(before: Iterable<T>, after: Iterable<T>): { added: T[]; removed: T[] } {
	const a = new Set(before);
	const b = new Set(after);
	return { added: [...b].filter((x) => !a.has(x)), removed: [...a].filter((x) => !b.has(x)) };
}

const NAMED = /jocasta|jarvis|herdr|\bomp\b|oh-my-pi|mise/i;

export const named = (entries: readonly string[]) => entries.filter((entry) => NAMED.test(entry));

function walk(root: string, files: Map<string, number>): void {
	let entries: Dirent[];
	try {
		entries = readdirSync(root, { withFileTypes: true });
	} catch {
		return;
	}
	for (const entry of entries) {
		const path = join(root, entry.name);
		if (entry.isSymbolicLink()) {
			files.set(`${path} ->`, 0);
		} else if (entry.isDirectory()) {
			files.set(`${path}\\`, 0);
			walk(path, files);
		} else {
			try {
				files.set(path, statSync(path).size);
			} catch {
				files.set(path, -1);
			}
		}
	}
}

function snapshot(roots: readonly string[]): Snapshot {
	const files = new Map<string, number>();
	for (const root of roots) if (existsSync(root)) walk(root, files);
	const registry = new Set<string>();
	const dir = mkdtempSync(join(tmpdir(), "h9-reg-"));
	try {
		for (const [i, hive] of ["HKCU", "HKLM\\SOFTWARE"].entries()) {
			const file = join(dir, `${i}.reg`);
			const result = spawnSync("reg", ["export", hive, file, "/y"], { windowsHide: true, encoding: "utf8" });
			if (result.status !== 0) throw new Error(`reg export ${hive} failed: ${result.stderr || result.error?.message}`);
			for (const line of parseRegExport(readFileSync(file, "utf16le"))) registry.add(line);
		}
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
	return { files, registry };
}

function timed(command: string): { exitCode: number | null; seconds: number } {
	const started = performance.now();
	const result = spawnSync(command, { shell: true, stdio: "inherit", windowsHide: false });
	return { exitCode: result.status, seconds: (performance.now() - started) / 1000 };
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: ["--install", "--uninstall", "--between", "--roots"],
		});
		const profile = process.env.USERPROFILE ?? "";
		const roots = listFlag(args, "roots", [
			process.env.LOCALAPPDATA ?? join(profile, "AppData", "Local"),
			process.env.APPDATA ?? join(profile, "AppData", "Roaming"),
			join(profile, ".jocasta"),
			join(profile, ".omp"),
			join(profile, ".herdr"),
			join(profile, ".local"),
			process.env.ProgramData ?? "C:\\ProgramData",
		]);
		const install = flag(args, "install");
		const uninstall = flag(args, "uninstall");
		const between = flag(args, "between");
		if (args.dryRun) {
			printPlan(
				"H9 install footprint",
				[
					`snapshot files under ${roots.join(", ")} and the registry (reg export HKCU, HKLM\\SOFTWARE)`,
					`run and time: ${install ?? "<--install>"}; snapshot`,
					...(between ? [`run and time: ${between}; snapshot`] : []),
					`run and time: ${uninstall ?? "<--uninstall>"}; snapshot`,
					"report added by install, added by setup and use, and left after uninstall, all and those naming jocasta, herdr, omp or mise",
					"Windows only, in 0481's VM",
				],
				args.out,
			);
			return;
		}
		if (process.platform !== "win32") throw new BenchUsageError("H9 runs on Windows (0481's VM)");
		if (!install || !uninstall) throw new BenchUsageError("--install and --uninstall are required");
		const out = args.out as string;
		const started = conditions(["git", "docker"]);
		const runs = [];
		for (let run = 0; run < args.runs; run++) {
			const before = snapshot(roots);
			const installed = timed(install);
			const afterInstall = snapshot(roots);
			const used = between ? timed(between) : undefined;
			const afterUse = between ? snapshot(roots) : afterInstall;
			const removed = timed(uninstall);
			const afterUninstall = snapshot(roots);
			const added = {
				files: diffSets(before.files.keys(), afterInstall.files.keys()).added,
				registry: diffSets(before.registry, afterInstall.registry).added,
			};
			const useAdded = {
				files: diffSets(afterInstall.files.keys(), afterUse.files.keys()).added,
				registry: diffSets(afterInstall.registry, afterUse.registry).added,
			};
			const left = {
				files: diffSets(before.files.keys(), afterUninstall.files.keys()).added,
				registry: diffSets(before.registry, afterUninstall.registry).added,
			};
			for (const file of added.files) appendRow(out, "h9-install-added", { run, kind: "file", entry: file });
			for (const value of added.registry) appendRow(out, "h9-install-added", { run, kind: "registry", entry: value });
			for (const file of useAdded.files) appendRow(out, "h9-use-added", { run, kind: "file", entry: file });
			for (const value of useAdded.registry) appendRow(out, "h9-use-added", { run, kind: "registry", entry: value });
			for (const file of left.files) appendRow(out, "h9-uninstall-left", { run, kind: "file", entry: file });
			for (const value of left.registry) appendRow(out, "h9-uninstall-left", { run, kind: "registry", entry: value });
			const megabytes = (paths: string[], at: Snapshot) =>
				paths.reduce((sum, path) => sum + Math.max(0, at.files.get(path) ?? 0), 0) / 2 ** 20;
			runs.push({
				run,
				install: installed,
				...(used ? { use: used } : {}),
				uninstall: removed,
				added: {
					files: added.files.length,
					registry: added.registry.length,
					megabytes: megabytes(added.files, afterInstall),
				},
				...(between
					? {
							useAdded: {
								files: useAdded.files.length,
								registry: useAdded.registry.length,
								megabytes: megabytes(useAdded.files, afterUse),
								namedFiles: named(useAdded.files),
								namedRegistry: named(useAdded.registry),
							},
						}
					: {}),
				left: {
					files: left.files.length,
					registry: left.registry.length,
					megabytes: megabytes(left.files, afterUninstall),
					namedFiles: named(left.files),
					namedRegistry: named(left.registry),
				},
			});
		}
		const file = writeJson(out, "h9-footprint", {
			benchmark: "H9",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			roots,
			runs,
		});
		console.log(`wrote ${file}`);
	});
}
