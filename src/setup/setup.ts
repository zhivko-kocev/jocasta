import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { TOKYO_NIGHT } from "../frontier/canvas/palette.ts";
import { findHerdr } from "../frontier/server/herdr.ts";
import { STT_TOGGLE_KEY, STT_TOGGLE_SETTING } from "../frontier/server/ptt-hold.ts";
import { WHITEBOARD_ENV, type WhiteboardRuntime, whiteboardRuntime } from "../frontier/server/whiteboard.ts";
import { CALENDAR_CREDENTIAL, CALENDAR_ENV, readCredential, writeCredential } from "../shared/credentials.ts";
import { UserError } from "../shared/errors.ts";
import { asObject, writeAtomically } from "../shared/fsx.ts";
import { normalizePathKey } from "../shared/identity.ts";
import { selfCommand, shippedConfigDir } from "../shared/install.ts";
import { log } from "../shared/log.ts";
import { configDir, dataDir, ompAgentDir } from "../shared/paths.ts";
import { lineDiff } from "./diff.ts";
import { fingerprint, type Marker, readMarker, recordedUndo, type Undo, writeMarker } from "./markers.ts";
import { MISE_FRAGMENT } from "./mise.ts";
import { mergeOmpSettings, ompYaml } from "./omp-settings.ts";
import { RUN_KEY, RUN_VALUE, startsCompanion } from "./run-value.ts";

/**
 * `jocasta setup` (0387, 0392, 0393, 0396, 0401, 0430, 0440, 0466, 0467, 0476, 0479): what Jocasta adds to omp, herdr, Windows
 * Terminal and mise, and the calendar's secret in Windows Credential Manager. Each thing is a numbered migration with
 * a marker in `~/.jocasta/agent/setup/` (0466): it is pending until applied, and again when what Jocasta ships for it
 * changes, so one runs once per shipped version and a file he edits afterwards is left alone until then (`--refresh`
 * resets one, 0467). Every pending change is shown as a diff and written only after his `y`, in order; a failed one
 * stops the rest. Jocasta's entries in omp's MCP configs are merged beside his other servers; omp's settings are merged
 * into his `config.yml` key by key, keys he set to another value kept and shown, and so are its keys in his
 * `keybindings.yml` and `lsp.json` (0479); his own advisor brief (`WATCHDOG.md`) is shown and kept. Every file is
 * written through a temp file and a rename, so a crash never leaves one torn. Each marker names how to undo its
 * migration (0468).
 */

/** Where setup reads and writes; every path injectable so tests never touch his homes. */
export interface SetupPaths {
	/** omp's agent dir, `~/.omp/agent` (`PI_CODING_AGENT_DIR` overrides it, as in omp; `ompAgentDir()`). */
	ompAgentDir: string;
	/** Jocasta's config dir, `~/.jocasta/agent`. */
	jocastaConfigDir: string;
	/** Jocasta's data dir; the boss pane runs in `<data>/home`. */
	dataDir: string;
	/** The shipped `config/` (boss manual, templates, skills): beside `jocasta.exe`, or the repo's in a source run. */
	assetsDir: string;
	/** `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments`; undefined off Windows. */
	terminalFragmentsDir?: string;
	/** mise's global config dir, `~/.config/mise` (`MISE_CONFIG_DIR` overrides it, as in mise). */
	miseConfigDir: string;
	/** How omp starts this Jocasta: `[jocasta.exe]`, or `[bun, <repo>/src/entry.ts]` in a source run. */
	jocastaCommand: string[];
	/** whiteboard's CLI for `whiteboard mcp` (0393): beside `jocasta.exe`, or the built clone; undefined when absent. */
	whiteboard: WhiteboardRuntime | undefined;
}

/** One change setup proposes: its diff lines (`+ `, `- `, `  `) and how to make it. */
export interface SetupChange {
	title: string;
	path: string;
	diff: string[];
	/** Keys he set to another value than Jocasta ships, kept as they are (omp's settings, 0466). */
	kept?: string[];
	/** Nothing is written: the change only shows what of his stays (0479). */
	keepsOnly?: true;
	apply(): Promise<void>;
}

/** Runs a program to completion; resolves its exit code and combined output. */
export type RunProgram = (argv: string[]) => Promise<{ code: number; output: string }>;

/** A change, or a migration already in place, with the marker written once it is applied. */
export interface PlannedChange {
	/** The marker's name: `<NNNN>-<slug>` for a migration, `calendar` for the calendar's secret. */
	name: string;
	/** The file it is about. */
	path: string;
	/** Undefined when the target already is what Jocasta ships: only the marker is written. */
	change?: SetupChange;
	marker: Omit<Marker, "appliedAt">;
}

export interface SetupPlan {
	/** Pending changes, in migration order. */
	changes: PlannedChange[];
	/** Migrations already in place without a current marker: marked when setup runs. */
	settled: PlannedChange[];
	/** Things setup can't do now, said before the diff. */
	notes: string[];
}

/** The calendar secret's store (0430): Windows Credential Manager target `jocasta/calendar-ical`. */
export interface CalendarStore {
	/** Whether the secret is stored. */
	stored(): boolean;
	save(secret: string): void;
	/** `JOCASTA_CALENDAR_ICAL_SECRET`'s value, imported when the store has none. */
	fromEnv: string | undefined;
}

export interface SetupDeps {
	/** The herdr binary, or undefined when herdr isn't installed. */
	herdr: string | undefined;
	/** The mise binary, or undefined when mise isn't installed: then setup writes no mise file (0476). */
	mise?: string;
	run: RunProgram;
	/** The calendar secret's store; undefined where there is none (off Windows). */
	calendar?: CalendarStore;
	/** `herdr server reload-config` for Jocasta's session, after a herdr.toml refresh (0467). */
	reloadHerdr?: () => Promise<{ code: number; output: string }>;
	/**
	 * The logon Run value the companion wrote (0367), read and removed (0557); undefined off Windows and in tests, so
	 * nothing touches his registry there.
	 */
	runValue?: { read(name: string): string | undefined; remove(name: string): boolean };
}

/** Credential Manager as the calendar's store, with the env value taken at start; undefined off Windows. */
export function defaultCalendarStore(fromEnv: string | undefined): CalendarStore | undefined {
	if (process.platform !== "win32") return undefined;
	return {
		stored: () => readCredential(CALENDAR_CREDENTIAL) !== undefined,
		save: (secret) => writeCredential(CALENDAR_CREDENTIAL, secret),
		fromEnv,
	};
}

/** mise's global config dir as mise resolves it: `MISE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/mise`, else `~/.config/mise`. */
export function miseConfigDir(): string {
	if (process.env.MISE_CONFIG_DIR) return process.env.MISE_CONFIG_DIR;
	return join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "mise");
}

/** The real paths on this machine. */
export function defaultSetupPaths(assetsDir: string, jocastaCommand: string[]): SetupPaths {
	const local = process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local");
	return {
		ompAgentDir: ompAgentDir(),
		jocastaConfigDir: configDir(),
		dataDir: dataDir(),
		assetsDir,
		terminalFragmentsDir:
			process.platform === "win32" ? join(local, "Microsoft", "Windows Terminal", "Fragments") : undefined,
		miseConfigDir: miseConfigDir(),
		jocastaCommand,
		whiteboard: whiteboardRuntime(),
	};
}

/** The file herdr's omp integration writes (`herdr:src/integration/targets.rs:89-106`). */
export const HERDR_OMP_EXTENSION = "herdr-omp-agent-state.ts";

/**
 * herdr's Tokyo Night as a Windows Terminal scheme, so pane content matches the chrome (0401), from the one palette
 * file (0474). A fragment scheme must define all 16 colours.
 */
export const TOKYO_NIGHT_SCHEME = (() => {
	const hex = (colour: string) => colour.toUpperCase();
	const p = TOKYO_NIGHT;
	return {
		name: "Tokyo Night",
		background: hex(p.bg0),
		foreground: hex(p.fg),
		cursorColor: hex(p.fg),
		selectionBackground: hex(p.bg3),
		black: hex(p.bg2),
		red: hex(p.red),
		green: hex(p.green),
		yellow: hex(p.yellow),
		blue: hex(p.blue),
		purple: hex(p.purple),
		cyan: hex(p.cyan),
		white: hex(p.fg2),
		brightBlack: hex(p.brightBlack),
		brightRed: hex(p.red),
		brightGreen: hex(p.green),
		brightYellow: hex(p.yellow),
		brightBlue: hex(p.blue),
		brightPurple: hex(p.purple),
		brightCyan: hex(p.cyan),
		brightWhite: hex(p.fg),
	} as const;
})();

const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`;

function readText(path: string): string | undefined {
	try {
		return readFileSync(path, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
		throw error;
	}
}

function readJsonObject(path: string, text: string): Record<string, unknown> {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch (error) {
		throw new UserError(`${path} is not JSON (${(error as Error).message}); fix it and run setup again`, 1);
	}
	const object = asObject(value);
	if (!object) throw new UserError(`${path} is not a JSON object; fix it and run setup again`, 1);
	return object;
}

function readYamlObject(path: string, text: string): Record<string, unknown> {
	let value: unknown;
	try {
		value = text.trim() === "" ? {} : Bun.YAML.parse(text);
	} catch (error) {
		throw new UserError(`${path} is not YAML (${(error as Error).message}); fix it and run setup again`, 1);
	}
	const object = asObject(value);
	if (!object) throw new UserError(`${path} is not a YAML mapping; fix it and run setup again`, 1);
	return object;
}

/** A new file: every line added. */
function newFile(title: string, path: string, text: string): SetupChange {
	return {
		title,
		path,
		diff: text
			.replace(/\n$/, "")
			.split("\n")
			.map((l) => `+ ${l}`),
		apply: async () => writeAtomically(path, text),
	};
}

/**
 * How `file` in folder `dir` is undone (0484): the whole folder only when setup makes it now, or its marker says setup
 * made it; else only the file, so a folder that was his stays.
 */
function folderUndo(dir: string, file: string, marker: Marker | undefined): Undo {
	const made =
		!existsSync(dir) || marker?.undo.some((u) => u.kind === "dir" && u.path === dir && u.file === file) === true;
	return made ? { kind: "dir", path: dir, file } : { kind: "file", path: join(dir, file) };
}

/** A file Jocasta owns, following `wanted`: new when missing, its diff when it differs, undefined when it matches. */
function ownedFile(title: string, path: string, wanted: string): SetupChange | undefined {
	const has = readText(path);
	if (has === undefined) return newFile(title, path, wanted);
	if (has === wanted) return undefined;
	return { title, path, diff: lineDiff(has, wanted), apply: async () => writeAtomically(path, wanted) };
}

/**
 * MCP servers merged into an omp `mcp.json`, his other servers and keys kept; one change per file, titled by the
 * servers it adds or changes; undefined when they're all there.
 */
function mcpEntry(path: string, servers: Record<string, Record<string, unknown>>): SetupChange | undefined {
	const before = readText(path);
	const config = before === undefined ? {} : readJsonObject(path, before);
	const existing = config.mcpServers;
	if (existing !== undefined && !asObject(existing)) {
		throw new UserError(`${path}: mcpServers is not an object; fix it and run setup again`, 1);
	}
	const current = (existing ?? {}) as Record<string, unknown>;
	const changed = Object.keys(servers).filter(
		(name) => JSON.stringify(current[name]) !== JSON.stringify(servers[name]),
	);
	if (changed.length === 0) return undefined;
	const after = json({ ...config, mcpServers: { ...current, ...servers } });
	const title = `omp MCP server${changed.length > 1 ? "s" : ""} ${changed.map((name) => `"${name}"`).join(", ")}`;
	if (before === undefined) return newFile(title, path, after);
	return { title, path, diff: lineDiff(before, after), apply: async () => writeAtomically(path, after) };
}

/** `config/herdr.toml` with `{{data}}` filled in, as a TOML basic-string value (comments keep the placeholder). */
export function herdrConfig(template: string, data: string): string {
	const escaped = data.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
	return template
		.split("\n")
		.map((line) => (line.trimStart().startsWith("#") ? line : line.replaceAll("{{data}}", escaped)))
		.join("\n");
}

/** The plugin's manifest file name, shipped in `config/herdr-plugin/` and written to `~/.jocasta/agent/herdr-plugin/`. */
export const PLUGIN_MANIFEST = "herdr-plugin.toml";

/**
 * `config/herdr-plugin/herdr-plugin.toml` with `{{jocasta}}` filled in as TOML strings, one per argv element of how herdr
 * starts this Jocasta (comments keep the placeholder).
 */
export function pluginManifest(template: string, jocastaCommand: string[]): string {
	const argv = jocastaCommand.map((part) => JSON.stringify(part)).join(", ");
	return template
		.split("\n")
		.map((line) => (line.trimStart().startsWith("#") ? line : line.replaceAll("{{jocasta}}", argv)))
		.join("\n");
}

/** `frontier`/`pollers` from Pi's `settings.json` as `jocasta.yaml` (0387); undefined when there's nothing to move. */
function jocastaYaml(settingsPath: string): string | undefined {
	const text = readText(settingsPath);
	if (text === undefined) return undefined;
	const settings = readJsonObject(settingsPath, text);
	const moved: Record<string, unknown> = {};
	for (const key of ["frontier", "pollers"]) if (settings[key] !== undefined) moved[key] = settings[key];
	if (Object.keys(moved).length === 0) return undefined;
	const yaml = Bun.YAML.stringify(moved, null, 2)
		.split("\n")
		.map((l) => l.trimEnd())
		.join("\n")
		.replace(/\n*$/, "\n");
	return `# Jocasta-only settings (0387), moved from settings.json's frontier and pollers keys by \`jocasta setup\`.\n${yaml}`;
}

/** What one migration would do now. */
interface Step {
	/** The file it is about. */
	path: string;
	/** What Jocasta ships for it on this machine: a new one makes the migration pending again. */
	fingerprint: string;
	/** Undefined when the target already is what Jocasta ships. */
	change?: SetupChange;
	undo: Undo[];
	/** Kept in the marker (the omp settings applied). */
	shipped?: unknown;
	/** The MCP servers it writes, by name: uninstall compares his entries with them (0468). Not kept in the marker. */
	servers?: Record<string, Record<string, unknown>>;
}

interface PlanContext {
	paths: SetupPaths;
	deps: SetupDeps;
	notes: string[];
	/** This migration's marker, when it has one. */
	marker: Marker | undefined;
}

/** A numbered migration (0466); `plan` returns undefined when it doesn't apply here or can't run now (a note says why). */
interface Migration {
	id: number;
	slug: string;
	plan(context: PlanContext): Step | undefined;
}

/** A migration's marker name, `<NNNN>-<slug>`. */
function markerName(migration: Migration): string {
	return `${String(migration.id).padStart(4, "0")}-${migration.slug}`;
}

/** omp's request timeout for the boss's MCP server (0484): his Allow may take 2 minutes, then the action runs. */
const BOSS_MCP_TIMEOUT_MS = 300_000;

/** A stdio MCP server running this Jocasta with `args`. */
function jocastaServer(paths: SetupPaths, args: string[]): Record<string, unknown> {
	const [program = "jocasta", ...prefix] = paths.jocastaCommand;
	return { type: "stdio", command: program, args: [...prefix, ...args] };
}

/** Jocasta's servers in an omp `mcp.json`; `owned` names every server uninstall removes there (0468). */
function mcpStep(path: string, servers: Record<string, Record<string, unknown>>, owned: string[]): Step {
	return {
		path,
		fingerprint: fingerprint(JSON.stringify(servers)),
		change: mcpEntry(path, servers),
		undo: [{ kind: "mcp", path, servers: owned }],
		servers,
	};
}

type KeysUndo = Extract<Undo, { kind: "keys" }>;

/**
 * Jocasta's keys merged into one of his omp files (`keybindings.yml` YAML, `lsp.json` JSON; 0479) as `config.yml`'s are
 * (0466): a missing key added, one still at Jocasta's earlier value following the new one, his own value kept and shown;
 * his other keys untouched. The undo names only the keys setup added, and whether setup made the file.
 */
function keysStep(title: string, path: string, shipped: Record<string, unknown>, marker: Marker | undefined): Step {
	const yaml = !path.endsWith(".json");
	const has = readText(path);
	const his =
		has === undefined ? {} : yaml ? readYamlObject(path, has) : has.trim() === "" ? {} : readJsonObject(path, has);
	const claimed = marker?.undo.find((u): u is KeysUndo => u.kind === "keys" && u.path === path);
	const keys: Record<string, unknown> = {};
	for (const [key, value] of Object.entries(shipped)) {
		const yours = his[key];
		if (yours === undefined || (key in (claimed?.keys ?? {}) && Bun.deepEquals(yours, claimed?.keys[key], true))) {
			keys[key] = value;
		}
	}
	const step: Step = {
		path,
		fingerprint: fingerprint(JSON.stringify(shipped)),
		undo:
			Object.keys(keys).length > 0
				? [{ kind: "keys", path, keys, created: claimed?.created ?? has === undefined }]
				: [],
		shipped,
	};
	const { merged, changed, kept } = mergeOmpSettings(his, shipped, asObject(marker?.shipped));
	if (!changed && kept.length === 0) return step;
	const after = yaml ? ompYaml(merged) : json(merged);
	if (has === undefined) {
		step.change = newFile(title, path, after);
		return step;
	}
	step.change = {
		title: kept.length > 0 ? `${title} (yours kept)` : title,
		path,
		diff: changed ? lineDiff(has, after) : [],
		kept: kept.map((k) => `${k.key}: yours ${JSON.stringify(k.yours)} kept (Jocasta ships ${JSON.stringify(k.ships)})`),
		...(changed ? {} : { keepsOnly: true as const }),
		apply: async () => {
			if (changed) writeAtomically(path, after);
		},
	};
	return step;
}

/** The migrations, in the order they apply. A new one takes the next number; numbers are never reused. */
const MIGRATIONS: Migration[] = [
	{
		// Everyone's `graph_search` (0389) and whiteboard's own tools for drawing (0393, on the service's whiteboard
		// state, telemetry off).
		id: 1,
		slug: "mcp",
		plan: ({ paths, notes }) => {
			const servers: Record<string, Record<string, unknown>> = { jocasta: jocastaServer(paths, ["mcp"]) };
			if (paths.whiteboard) {
				// whiteboard mcp finds the service's whiteboard server (its Desktop server, instance `stable`) through
				// DEV_REVIEW_HOME.
				servers.whiteboard = {
					type: "stdio",
					command: paths.whiteboard.node,
					args: [paths.whiteboard.cli, "mcp"],
					env: { DEV_REVIEW_HOME: join(paths.dataDir, "whiteboard"), ...WHITEBOARD_ENV },
				};
			} else {
				notes.push("whiteboard isn't built: run `bun run build` (or install a release), then `jocasta setup` again.");
			}
			return mcpStep(join(paths.ompAgentDir, "mcp.json"), servers, ["jocasta", "whiteboard"]);
		},
	},
	{
		// The boss's tools only in its own folder (0392: project-level MCP config). A call may wait up to 2 minutes for
		// his Allow (0484), past omp's 30 s request timeout: 5 minutes leaves the action itself time too.
		id: 2,
		slug: "boss-mcp",
		plan: ({ paths }) => {
			const step = mcpStep(
				join(paths.dataDir, "home", ".omp", "mcp.json"),
				{ jocasta: { ...jocastaServer(paths, ["mcp", "--boss"]), timeout: BOSS_MCP_TIMEOUT_MS } },
				["jocasta"],
			);
			if (step.change) step.change.title = `the boss's MCP server "jocasta" (--boss)`;
			return step;
		},
	},
	{
		// The omp settings Jocasta ships (0396–0400, 0447–0465): the whole file when he has none, else merged key by key.
		id: 3,
		slug: "omp-settings",
		plan: ({ paths, marker }) => {
			const path = join(paths.ompAgentDir, "config.yml");
			const text = readFileSync(join(paths.assetsDir, "omp-config.yml"), "utf8");
			const shipped = asObject(Bun.YAML.parse(text)) ?? {};
			const step: Step = { path, fingerprint: fingerprint(text), undo: [], shipped };
			const has = readText(path);
			if (has === undefined) {
				step.change = newFile("omp settings", path, text);
				return step;
			}
			const his = readYamlObject(path, has);
			const { merged, changed, kept } = mergeOmpSettings(his, shipped, asObject(marker?.shipped));
			if (!changed && kept.length === 0) return step;
			const after = ompYaml(merged);
			step.change = {
				title:
					kept.length > 0
						? "omp settings Jocasta ships (yours kept where you changed them)"
						: "omp settings Jocasta ships",
				path,
				diff: changed ? lineDiff(has, after) : [],
				kept: kept.map(
					(k) => `${k.key}: yours ${JSON.stringify(k.yours)} kept (Jocasta ships ${JSON.stringify(k.ships)})`,
				),
				apply: async () => {
					if (changed) writeAtomically(path, after);
				},
			};
			return step;
		},
	},
	{
		// herdr's omp integration reports each pane's agent state (0388). One that was there before setup (another
		// install's, his own) isn't setup's to undo (0468).
		id: 4,
		slug: "herdr-integration",
		plan: ({ paths, deps, notes, marker }) => {
			const path = join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION);
			const claimed = marker?.undo.some((u) => u.kind === "herdr-integration" && u.path === path) === true;
			const step: Step = {
				path,
				fingerprint: "herdr integration install omp",
				undo: [{ kind: "herdr-integration", path }],
			};
			if (existsSync(path)) {
				if (!claimed) step.undo = [];
				return step;
			}
			const herdr = deps.herdr;
			if (!herdr) {
				notes.push("herdr isn't installed: install it from https://herdr.dev, then run `jocasta setup` again.");
				return undefined;
			}
			step.change = {
				title: "herdr's omp integration (`herdr integration install omp`)",
				path,
				diff: [`+ ${path} (written by herdr)`],
				apply: async () => {
					const { code, output } = await deps.run([herdr, "integration", "install", "omp"]);
					if (code !== 0) throw new UserError(`herdr integration install omp exited ${code}: ${output.trim()}`, 1);
				},
			};
			return step;
		},
	},
	{
		// Jocasta's own herdr config follows the shipped template (0440).
		id: 5,
		slug: "herdr-config",
		plan: ({ paths }) => {
			const path = join(paths.jocastaConfigDir, "herdr.toml");
			const wanted = herdrConfig(readFileSync(join(paths.assetsDir, "herdr.toml"), "utf8"), paths.dataDir);
			const exists = existsSync(path);
			const change = ownedFile(
				exists ? "Jocasta's herdr config, to the shipped template" : "herdr config for session `jocasta`",
				path,
				wanted,
			);
			return { path, fingerprint: fingerprint(wanted), change, undo: [{ kind: "file", path }] };
		},
	},
	{
		// Pi's settings.json `frontier`/`pollers` keys moved to jocasta.yaml once (0387).
		id: 6,
		slug: "jocasta-yaml",
		plan: ({ paths }) => {
			const path = join(paths.jocastaConfigDir, "jocasta.yaml");
			const step: Step = { path, fingerprint: "settings.json", undo: [] };
			if (existsSync(path)) return step;
			const yaml = jocastaYaml(join(paths.jocastaConfigDir, "settings.json"));
			if (yaml) step.change = newFile("Jocasta settings", path, yaml);
			return step;
		},
	},
	{
		// The Tokyo Night scheme for Windows Terminal (0401, 0474).
		id: 7,
		slug: "terminal-scheme",
		plan: ({ paths, marker }) => {
			if (!paths.terminalFragmentsDir) return undefined;
			const dir = join(paths.terminalFragmentsDir, "jocasta");
			const path = join(dir, "jocasta.json");
			const wanted = json({ schemes: [TOKYO_NIGHT_SCHEME] });
			return {
				path,
				fingerprint: fingerprint(wanted),
				change: ownedFile("Windows Terminal scheme Tokyo Night", path, wanted),
				undo: [folderUndo(dir, "jocasta.json", marker)],
			};
		},
	},
	{
		// mise's machine file (0476), only when mise is installed.
		id: 8,
		slug: "mise-config",
		plan: ({ paths, deps }) => {
			if (!deps.mise) return undefined;
			const path = join(paths.miseConfigDir, "conf.d", "jocasta.toml");
			return {
				path,
				fingerprint: fingerprint(MISE_FRAGMENT),
				change: ownedFile("mise's pins and settings for Jocasta's tools", path, MISE_FRAGMENT),
				undo: [{ kind: "file", path }],
			};
		},
	},
	{
		// The mise skill for agents (0476), in omp's user skills.
		id: 9,
		slug: "mise-skill",
		plan: ({ paths, marker }) => {
			const dir = join(paths.ompAgentDir, "skills", "mise");
			const path = join(dir, "SKILL.md");
			const wanted = readFileSync(join(paths.assetsDir, "skills", "mise", "SKILL.md"), "utf8");
			return {
				path,
				fingerprint: fingerprint(wanted),
				change: ownedFile("omp skill mise", path, wanted),
				undo: [folderUndo(dir, "SKILL.md", marker)],
			};
		},
	},
	{
		// Ctrl+L no longer starts omp's live voice (0451, 0479): `app.live.toggle` unbound; F9, which omp and herdr leave
		// unused, toggles omp's dictation (0556): Right Ctrl sends it to the boss pane through herdr. Both in the file omp
		// reads (`keybindings.yml`, else his `keybindings.yaml`; pi-tui `app-keybindings.ts`).
		id: 10,
		slug: "omp-keybindings",
		plan: ({ paths, notes, marker }) => {
			const yml = join(paths.ompAgentDir, "keybindings.yml");
			const yaml = join(paths.ompAgentDir, "keybindings.yaml");
			if (!existsSync(yml) && !existsSync(yaml) && existsSync(join(paths.ompAgentDir, "keybindings.json"))) {
				notes.push(
					"omp's keybindings.json is in its old format: start omp once (it moves it to keybindings.yml), then run `jocasta setup` again.",
				);
				return undefined;
			}
			const path = !existsSync(yml) && existsSync(yaml) ? yaml : yml;
			return keysStep(
				"omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation",
				path,
				{ "app.live.toggle": [], [STT_TOGGLE_SETTING]: STT_TOGGLE_KEY },
				marker,
			);
		},
	},
	{
		// The advisor's brief (0451, 0479): written when missing, following Jocasta's text while it is still Jocasta's;
		// his own is shown and kept.
		id: 11,
		slug: "omp-watchdog",
		plan: ({ paths, marker }) => {
			const path = join(paths.ompAgentDir, "WATCHDOG.md");
			const wanted = readFileSync(join(paths.assetsDir, "WATCHDOG.md"), "utf8");
			const ours = fingerprint(wanted);
			const step: Step = { path, fingerprint: ours, undo: [{ kind: "text", path, fingerprint: ours }] };
			const title = "the advisor's brief";
			const has = readText(path);
			if (has === undefined) {
				step.change = newFile(title, path, wanted);
				return step;
			}
			if (has === wanted) return step;
			const before = marker?.undo.find((u) => u.kind === "text" && u.path === path);
			if (before?.kind === "text" && before.fingerprint === fingerprint(has)) {
				step.change = { title, path, diff: lineDiff(has, wanted), apply: async () => writeAtomically(path, wanted) };
				return step;
			}
			step.undo = [];
			step.change = {
				title: `${title} (yours kept; Jocasta's differs as shown)`,
				path,
				diff: lineDiff(has, wanted),
				kept: ["yours stays as it is, never overwritten (Jocasta's text is config/WATCHDOG.md)"],
				keepsOnly: true,
				apply: async () => {},
			};
			return step;
		},
	},
	{
		// Idle language servers stop after 5 minutes (0451, 0479); each pane holds its own.
		id: 12,
		slug: "omp-lsp",
		plan: ({ paths, marker }) =>
			keysStep(
				"omp language servers: idle ones stop after 5 minutes",
				join(paths.ompAgentDir, "lsp.json"),
				{ idleTimeoutMs: 300000 },
				marker,
			),
	},
	{
		// No autostart (0557): the logon Run value the companion wrote (0367) goes; he starts `jocasta` himself. Only a
		// value that starts Jocasta's companion launcher: another program's value of that name stays.
		id: 13,
		slug: "autostart-off",
		plan: ({ deps }) => {
			const data = deps.runValue?.read(RUN_VALUE);
			if (data === undefined || !startsCompanion(data)) return undefined;
			const path = `${RUN_KEY}\\${RUN_VALUE}`;
			return {
				path,
				fingerprint: fingerprint(path),
				undo: [],
				change: {
					title: "Jocasta no longer starts at logon: its Run value goes",
					path,
					diff: [`- ${RUN_VALUE}  ${data}`],
					apply: async () => {
						if (!deps.runValue?.remove(RUN_VALUE)) throw new Error(`${path} wasn't removed`);
					},
				},
			};
		},
	},
	{
		// The `jocasta` herdr plugin (0553): its manifest with how herdr starts this Jocasta, linked into herdr with
		// `herdr plugin link`, then herdr's config reloaded so its key bindings find the actions (0558).
		id: 14,
		slug: "herdr-plugin",
		plan: ({ paths, deps, marker }) => {
			const herdr = deps.herdr;
			// Migration 4 says herdr is missing.
			if (!herdr) return undefined;
			const dir = join(paths.jocastaConfigDir, "herdr-plugin");
			const path = join(dir, PLUGIN_MANIFEST);
			const wanted = pluginManifest(
				readFileSync(join(paths.assetsDir, "herdr-plugin", PLUGIN_MANIFEST), "utf8"),
				paths.jocastaCommand,
			);
			const link = async () => {
				const { code, output } = await deps.run([herdr, "plugin", "link", dir]);
				if (code !== 0 && !/already/i.test(output)) {
					throw new UserError(`herdr plugin link exited ${code}: ${output.trim()}`, 1);
				}
				// A running herdr takes the plugin's key bindings after a reload (0558); with none running, the next start does.
				await deps.reloadHerdr?.();
			};
			const file = ownedFile(
				existsSync(path) ? "Jocasta's herdr plugin, to the shipped manifest" : "the `jocasta` herdr plugin",
				path,
				wanted,
			);
			return {
				path,
				fingerprint: fingerprint(wanted),
				undo: [{ kind: "herdr-plugin", id: "jocasta", path: dir }, folderUndo(dir, PLUGIN_MANIFEST, marker)],
				change: {
					title: file?.title ?? "the `jocasta` herdr plugin, linked",
					path,
					diff: [...(file?.diff ?? []), `+ herdr plugin link ${dir}`],
					apply: async () => {
						await file?.apply();
						await link();
					},
				},
			};
		},
	},
];

/** The migrations' steps now; `force` plans those with a current marker too (`--refresh`). */
function planMigrations(
	paths: SetupPaths,
	deps: SetupDeps,
	notes: string[],
	force: boolean,
): (PlannedChange & { slug: string; step: Step })[] {
	const planned: (PlannedChange & { slug: string; step: Step })[] = [];
	for (const migration of MIGRATIONS) {
		const name = markerName(migration);
		const marker = readMarker(paths.jocastaConfigDir, name);
		const step = migration.plan({ paths, deps, notes, marker });
		if (!step || (!force && marker?.fingerprint === step.fingerprint)) continue;
		planned.push({
			name,
			slug: migration.slug,
			step,
			path: step.path,
			...(step.change ? { change: step.change } : {}),
			marker: { fingerprint: step.fingerprint, undo: step.undo, ...(step.shipped ? { shipped: step.shipped } : {}) },
		});
	}
	return planned;
}

/** Every change setup would make now, and what it can't do. Reads only. */
export function planSetup(paths: SetupPaths, deps: SetupDeps): SetupPlan {
	const notes: string[] = [];
	const planned = planMigrations(paths, deps, notes, false);
	const changes: PlannedChange[] = planned.filter((p) => p.change);
	if (deps.calendar?.fromEnv && !deps.calendar.stored()) {
		const { calendar } = deps;
		const secret = calendar.fromEnv as string;
		const path = `Credential Manager: ${CALENDAR_CREDENTIAL}`;
		changes.push({
			name: "calendar",
			path,
			change: {
				title: `the calendar's secret address from ${CALENDAR_ENV} into Windows Credential Manager`,
				path,
				diff: [`+ ${CALENDAR_CREDENTIAL} (the address itself isn't shown)`],
				apply: async () => calendar.save(secret),
			},
			marker: CALENDAR_MARKER,
		});
	}
	return { changes, settled: planned.filter((p) => !p.change), notes };
}

const CALENDAR_MARKER: PlannedChange["marker"] = {
	fingerprint: CALENDAR_CREDENTIAL,
	undo: [{ kind: "credential", target: CALENDAR_CREDENTIAL }],
};

/**
 * An omp `mcp.json` undo with what setup wrote there (0468): each server goes only while it still is that. A marker
 * names only the servers and a fingerprint of the entries written, so the entries setup writes today stand in for them.
 */
export type McpUndo = Extract<Undo, { kind: "mcp" }> & {
	/** The entries setup writes there today, by name. */
	writes: Record<string, unknown>;
	/** The marker's fingerprint of the entries it wrote: present entries that still match it are setup's too. */
	wrote: string;
};

/** One thing setup added, as uninstall undoes it. */
export type SetupUndo = Exclude<Undo, { kind: "mcp" }> | McpUndo;

/**
 * How to undo what setup added on this machine (0468): only what its markers recorded (a step without its marker
 * isn't setup's to undo), the calendar's secret while it is stored. Reads only.
 */
export function setupUndo(paths: SetupPaths, calendarStored: boolean): SetupUndo[] {
	const deps: SetupDeps = { herdr: undefined, mise: "mise", run: async () => ({ code: 1, output: "read only" }) };
	const written = new Map<string, { writes: Record<string, unknown>; wrote: string }>();
	for (const migration of MIGRATIONS) {
		const marker = readMarker(paths.jocastaConfigDir, markerName(migration));
		if (!marker?.undo.some((u) => u.kind === "mcp")) continue;
		let writes: Record<string, unknown> = {};
		try {
			writes = migration.plan({ paths, deps, notes: [], marker })?.servers ?? {};
		} catch {
			// An mcp.json setup can't read: uninstall says so and leaves it to him.
		}
		for (const u of marker.undo) {
			if (u.kind === "mcp") written.set(normalizePathKey(u.path), { writes, wrote: marker.fingerprint });
		}
	}
	const undo: SetupUndo[] = [];
	for (const item of recordedUndo(paths.jocastaConfigDir)) {
		if (item.kind === "credential" && !calendarStored) continue;
		if (item.kind === "mcp") {
			undo.push({ ...item, ...(written.get(normalizePathKey(item.path)) ?? { writes: {}, wrote: "" }) });
		} else {
			undo.push(item);
		}
	}
	// A file in a folder that goes whole goes with it; folders go after files, so one setup may not have made goes
	// only once it is empty (a marker from before 0484).
	const folders = undo.flatMap((item) => (item.kind === "dir" && item.file !== undefined ? [item.path] : []));
	const inFolder = (item: SetupUndo) =>
		item.kind === "file" && folders.some((dir) => normalizePathKey(dirname(item.path)) === normalizePathKey(dir));
	const seen = new Set<string>();
	const unique = undo.filter((item) => {
		const key = JSON.stringify(item).toLowerCase();
		if (seen.has(key) || inFolder(item)) return false;
		seen.add(key);
		return true;
	});
	return [...unique.filter((item) => item.kind !== "dir"), ...unique.filter((item) => item.kind === "dir")];
}

export interface SetupIo {
	print(line: string): void;
	/** Asks one question; resolves his answer, or undefined at end of input. */
	ask(question: string): Promise<string | undefined>;
}

/** Whether he answered yes (`y` or `yes`, any case). */
export const answeredYes = (answer: string | undefined) => ["y", "yes"].includes(answer?.trim().toLowerCase() ?? "");

function mark(paths: SetupPaths, planned: PlannedChange): void {
	writeMarker(paths.jocastaConfigDir, planned.name, { ...planned.marker, appliedAt: new Date().toISOString() });
}

function printChange(io: SetupIo, change: SetupChange): void {
	io.print("");
	io.print(`${change.title}: ${change.path}`);
	for (const line of change.diff) io.print(line);
	for (const line of change.kept ?? []) io.print(`  kept: ${line}`);
}

/**
 * Applies `changes` in order, each marked once written; the first failure stops the rest and says why. Resolves
 * whether all were applied.
 */
async function applyInOrder(paths: SetupPaths, changes: PlannedChange[], io: SetupIo): Promise<boolean> {
	for (const [index, planned] of changes.entries()) {
		const change = planned.change as SetupChange;
		try {
			await change.apply();
		} catch (error) {
			const waiting = changes.length - index - 1;
			io.print(
				`jocasta setup: ${planned.name} (${change.title}) failed: ${error instanceof Error ? error.message : String(error)}${waiting > 0 ? `; the ${waiting} change${waiting > 1 ? "s" : ""} after it wait` : ""}.`,
			);
			return false;
		}
		mark(paths, planned);
		io.print(`${change.keepsOnly ? "kept" : "wrote"} ${change.path}`);
	}
	return true;
}

/** Shows the plan, asks once, and writes everything only after `y`; then asks for a missing calendar address. */
export async function runSetup(paths: SetupPaths, deps: SetupDeps, io: SetupIo): Promise<number> {
	const { changes, settled, notes } = planSetup(paths, deps);
	for (const note of notes) io.print(note);
	if (changes.length === 0) {
		for (const planned of settled) mark(paths, planned);
		io.print("Nothing to set up: everything Jocasta adds is in place.");
	} else {
		io.print("jocasta setup will make these changes:");
		for (const planned of changes) printChange(io, planned.change as SetupChange);
		io.print("");
		if (!answeredYes(await io.ask("Write these changes? [y/N] "))) {
			io.print("Nothing written.");
			return 0;
		}
		if (!(await applyInOrder(paths, changes, io))) return 1;
		for (const planned of settled) mark(paths, planned);
		io.print("Done: run `jocasta` to open Jocasta.");
	}
	await askCalendar(paths, deps.calendar, io);
	return 0;
}

/**
 * The calendar's secret iCal address (0430), asked once per setup when neither Credential Manager nor the env var has
 * it; Enter skips (the calendar stays off).
 */
async function askCalendar(paths: SetupPaths, calendar: CalendarStore | undefined, io: SetupIo): Promise<void> {
	if (!calendar || calendar.fromEnv || calendar.stored()) return;
	const answer = (await io.ask("Your calendar's secret iCal address, for today's meetings (Enter to skip): "))?.trim();
	if (!answer) {
		io.print("No calendar: run `jocasta setup` again to add it.");
		return;
	}
	if (!/^(https?|webcal):\/\/\S+$/i.test(answer)) {
		io.print("That isn't an http(s) or webcal address; nothing saved. Run `jocasta setup` again to add it.");
		return;
	}
	calendar.save(answer);
	writeMarker(paths.jocastaConfigDir, "calendar", { ...CALENDAR_MARKER, appliedAt: new Date().toISOString() });
	io.print(`saved in Windows Credential Manager as ${CALENDAR_CREDENTIAL}`);
}

/** The files `--refresh` resets (0467), by name, with the migrations that own them. */
const REFRESHABLE: Record<string, string[]> = {
	"herdr.toml": ["herdr-config"],
	"jocasta.json": ["terminal-scheme"],
	"mcp.json": ["mcp", "boss-mcp"],
};

/** `YYYYMMDD-HHMMSS`, local time, for a backup's name. */
export function backupStamp(now = new Date()): string {
	const two = (n: number) => String(n).padStart(2, "0");
	return `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}-${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
}

/**
 * `jocasta setup --refresh <file>` (0467): resets one Jocasta-owned file (`herdr.toml`, the Terminal fragment
 * `jocasta.json`, Jocasta's entries in omp's `mcp.json`, by name or full path) to the shipped version after showing the
 * diff and his `y`, keeping the old one as `<name>.bak.<YYYYMMDD-HHMMSS>` beside it; herdr reloads its config after a
 * herdr.toml refresh.
 */
export async function refreshSetup(
	paths: SetupPaths,
	deps: SetupDeps,
	io: SetupIo,
	file: string,
	now = new Date(),
): Promise<number> {
	const all = planMigrations(paths, deps, [], true).filter((p) => Object.values(REFRESHABLE).flat().includes(p.slug));
	const byName = REFRESHABLE[basename(file).toLowerCase()];
	const target = resolve(file).toLowerCase();
	const chosen = all.filter((p) =>
		file.includes("/") || file.includes("\\") ? p.path.toLowerCase() === target : byName?.includes(p.slug),
	);
	if (chosen.length === 0) {
		throw new UserError(
			`setup --refresh: ${file} isn't a file setup refreshes; use herdr.toml, jocasta.json (the Windows Terminal scheme) or mcp.json (Jocasta's MCP entries)`,
			2,
		);
	}
	for (const planned of chosen) if (!planned.change) io.print(`${planned.path} already is the shipped version.`);
	const changes = chosen.filter((p) => p.change);
	if (changes.length === 0) return 0;
	io.print("jocasta setup --refresh will reset these to the shipped version, keeping a backup beside each:");
	for (const planned of changes) printChange(io, planned.change as SetupChange);
	io.print("");
	if (!answeredYes(await io.ask("Refresh? [y/N] "))) {
		io.print("Nothing written.");
		return 0;
	}
	const stamp = backupStamp(now);
	for (const planned of changes) {
		if (existsSync(planned.path)) {
			const backup = `${planned.path}.bak.${stamp}`;
			copyFileSync(planned.path, backup);
			io.print(`kept ${backup}`);
		}
	}
	if (!(await applyInOrder(paths, changes, io))) return 1;
	if (changes.some((p) => p.slug === "herdr-config")) {
		const reload = deps.reloadHerdr ? await deps.reloadHerdr() : { code: 1, output: "herdr isn't installed" };
		if (reload.code === 0) {
			io.print("herdr reloaded its config.");
		} else {
			const why = reload.output.trim().split(/\r?\n/)[0] || `exit ${reload.code}`;
			io.print(
				`herdr didn't reload its config (${why}): restart herdr to use it (close Jocasta, then run \`jocasta\`).`,
			);
		}
	}
	return 0;
}

/**
 * The service's start check (0466): setup's pending changes, read only, as one toast's text; undefined when none.
 * Never throws: a problem goes to Jocasta's log.
 */
export async function setupToast(): Promise<string | undefined> {
	try {
		const { changes } = planSetup(defaultSetupPaths(shippedConfigDir(), selfCommand()), {
			herdr: findHerdr(),
			mise: Bun.which("mise") ?? undefined,
			run: async () => ({ code: 1, output: "read only" }),
			calendar: defaultCalendarStore(undefined),
		});
		const n = changes.length;
		if (n === 0) return undefined;
		return `jocasta setup has ${n} change${n > 1 ? "s" : ""} waiting: run \`jocasta setup\` to see ${n > 1 ? "them" : "it"}.`;
	} catch (error) {
		log("warn", "setup", `checking for setup changes: ${error instanceof Error ? error.message : String(error)}`);
		return undefined;
	}
}
