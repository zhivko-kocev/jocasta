/**
 * `jocasta uninstall` (0468): removes exactly what install and setup added. The installer records its part in
 * `install.json` (the folder, the release's top-level entries, its PATH entry, the autostart value, the companion's
 * WebView2 data); setup's migrations record theirs in their markers (`setupUndo`). herdr session `jocasta` and
 * Jocasta's per-project mise configs (0477) go too. The data folder (memory, models) and the containers, images and
 * volumes are each asked about separately. Keys setup merged into his omp `config.yml` stay: they are his settings. In
 * `keybindings.yml` and `lsp.json` only the keys setup added go (in `keybindings.yml` only their lines, 0584), and
 * `WATCHDOG.md` only while it is still Jocasta's text (0479). From the install folder only the release's entries go,
 * and the folder once it is empty (0579); omp sessions still running programs from it are named and refused first.
 */
import { spawnSync } from "node:child_process";
import { existsSync, lstatSync, readdirSync, readFileSync, rmdirSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join, resolve, win32 } from "node:path";
import type { ServiceState } from "../frontier/server/launch.ts";
import { dockerEngineRunning } from "../shared/containers/index.ts";
import { UserError } from "../shared/errors.ts";
import { asObject, readJsonObject, removeTree, writeAtomically } from "../shared/fsx.ts";
import { INSTALL_MARKER } from "../shared/install.ts";
import { isInside, samePath } from "../shared/pathkey.ts";
import { editConfig, parseConfig } from "./config-file.ts";
import { fingerprint } from "./markers.ts";
import { deleteRunValue, RUN_KEY, readRunValue } from "./run-value.ts";
import { answeredYes, type McpUndo, type RunProgram, type SetupIo, type SetupUndo } from "./setup.ts";

/**
 * What the installer added (`install.json`). An older Windows file names only `dir`: the rest is then what
 * `install.ps1` adds today.
 */
export interface InstallRecord {
	/** The install folder. */
	dir: string;
	/** The release's top-level entries the installer put there (0579); absent in an install from before the list. */
	entries?: string[];
	/** The entry `install.ps1` put on the user PATH. */
	path?: string;
	/** The shell rc file `install.sh` appended its PATH line to. */
	rc?: string;
	/** The autostart Run value's name under `HKCU\…\Run` (the companion writes it, 0367). */
	autostart?: string;
	/** The companion's WebView2 data, `%LOCALAPPDATA%\<app identifier>`. */
	appData?: string;
}

/** A plain name in the install folder: never a path, `.` or `..`. */
const plainName = (value: unknown): value is string =>
	typeof value === "string" && value !== "" && value !== "." && value !== ".." && !/[\\/:]/.test(value);

/**
 * `<dir>/install.json`, or undefined when there is none (a source run, a copy) or it names another folder (0484). The
 * companion's window data counts only directly under `%LOCALAPPDATA%`.
 */
export function readInstallRecord(
	dir: string,
	localAppData: string | undefined,
	windows = process.platform === "win32",
): InstallRecord | undefined {
	const value = readJsonObject(join(dir, INSTALL_MARKER));
	if (!value || typeof value.dir !== "string" || !samePath(value.dir, dir)) return undefined;
	const record: InstallRecord = { dir: value.dir };
	if (Array.isArray(value.entries) && value.entries.every(plainName)) record.entries = value.entries;
	for (const key of ["path", "rc", "autostart", "appData"] as const) {
		if (typeof value[key] === "string") record[key] = value[key] as string;
	}
	if (windows && Object.keys(record).every((key) => key === "dir" || key === "entries")) {
		record.path = value.dir;
		record.autostart = "Jocasta";
		if (localAppData) record.appData = join(localAppData, "local.jocasta.frontier");
	}
	if (record.appData && !(localAppData && samePath(dirname(record.appData), localAppData))) delete record.appData;
	return record;
}

/** The line `install.sh` appends to the rc file for `dir`. */
export function rcPathLine(dir: string): string {
	return `export PATH="${dir}:$PATH" # jocasta`;
}

/** Removes `install.sh`'s PATH line from `rc`; false when it isn't there. */
export function removeRcLine(rc: string, dir: string): boolean {
	let text: string;
	try {
		text = readFileSync(rc, "utf8");
	} catch {
		return false;
	}
	const line = rcPathLine(dir);
	const lines = text.split("\n");
	const kept = lines.filter((l) => l !== line);
	if (kept.length === lines.length) return false;
	writeAtomically(rc, kept.join("\n").replace(/\n\n+$/, "\n"));
	return true;
}

export interface UninstallPaths {
	jocastaConfigDir: string;
	dataDir: string;
	/** mise's global config dir, for Jocasta's per-project configs (0477). */
	miseConfigDir: string;
	/** `~/.jocasta/agent/compose.yaml`, Jocasta's containers. */
	composeFile: string;
	/** The install this command removes; undefined in a source run. */
	install: InstallRecord | undefined;
	/**
	 * Whether this process runs from the install and Windows holds its program open: Jocasta's entries there go after
	 * it exits. On Linux and macOS a running program's files can be removed at once.
	 */
	runningFromInstall: boolean;
}

export interface UninstallDeps {
	serviceState(): Promise<ServiceState>;
	/** The herdr binary, or undefined when herdr isn't installed. */
	herdr: string | undefined;
	/** herdr's session, `jocasta` (`JOCASTA_HERDR_SESSION` for checks). */
	session: string;
	run: RunProgram;
	/** How to undo what setup added here (`setupUndo`). */
	setupUndo: SetupUndo[];
	/** `docker compose down --rmi all --volumes` for Jocasta's project. */
	composeDown(): Promise<{ code: number; output: string }>;
	/** Whether Docker's engine runs now; `dockerEngineRunning` by default (test seam). */
	dockerRunning?: () => boolean;
	/** Removes `entry` from the user PATH; false when it wasn't there. */
	removePathEntry(entry: string): boolean;
	/** Removes Run value `name` when it starts `launcher`; false when it doesn't. */
	removeRunValue(name: string, launcher: string): boolean;
	deleteCredential(target: string): boolean;
	/** Removes `entries` of the install folder `dir`, then `install.json` and the folder if empty, once this process exited. */
	removeAfterExit(dir: string, entries: string[]): void;
	/** The other processes running a program from the install folder (`processesUnder`); none when absent. */
	processesUnder?(dir: string): InstallProcess[];
	/** Starts herdr's server for `session` headless and waits for it, for `plugin unlink` (0558); absent in tests. */
	startHerdr?(): Promise<void>;
}

/** One thing to remove: what it is, and how. */
interface Removal {
	what: string;
	remove(): Promise<string | undefined>;
}

/** Files in Jocasta's config folder that are his, not Jocasta's: the private-names list the repo scan reads (AGENTS.md). */
const HIS_CONFIG_FILES = ["private-names.txt"];

/** Whether `path` is there, a dangling link included. */
function present(path: string): boolean {
	try {
		lstatSync(path);
		return true;
	} catch {
		return false;
	}
}

/** Why folder `path` mustn't be removed whole (0484): a drive root, or his home folder or one holding it. */
export function unsafeFolder(path: string): string | undefined {
	const full = resolve(path);
	if (dirname(full) === full) return "it is a drive root";
	if (isInside(full, homedir())) return "it is your home folder or holds it";
	return undefined;
}

/**
 * Removes a file or folder without following links (0484); says what it did. A folder that is a drive root or his
 * home folder is refused.
 */
function removePath(path: string): string {
	if (!present(path)) return `${path} was already gone`;
	const unsafe = unsafeFolder(path);
	if (unsafe) throw new Error(`kept ${path}: ${unsafe}`);
	removeTree(path);
	return `removed ${path}`;
}

/**
 * Removes a folder whose path was recorded (install.json, setup's markers) only while it still holds Jocasta's
 * `marker` (0484); `holds` checks it.
 */
function removeRecordedFolder(path: string, marker: string, holds: (dir: string) => boolean): string {
	if (!present(path)) return `${path} was already gone`;
	if (!holds(path)) throw new Error(`kept ${path}: it no longer holds ${marker}`);
	return removePath(path);
}

/** Whether `dir` is the install `install.json` names: the marker there names this same folder. */
function holdsInstallMarker(dir: string): boolean {
	const marker = readJsonObject(join(dir, INSTALL_MARKER));
	return typeof marker?.dir === "string" && samePath(marker.dir, dir);
}

/** The top-level names a release has shipped (appendix I), the companion's `app` (0552) included. */
const RELEASE_NAMES = [
	"jocasta.exe",
	"jocasta",
	"config",
	"node",
	"whiteboard",
	"whiteboard-canvas",
	"editor.worker.js",
	"hunk",
	"README.md",
	"CHANGELOG.md",
	"app",
];

/** What of the install folder uninstall removes (0579), and what it leaves there. */
export interface InstallContents {
	/** Jocasta's entries there now, `install.json` aside (it goes last). */
	ours: string[];
	/** His other entries: the folder stays for them. */
	others: string[];
}

/**
 * The install folder's entries by owner (0579): the ones `install.json` lists, with the `<name>.old-<stamp>` copies an
 * upgrade renamed aside while they were in use (install.ps1); an install from before the list is all Jocasta's only when
 * every entry is a release's name, else it is refused whole.
 */
export function installContents(record: InstallRecord, windows = process.platform === "win32"): InstallContents {
	const key = (name: string) => (windows ? name.toLowerCase() : name);
	const listed = new Set((record.entries ?? RELEASE_NAMES).map(key));
	const isOurs = (name: string) => listed.has(key(name.replace(/\.old-\d+$/, "")));
	const names = readdirSync(record.dir)
		.filter((name) => key(name) !== key(INSTALL_MARKER))
		.sort();
	const contents = { ours: names.filter(isOurs), others: names.filter((name) => !isOurs(name)) };
	if (!record.entries && contents.others.length > 0) {
		throw new Error(
			`kept ${record.dir}: it holds files that aren't Jocasta's (${contents.others.join(", ")}); remove Jocasta's files from it by hand`,
		);
	}
	return contents;
}

/** Removes Jocasta's entries from the install folder, `install.json` last, then the folder when nothing else is left. */
function removeInstall(dir: string, contents: InstallContents): string {
	for (const name of contents.ours) removeTree(join(dir, name));
	rmSync(join(dir, INSTALL_MARKER), { force: true });
	if (contents.others.length > 0) return `removed Jocasta's files from ${dir}; kept the folder (it holds other files)`;
	removeIfEmpty(dir);
	return `removed ${dir}`;
}

/** A process whose program is under the install folder. */
export interface InstallProcess {
	pid: number;
	/** Its program's full path. */
	path: string;
	/** Its command line as the OS reports it. */
	commandLine: string;
}

/**
 * How the refusal names a process (0579): its program and up to three arguments, a path inside the install shortened
 * to its top-level entry (`node.exe whiteboard mcp (pid 5332)`). Windows paths: only Windows lists them.
 */
export function describeProcess(dir: string, proc: InstallProcess): string {
	const words = [...proc.commandLine.matchAll(/"([^"]*)"|(\S+)/g)].map((m) => m[1] ?? m[2] ?? "").slice(1, 4);
	const shown = words.map((word) => {
		const inside = win32.isAbsolute(word) ? win32.relative(dir, word) : "";
		return inside && !inside.startsWith("..") && !win32.isAbsolute(inside) ? (inside.split("\\")[0] as string) : word;
	});
	return `${[win32.basename(proc.path), ...shown].join(" ")} (pid ${proc.pid})`;
}

/** The processes, other than this one, whose program is under `dir` (Windows: Win32_Process); none elsewhere. */
export function processesUnder(dir: string): InstallProcess[] {
	if (process.platform !== "win32") return [];
	const script = `
$dir = $env:JOCASTA_INSTALL_DIR.TrimEnd('\\') + '\\'
Get-CimInstance Win32_Process | Where-Object {
	$_.ExecutablePath -and $_.ExecutablePath.StartsWith($dir, [StringComparison]::OrdinalIgnoreCase)
} | ForEach-Object { "$($_.ProcessId)\`t$($_.ExecutablePath)\`t$($_.CommandLine)" }
`;
	const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
		env: { ...process.env, JOCASTA_INSTALL_DIR: dir },
		encoding: "utf8",
		windowsHide: true,
	});
	return result.stdout
		.split(/\r?\n/)
		.map((line) => line.split("\t"))
		.filter(([pid, path]) => pid && path && Number(pid) !== process.pid)
		.map(([pid, path, commandLine]) => ({ pid: Number(pid), path: path as string, commandLine: commandLine ?? "" }));
}

/** Whether `dir` holds the companion's WebView2 data: `<channel>/WebView2` (Electrobun's layout). */
function holdsWindowData(dir: string): boolean {
	try {
		return readdirSync(dir, { withFileTypes: true }).some(
			(entry) => entry.isDirectory() && existsSync(join(dir, entry.name, "WebView2")),
		);
	} catch {
		return false;
	}
}

/**
 * A folder setup recorded (0484): removed whole only with the file setup wrote in it still there. A record from before
 * 0484 doesn't name the file and may name a folder setup didn't make: that folder goes only when it is empty.
 */
function removeSetupFolder(path: string, file: string | undefined): string {
	if (file !== undefined) return removeRecordedFolder(path, file, (dir) => existsSync(join(dir, file)));
	if (!present(path)) return `${path} was already gone`;
	try {
		rmdirSync(path);
		return `removed ${path}`;
	} catch {
		return `kept ${path}: setup may not have made it, and it isn't empty`;
	}
}

/**
 * Jocasta's servers out of an omp `mcp.json` (0468), each only while it still is what setup wrote: the entry setup
 * writes today, or, all together, the entries its marker fingerprinted. His other servers and keys stay; the file goes
 * when nothing else is in it.
 */
function removeMcpServers(item: McpUndo): string {
	const { path, servers } = item;
	const quoted = (names: string[]) => names.map((s) => `"${s}"`).join(", ");
	let config: Record<string, unknown> | undefined;
	try {
		config = asObject(JSON.parse(readFileSync(path, "utf8")));
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return `${path} was already gone`;
		throw new Error(`${path} is not JSON; remove ${servers.join(", ")} from it by hand`);
	}
	const current = asObject(config?.mcpServers);
	if (!config || !current) return `${path} has no MCP servers to remove`;
	const present = servers.filter((name) => current[name] !== undefined);
	const asWritten =
		fingerprint(JSON.stringify(Object.fromEntries(present.map((name) => [name, current[name]])))) === item.wrote;
	const gone = present.filter((name) => asWritten || Bun.deepEquals(current[name], item.writes[name], true));
	const yours = present.filter((name) => !gone.includes(name));
	if (gone.length > 0) {
		const kept = Object.fromEntries(Object.entries(current).filter(([name]) => !gone.includes(name)));
		if (Object.keys(kept).length === 0 && Object.keys(config).length === 1) {
			rmSync(path, { force: true });
			return `removed ${path}`;
		}
		writeAtomically(path, `${JSON.stringify({ ...config, mcpServers: kept }, null, 2)}\n`);
	}
	const said =
		gone.length > 0
			? `removed MCP server${gone.length > 1 ? "s" : ""} ${quoted(gone)} from ${path}`
			: `${path} has none of Jocasta's MCP servers left`;
	return yours.length > 0 ? `${said}; kept ${quoted(yours)} (not what setup wrote)` : said;
}

/**
 * The keys setup added out of one of his omp files (0479), each only while it still has the value setup wrote; in a
 * YAML file only their lines go, his comments and formatting stay (0584), and a file whose lines can't be edited is
 * left as it is. The file goes when setup made it and nothing else is left in it: no key, comment or line of his.
 */
function removeKeys(path: string, keys: Record<string, unknown>, created: boolean): string {
	const names = Object.keys(keys);
	const unreadable = () => new Error(`${path} can't be read; remove ${names.join(", ")} from it by hand`);
	let text: string;
	try {
		text = readFileSync(path, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return `${path} was already gone`;
		throw unreadable();
	}
	let config: Record<string, unknown>;
	try {
		config = parseConfig(path, text);
	} catch {
		throw unreadable();
	}
	const gone: string[] = [];
	const yours: string[] = [];
	for (const name of names) {
		if (config[name] === undefined) continue;
		if (Bun.deepEquals(config[name], keys[name], true)) gone.push(name);
		else yours.push(name);
	}
	const left = Object.fromEntries(Object.entries(config).filter(([name]) => !gone.includes(name)));
	const edited = gone.length > 0 ? editConfig(path, text, left, [], gone) : text;
	if (edited === undefined) {
		throw new Error(
			`${path} can't be edited line by line, so uninstall left it as it is; remove ${gone.join(", ")} from it by hand`,
		);
	}
	// `{}` is what an emptied JSON file reads as; a YAML one with anything of his left still has lines.
	if (created && Object.keys(left).length === 0 && ["", "{}"].includes(edited.trim())) {
		rmSync(path, { force: true });
		return `removed ${path}`;
	}
	if (gone.length > 0) writeAtomically(path, edited);
	const said = gone.length > 0 ? `removed ${gone.join(", ")} from ${path}` : `${path} has none of Jocasta's keys left`;
	return yours.length > 0 ? `${said}; kept ${yours.join(", ")} (you changed it)` : said;
}

/** A file setup wrote, only while it is still that text (0479). */
function removeText(path: string, expected: string): string {
	let text: string;
	try {
		text = readFileSync(path, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return `${path} was already gone`;
		throw error;
	}
	if (fingerprint(text) !== expected) return `kept ${path}: you changed it`;
	rmSync(path, { force: true });
	return `removed ${path}`;
}

function undoRemoval(item: SetupUndo, deps: UninstallDeps): Removal {
	switch (item.kind) {
		case "file":
			return { what: item.path, remove: async () => removePath(item.path) };
		case "dir":
			return { what: item.path, remove: async () => removeSetupFolder(item.path, item.file) };
		case "mcp":
			return {
				what: `MCP server${item.servers.length > 1 ? "s" : ""} ${item.servers.map((s) => `"${s}"`).join(", ")} in ${item.path} (each while it is still what setup wrote)`,
				remove: async () => removeMcpServers(item),
			};
		case "herdr-integration":
			return {
				what: `herdr's omp integration: ${item.path}`,
				remove: async () => {
					if (!deps.herdr) return removePath(item.path);
					const { code, output } = await deps.run([deps.herdr, "integration", "uninstall", "omp"]);
					if (code !== 0) throw new Error(`herdr integration uninstall omp exited ${code}: ${output.trim()}`);
					return "removed herdr's omp integration";
				},
			};
		case "herdr-plugin": {
			const herdr = deps.herdr;
			return {
				what: `the \`${item.id}\` herdr plugin (unlinked from herdr)`,
				remove: async () => {
					if (!herdr) return `herdr isn't installed; the ${item.id} plugin had nothing to unlink`;
					const unlink = () => deps.run([herdr, "--session", deps.session, "plugin", "unlink", item.id]);
					let { code, output } = await unlink();
					// Unlinking needs a running server (0558): a headless one for the session, which its own step stops.
					if (code !== 0 && /not.running|unreachable/i.test(output) && deps.startHerdr) {
						await deps.startHerdr();
						({ code, output } = await unlink());
					}
					// herdr 0.9.5 answers an unlink of a plugin it doesn't have with exit 0 and `"removed":false`.
					if (code === 0 && /"removed":\s*false/.test(output)) {
						return `the ${item.id} herdr plugin was already unlinked`;
					}
					if (code !== 0) throw new Error(`herdr plugin unlink ${item.id} exited ${code}: ${output.trim()}`);
					return `unlinked the ${item.id} herdr plugin`;
				},
			};
		}
		case "keys":
			return {
				what: `Jocasta's ${Object.keys(item.keys).join(", ")} in ${item.path}`,
				remove: async () => removeKeys(item.path, item.keys, item.created),
			};
		case "text":
			return {
				what: `${item.path} (while it is still Jocasta's text)`,
				remove: async () => removeText(item.path, item.fingerprint),
			};
		case "credential":
			return {
				what: `Windows Credential Manager: ${item.target}`,
				remove: async () =>
					deps.deleteCredential(item.target) ? `removed ${item.target}` : `${item.target} was already gone`,
			};
	}
}

/** Jocasta's per-project mise configs (0477), `config.jocasta-<project id>.override.toml`. */
function miseProjectConfigs(dir: string): string[] {
	try {
		return readdirSync(dir)
			.filter((name) => /^config\.jocasta-.+\.override\.toml$/.test(name))
			.map((name) => join(dir, name));
	} catch {
		return [];
	}
}

/** Removes `dir` when it is empty. */
function removeIfEmpty(dir: string): void {
	try {
		rmdirSync(dir);
	} catch {
		// Not empty, or already gone.
	}
}

/** Shows what goes, asks, and removes it; the data folder and the containers are asked about separately. */
export async function runUninstall(paths: UninstallPaths, deps: UninstallDeps, io: SetupIo): Promise<number> {
	const state = await deps.serviceState();
	if (state !== "down") {
		throw new UserError(
			"Jocasta is running: close it (quit herdr's client in the `jocasta` terminal), then run `jocasta uninstall` again.",
			1,
		);
	}
	// Open omp sessions run `jocasta.exe mcp` and whiteboard's node.exe from the install: its files can't go (0579).
	const running = paths.install ? (deps.processesUnder?.(paths.install.dir) ?? []) : [];
	if (paths.install && running.length > 0) {
		const dir = paths.install.dir;
		throw new UserError(
			`omp sessions still run programs from ${dir}: ${running.map((p) => describeProcess(dir, p)).join(", ")}. Close them, then run jocasta uninstall again.`,
			1,
		);
	}
	const removals: Removal[] = [];
	// The plugin is unlinked first, while session `jocasta` can still run the server unlinking needs (0558).
	const plugins = deps.setupUndo.filter((item) => item.kind === "herdr-plugin");
	for (const item of plugins) removals.push(undoRemoval(item, deps));
	if (deps.herdr) {
		const herdr = deps.herdr;
		removals.push({
			what: `herdr session \`${deps.session}\` (stopped, its panes closed, then deleted)`,
			remove: async () => {
				await deps.run([herdr, "session", "stop", deps.session]);
				const { code, output } = await deps.run([herdr, "session", "delete", deps.session]);
				if (code !== 0) throw new Error(`herdr session delete ${deps.session} exited ${code}: ${output.trim()}`);
				return `removed herdr session \`${deps.session}\``;
			},
		});
	}
	for (const item of deps.setupUndo) if (item.kind !== "herdr-plugin") removals.push(undoRemoval(item, deps));
	for (const file of miseProjectConfigs(paths.miseConfigDir)) {
		removals.push({ what: `Jocasta's mise config for a project: ${file}`, remove: async () => removePath(file) });
	}
	const install = paths.install;
	if (install) {
		const { autostart, path, rc, appData } = install;
		if (autostart) {
			const launcher = join(install.dir, "app", "bin", "launcher.exe");
			removals.push({
				what: `the autostart value ${RUN_KEY}\\${autostart}`,
				remove: async () =>
					deps.removeRunValue(autostart, launcher)
						? `removed the autostart value ${autostart}`
						: `the autostart value ${autostart} was already gone`,
			});
		}
		if (path) {
			removals.push({
				what: `the PATH entry ${path}`,
				remove: async () =>
					deps.removePathEntry(path) ? `removed ${path} from the user PATH` : `${path} was already off the user PATH`,
			});
		}
		if (rc) {
			removals.push({
				what: `the PATH line in ${rc}`,
				remove: async () =>
					removeRcLine(rc, install.dir) ? `removed the PATH line from ${rc}` : `${rc} had no Jocasta PATH line`,
			});
		}
		if (appData) {
			removals.push({
				what: `the companion's window data: ${appData}`,
				remove: async () => removeRecordedFolder(appData, "the companion's WebView2 data", holdsWindowData),
			});
		}
	}
	// The containers' compose file lives in the config folder: they go before it.
	const containers = existsSync(paths.composeFile);

	io.print("jocasta uninstall will remove what install and setup added:");
	for (const removal of removals) io.print(`- ${removal.what}`);
	io.print(`- Jocasta's config folder: ${paths.jocastaConfigDir}`);
	const ownFiles = HIS_CONFIG_FILES.map((name) => join(paths.jocastaConfigDir, name)).filter((path) =>
		existsSync(path),
	);
	for (const file of ownFiles) io.print(`  (kept: ${file}, yours, not Jocasta's)`);
	if (install) io.print(`- the install: ${install.dir}`);
	io.print("Keys setup merged into omp's config.yml stay: they are your settings.");
	io.print("");
	if (!answeredYes(await io.ask("Remove these? [y/N] "))) {
		io.print("Nothing removed.");
		return 0;
	}
	const data = existsSync(paths.dataDir)
		? answeredYes(await io.ask(`Also remove Jocasta's data folder ${paths.dataDir} (memory, models, logs)? [y/N] `))
		: false;
	const volumes = containers
		? answeredYes(
				await io.ask(
					"Also remove Jocasta's containers, images and volumes (Graphiti and FalkorDB, with the memory graph)? [y/N] ",
				),
			)
		: false;

	let failed = 0;
	const attempt = async (what: string, remove: () => Promise<string | undefined>) => {
		try {
			const said = await remove();
			if (said) io.print(said);
		} catch (error) {
			failed++;
			io.print(`couldn't remove ${what}: ${error instanceof Error ? error.message : String(error)}`);
		}
	};
	for (const removal of removals) await attempt(removal.what, removal.remove);
	if (volumes) {
		await attempt("Jocasta's containers, images and volumes", async () => {
			if (!(deps.dockerRunning ?? dockerEngineRunning)()) throw new Error("Docker isn't running, so they stay");
			const { code, output } = await deps.composeDown();
			if (code !== 0) throw new Error(`docker compose down exited ${code}: ${output.trim()}`);
			return "removed Jocasta's containers, images and volumes";
		});
	}
	if (data) {
		await attempt(paths.dataDir, async () => {
			const said = removePath(paths.dataDir);
			// `%LOCALAPPDATA%\jocasta` holds only the data folder.
			if (basename(dirname(paths.dataDir)).toLowerCase() === "jocasta") removeIfEmpty(dirname(paths.dataDir));
			return said;
		});
	}
	await attempt(paths.jocastaConfigDir, async () => {
		if (ownFiles.length === 0) {
			const said = removePath(paths.jocastaConfigDir);
			removeIfEmpty(dirname(paths.jocastaConfigDir));
			return said;
		}
		const keep = new Set(ownFiles.map((file) => basename(file).toLowerCase()));
		for (const name of readdirSync(paths.jocastaConfigDir)) {
			if (!keep.has(name.toLowerCase())) removePath(join(paths.jocastaConfigDir, name));
		}
		return `removed ${paths.jocastaConfigDir} except ${ownFiles.join(", ")}`;
	});
	if (install) {
		await attempt(install.dir, async () => {
			if (!present(install.dir)) return `${install.dir} was already gone`;
			const unsafe = unsafeFolder(install.dir);
			if (unsafe) throw new Error(`kept ${install.dir}: ${unsafe}`);
			if (!holdsInstallMarker(install.dir)) {
				throw new Error(`kept ${install.dir}: it no longer holds ${INSTALL_MARKER} naming it`);
			}
			const contents = installContents(install);
			if (!paths.runningFromInstall) return removeInstall(install.dir, contents);
			deps.removeAfterExit(install.dir, contents.ours);
			return contents.others.length > 0
				? `Jocasta's files in ${install.dir} go when this command exits; the folder stays (it holds other files).`
				: `${install.dir} goes when this command exits.`;
		});
	}
	if (failed > 0) {
		io.print(
			`Done, except ${failed} thing${failed > 1 ? "s" : ""} above; remove ${failed > 1 ? "them" : "it"} by hand.`,
		);
		return 1;
	}
	io.print("Jocasta is uninstalled.");
	return 0;
}

/** The user PATH without `entry` (as stored, `%VARS%` kept); open programs are told the environment changed. */
export function removeUserPathEntry(entry: string): boolean {
	if (process.platform !== "win32") return false;
	const script = `
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
try {
	$raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
	$entries = @($raw -split ';' | Where-Object { $_ })
	$kept = @($entries | Where-Object { $_.TrimEnd('\\') -ne $env:JOCASTA_PATH_ENTRY.TrimEnd('\\') })
	if ($kept.Count -eq $entries.Count) { exit 3 }
	$key.SetValue('Path', ($kept -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
} finally { $key.Close() }
Add-Type -Namespace JocastaUninstall -Name Native -MemberDefinition '[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);'
$result = [UIntPtr]::Zero
[void][JocastaUninstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
`;
	const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
		env: { ...process.env, JOCASTA_PATH_ENTRY: entry },
		encoding: "utf8",
		windowsHide: true,
	});
	if (result.status === 3) return false;
	if (result.status !== 0)
		throw new Error(`the user PATH couldn't be changed: ${(result.stderr || result.stdout).trim()}`);
	return true;
}

/** Removes Run value `name` when its data starts `launcher` (another program's value of that name stays). */
export function removeRunValueFor(name: string, launcher: string): boolean {
	if (readRunValue(name) !== `"${launcher}"`) return false;
	if (!deleteRunValue(name)) throw new Error(`reg delete ${RUN_KEY}\\${name} failed`);
	return true;
}

/**
 * Removes `entries` of the install folder `dir` once this process has exited (Windows holds the running jocasta.exe):
 * a hidden PowerShell of its own console, so closing this terminal doesn't end it, tries every few seconds for five
 * minutes, as omp sessions closing free their programs (0579); `install.json` goes, then the folder if empty, only once
 * every entry is gone. .NET's recursive delete never follows a junction. The names reach it through the environment,
 * never its command line. Throws when the remover didn't start.
 */
export function removeAfterExit(dir: string, entries: string[], pid = process.pid): void {
	const script = `
$dir = $env:JOCASTA_UNINSTALL_DIR
$entries = @($env:JOCASTA_UNINSTALL_ENTRIES -split "\`n" | Where-Object { $_ })
Wait-Process -Id ([int]$env:JOCASTA_UNINSTALL_PID) -Timeout 30 -ErrorAction SilentlyContinue
for ($round = 0; $round -lt 60; $round++) {
	$left = 0
	foreach ($name in $entries) {
		$path = Join-Path $dir $name
		try {
			if ([IO.Directory]::Exists($path)) { [IO.Directory]::Delete($path, $true) }
			elseif ([IO.File]::Exists($path)) { [IO.File]::Delete($path) }
		} catch { $left++ }
	}
	if ($left -eq 0) { break }
	Start-Sleep -Seconds 5
}
if ($left -eq 0) {
	[IO.File]::Delete((Join-Path $dir '${INSTALL_MARKER}'))
	try { [IO.Directory]::Delete($dir) } catch {}
}
`;
	// Started by a short-lived PowerShell's Start-Process: a detached console program gets no console and Windows
	// PowerShell then never runs. The script goes as -EncodedCommand (UTF-16LE, base64), its quotes untouched.
	const start =
		"Start-Process powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand',$env:JOCASTA_UNINSTALL_SCRIPT";
	const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", start], {
		encoding: "utf8",
		windowsHide: true,
		env: {
			...process.env,
			JOCASTA_UNINSTALL_SCRIPT: Buffer.from(script, "utf16le").toString("base64"),
			JOCASTA_UNINSTALL_DIR: dir,
			JOCASTA_UNINSTALL_ENTRIES: entries.join("\n"),
			JOCASTA_UNINSTALL_PID: String(pid),
		},
	});
	if (result.status !== 0) {
		throw new Error(`its remover didn't start (${(result.stderr || result.error?.message || "").trim()})`);
	}
}
