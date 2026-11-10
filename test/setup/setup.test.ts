import { afterEach, beforeEach, expect, test } from "bun:test";
import {
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	realpathSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { editConfig } from "../../src/setup/config-file.ts";
import { lineDiff } from "../../src/setup/diff.ts";
import { fingerprint, markersDir, readMarker } from "../../src/setup/markers.ts";
import { MISE_FRAGMENT } from "../../src/setup/mise.ts";
import { mergeOmpSettings, ompSettingsText, ompYaml } from "../../src/setup/omp-settings.ts";
import {
	backupStamp,
	type CalendarStore,
	HERDR_OMP_EXTENSION,
	herdrConfig,
	type RunProgram,
	refreshSetup,
	runSetup,
	type SetupDeps,
	type SetupPaths,
	setupToast,
	setupUndo,
	TOKYO_NIGHT_SCHEME,
} from "../../src/setup/setup.ts";
import { UserError } from "../../src/shared/errors.ts";
import { removeTree } from "../../src/shared/fsx.ts";

const ASSETS = resolve(import.meta.dir, "..", "..", "config");
let root = "";
let paths: SetupPaths;
let ran: string[][];
let run: RunProgram;
let extra: Partial<SetupDeps>;

beforeEach(() => {
	root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-setup-")));
	paths = {
		ompAgentDir: join(root, "omp", "agent"),
		jocastaConfigDir: join(root, "jocasta", "agent"),
		dataDir: join(root, "data"),
		assetsDir: ASSETS,
		terminalFragmentsDir: join(root, "Fragments"),
		miseConfigDir: join(root, "mise"),
		jocastaCommand: ["C:\\Jocasta\\jocasta.exe"],
		whiteboard: { node: "C:\\Jocasta\\node\\node.exe", cli: "C:\\Jocasta\\whiteboard\\cli.js" },
	};
	ran = [];
	extra = {};
	// Stands in for herdr: writes the extension file the real install writes.
	run = async (argv) => {
		ran.push(argv);
		const file = join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION);
		mkdirSync(dirname(file), { recursive: true });
		writeFileSync(file, "// herdr\n");
		return { code: 0, output: "" };
	};
});
afterEach(() => removeTree(root));

const put = (path: string, text: string) => {
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, text);
};

/** A copy of the shipped config/ the test may change. */
function ownAssets(): string {
	const dir = join(root, "assets");
	cpSync(ASSETS, dir, { recursive: true });
	paths.assetsDir = dir;
	return dir;
}

function io(answers: (string | undefined)[]) {
	const out: string[] = [];
	const asked: string[] = [];
	return {
		out,
		asked,
		io: {
			print: (l: string) => void out.push(l),
			ask: async (q: string) => {
				asked.push(q);
				return answers.shift();
			},
		},
	};
}

async function setup(answer: string | undefined, herdr: string | null = "herdr.exe") {
	const { out, asked, io: setupIo } = io([answer]);
	const code = await runSetup(paths, { herdr: herdr ?? undefined, run, ...extra }, setupIo);
	return { code, text: out.join("\n"), asked };
}

async function refresh(file: string, answer: string | undefined) {
	const { out, io: setupIo } = io([answer]);
	const code = await refreshSetup(
		paths,
		{ herdr: "herdr.exe", run, ...extra },
		setupIo,
		file,
		new Date(2026, 9, 23, 9, 5, 7),
	);
	return { code, text: out.join("\n") };
}

const globalMcp = () => join(paths.ompAgentDir, "mcp.json");
const bossMcp = () => join(paths.dataDir, "home", ".omp", "mcp.json");
const ompConfig = () => join(paths.ompAgentDir, "config.yml");
const herdrToml = () => join(paths.jocastaConfigDir, "herdr.toml");
const whiteboardServer = () => ({
	type: "stdio",
	command: "C:\\Jocasta\\node\\node.exe",
	args: ["C:\\Jocasta\\whiteboard\\cli.js", "mcp"],
	env: {
		DEV_REVIEW_HOME: join(paths.dataDir, "whiteboard"),
		DO_NOT_TRACK: "1",
		DEV_FAST_REVIEW_TELEMETRY_DISABLED: "1",
	},
});
const fragment = () => join(root, "Fragments", "jocasta", "jocasta.json");
const miseSkill = () => join(paths.ompAgentDir, "skills", "mise", "SKILL.md");
const miseFile = () => join(paths.miseConfigDir, "conf.d", "jocasta.toml");
const keybindings = () => join(paths.ompAgentDir, "keybindings.yml");
const watchdog = () => join(paths.ompAgentDir, "WATCHDOG.md");
const lsp = () => join(paths.ompAgentDir, "lsp.json");
const secrets = () => join(paths.ompAgentDir, "secrets.yml");

test("a fresh machine: the diff names every file, and `n` writes nothing", async () => {
	put(join(paths.jocastaConfigDir, "settings.json"), JSON.stringify({ frontier: { port: 47615 }, theme: "x" }));
	const { code, text, asked } = await setup("n");
	expect(code).toBe(0);
	expect(asked).toEqual(["Write these changes? [y/N] "]);
	expect(text).toContain(`omp MCP servers "jocasta", "whiteboard": ${globalMcp()}`);
	expect(text).toContain(`the boss's MCP server "jocasta" (--boss): ${bossMcp()}`);
	expect(text).toContain(`omp settings: ${ompConfig()}`);
	expect(text).toContain(`+ ${join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION)} (written by herdr)`);
	expect(text).toContain(`herdr config for session \`jocasta\`: ${herdrToml()}`);
	expect(text).toContain(`Jocasta settings: ${join(paths.jocastaConfigDir, "jocasta.yaml")}`);
	expect(text).toContain(`Windows Terminal scheme Tokyo Night: ${fragment()}`);
	expect(text).toContain(`omp skill mise: ${miseSkill()}`);
	expect(text).toContain(
		`omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation: ${keybindings()}`,
	);
	expect(text).toContain(`the advisor's brief: ${watchdog()}`);
	expect(text).toContain(`omp language servers: idle ones stop after 5 minutes: ${lsp()}`);
	expect(text).toContain(`omp's secrets.yml: your own strings to hide, an empty list to fill: ${secrets()}`);
	// No mise on this machine: no mise file (0476).
	expect(text).not.toContain(miseFile());
	expect(text).toEndWith("Nothing written.");
	expect(existsSync(join(root, "omp"))).toBe(false);
	expect(existsSync(paths.dataDir)).toBe(false);
	expect(existsSync(markersDir(paths.jocastaConfigDir))).toBe(false);
	expect(existsSync(herdrToml())).toBe(false);
	expect(existsSync(fragment())).toBe(false);
	expect(ran).toEqual([]);
	// End of input is a no too.
	expect((await setup(undefined)).text).toEndWith("Nothing written.");
});

test("`y` writes every file, marks each migration, runs herdr's omp integration, and a second run has nothing to do", async () => {
	put(
		join(paths.jocastaConfigDir, "settings.json"),
		JSON.stringify({
			frontier: { quietHours: { from: "22:00", to: "07:00" } },
			pollers: { github: { enabled: true } },
		}),
	);
	const { code, text } = await setup("y");
	expect(code).toBe(0);
	expect(text).toContain(`wrote ${globalMcp()}`);
	expect(JSON.parse(readFileSync(globalMcp(), "utf8"))).toEqual({
		mcpServers: {
			jocasta: { type: "stdio", command: "C:\\Jocasta\\jocasta.exe", args: ["mcp"], timeout: 1_860_000 },
			whiteboard: whiteboardServer(),
		},
	});
	// omp's 30 s request timeout is raised past the tools' own bounds (0577): a thread's 30-minute `check` plus a minute,
	// the boss's calls more.
	expect(JSON.parse(readFileSync(bossMcp(), "utf8"))).toEqual({
		mcpServers: {
			jocasta: { type: "stdio", command: "C:\\Jocasta\\jocasta.exe", args: ["mcp", "--boss"], timeout: 2_100_000 },
		},
	});
	expect(readFileSync(ompConfig(), "utf8")).toBe(readFileSync(join(ASSETS, "omp-config.yml"), "utf8"));
	const pluginDir = join(paths.jocastaConfigDir, "herdr-plugin");
	expect(ran).toEqual([
		["herdr.exe", "integration", "install", "omp"],
		["herdr.exe", "plugin", "link", pluginDir],
	]);
	// The plugin's manifest names this Jocasta's command (0553).
	const manifest = Bun.TOML.parse(readFileSync(join(pluginDir, "herdr-plugin.toml"), "utf8")) as {
		id: string;
		panes: { id: string; command: string[] }[];
	};
	expect(manifest.id).toBe("jocasta");
	expect(manifest.panes.map((p) => [p.id, p.command])).toEqual([
		["today", ["C:\\Jocasta\\jocasta.exe", "plugin", "today"]],
		["ask", ["C:\\Jocasta\\jocasta.exe", "plugin", "ask"]],
	]);
	const toml = readFileSync(herdrToml(), "utf8");
	expect(Bun.TOML.parse(toml)).toMatchObject({
		ui: {
			tab_bar_right: [
				{
					command: process.platform === "win32" ? `type "${paths.dataDir}\\hud.txt"` : `cat '${paths.dataDir}/hud.txt'`,
				},
				{ type: "datetime" },
			],
		},
	});
	expect(() => Bun.TOML.parse(toml)).not.toThrow();
	expect(Bun.YAML.parse(readFileSync(join(paths.jocastaConfigDir, "jocasta.yaml"), "utf8"))).toEqual({
		frontier: { quietHours: { from: "22:00", to: "07:00" } },
		pollers: { github: { enabled: true } },
	});
	expect(JSON.parse(readFileSync(fragment(), "utf8"))).toEqual({ schemes: [TOKYO_NIGHT_SCHEME] });
	expect(readFileSync(miseSkill(), "utf8")).toBe(readFileSync(join(ASSETS, "skills", "mise", "SKILL.md"), "utf8"));
	expect(readFileSync(keybindings(), "utf8")).toBe(ompYaml({ "app.live.toggle": [], "app.stt.toggle": "f9" }));
	expect(readFileSync(watchdog(), "utf8")).toBe(readFileSync(join(ASSETS, "WATCHDOG.md"), "utf8"));
	expect(JSON.parse(readFileSync(lsp(), "utf8"))).toEqual({ idleTimeoutMs: 300000 });
	expect(readFileSync(secrets(), "utf8")).toBe(readFileSync(join(ASSETS, "secrets.yml"), "utf8"));
	expect(readdirSync(markersDir(paths.jocastaConfigDir)).sort()).toEqual([
		"0001-mcp.json",
		"0002-boss-mcp.json",
		"0003-omp-settings.json",
		"0004-herdr-integration.json",
		"0005-herdr-config.json",
		"0006-jocasta-yaml.json",
		"0007-terminal-scheme.json",
		"0009-mise-skill.json",
		"0010-omp-keybindings.json",
		"0011-omp-watchdog.json",
		"0012-omp-lsp.json",
		"0014-herdr-plugin.json",
		"0015-omp-secrets.json",
	]);

	const again = await setup("y");
	expect(again.text).toBe("Nothing to set up: everything Jocasta adds is in place.");
	expect(ran).toHaveLength(2);
});

test("with mise installed setup writes Jocasta's mise file: exact pins and the private, paranoid settings (0476)", async () => {
	extra = { mise: "mise.exe" };
	const { text } = await setup("y");
	expect(text).toContain(`mise's pins and settings for Jocasta's tools: ${miseFile()}`);
	expect(readFileSync(miseFile(), "utf8")).toBe(MISE_FRAGMENT);
	expect(Bun.TOML.parse(MISE_FRAGMENT)).toEqual({
		min_version: "2026.10.12",
		settings: {
			use_versions_host: false,
			use_versions_host_track: false,
			auto_install: false,
			paranoid: ["config"],
			upgrade: { auto_prune: false },
		},
		tools: { herdr: "0.9.5", hunk: "0.23.0", gh: "2.92.0", "github:DeusData/codebase-memory-mcp": "0.12.1" },
	});
	// His own config.toml in the same folder is never touched.
	expect(existsSync(join(paths.miseConfigDir, "config.toml"))).toBe(false);
});

test("an existing mcp.json keeps his other servers and keys; only Jocasta's entries change", async () => {
	const his = {
		mcpServers: {
			github: { type: "stdio", command: "gh-mcp", args: [] },
			jocasta: { type: "stdio", command: "old\\jocasta.exe", args: ["mcp"] },
		},
		disabledServers: ["github"],
	};
	put(globalMcp(), `${JSON.stringify(his, null, 2)}\n`);
	const { text } = await setup("y");
	expect(text).toContain('-       "command": "old\\\\jocasta.exe",');
	expect(text).toContain('+       "command": "C:\\\\Jocasta\\\\jocasta.exe",');
	expect(text).toContain('        "command": "gh-mcp",');
	expect(JSON.parse(readFileSync(globalMcp(), "utf8"))).toEqual({
		mcpServers: {
			github: { type: "stdio", command: "gh-mcp", args: [] },
			jocasta: { type: "stdio", command: "C:\\Jocasta\\jocasta.exe", args: ["mcp"], timeout: 1_860_000 },
			whiteboard: whiteboardServer(),
		},
		disabledServers: ["github"],
	});
});

test("without whiteboard's build setup says so and adds only the jocasta server (0393)", async () => {
	paths.whiteboard = undefined;
	const { text } = await setup("y");
	expect(text).toStartWith("whiteboard isn't built: run `bun run build`");
	expect(text).toContain(`omp MCP server "jocasta": ${globalMcp()}`);
	expect(JSON.parse(readFileSync(globalMcp(), "utf8")).mcpServers.whiteboard).toBeUndefined();
});

test("his config.yml gets the keys Jocasta ships merged in, his other keys untouched and his own values kept and shown (0466)", async () => {
	const assets = ownAssets();
	put(
		join(assets, "omp-config.yml"),
		"# Jocasta's\ncompaction:\n  thresholdPercent: 60\n  idleEnabled: true\nmemory:\n  backend: mnemopi\n",
	);
	put(ompConfig(), "theme:\n  dark: his-theme\ncompaction:\n  thresholdPercent: 45\n");
	const no = await setup("n");
	expect(no.text).toContain(`omp settings Jocasta ships (yours kept where you changed them): ${ompConfig()}`);
	expect(no.text).toContain("  kept: compaction.thresholdPercent: yours 45 kept (Jocasta ships 60)");
	expect(no.text).toMatch(/^\+ {3}idleEnabled: true$/m);
	expect(no.text).toMatch(/^\+ memory:$/m);
	expect(readFileSync(ompConfig(), "utf8")).toBe("theme:\n  dark: his-theme\ncompaction:\n  thresholdPercent: 45\n");

	await setup("y");
	expect(Bun.YAML.parse(readFileSync(ompConfig(), "utf8"))).toEqual({
		theme: { dark: "his-theme" },
		compaction: { thresholdPercent: 45, idleEnabled: true },
		memory: { backend: "mnemopi" },
	});
	// Written as omp writes it, and marked: the same shipped file is never proposed again.
	expect(readFileSync(ompConfig(), "utf8")).toBe(
		ompYaml({
			theme: { dark: "his-theme" },
			compaction: { thresholdPercent: 45, idleEnabled: true },
			memory: { backend: "mnemopi" },
		}),
	);
	expect((await setup("y")).text).not.toContain(ompConfig());

	// A newer shipped value replaces a key still at the value shipped before; his own stays.
	put(
		join(assets, "omp-config.yml"),
		"compaction:\n  thresholdPercent: 70\n  idleEnabled: false\nmemory:\n  backend: mnemopi\n",
	);
	const newer = await setup("y");
	expect(newer.text).toContain("  kept: compaction.thresholdPercent: yours 45 kept (Jocasta ships 70)");
	expect(Bun.YAML.parse(readFileSync(ompConfig(), "utf8"))).toEqual({
		theme: { dark: "his-theme" },
		compaction: { thresholdPercent: 45, idleEnabled: false },
		memory: { backend: "mnemopi" },
	});
});

test("mergeOmpSettings: missing keys added, Jocasta's old values followed, his values kept, lists one value", () => {
	const merged = mergeOmpSettings(
		{ a: 1, b: { c: 2, d: ["x"] }, his: true },
		{ a: 1, b: { c: 3, d: ["y"], e: 4 }, f: { g: 5 } },
		{ b: { c: 2, d: ["z"] } },
	);
	expect(merged).toEqual({
		merged: { a: 1, b: { c: 3, d: ["x"], e: 4 }, his: true, f: { g: 5 } },
		changed: true,
		kept: [{ key: "b.d", yours: ["x"], ships: ["y"] }],
	});
	expect(mergeOmpSettings({ a: 1 }, { a: 1 }, undefined)).toEqual({ merged: { a: 1 }, changed: false, kept: [] });
});

test("jocasta.yaml is never overwritten; the Terminal fragment follows the shipped scheme after `y`", async () => {
	put(join(paths.jocastaConfigDir, "jocasta.yaml"), "his own\n");
	put(join(paths.jocastaConfigDir, "settings.json"), JSON.stringify({ frontier: { port: 1 } }));
	put(fragment(), "his own\n");
	const { text } = await setup("y");
	expect(readFileSync(join(paths.jocastaConfigDir, "jocasta.yaml"), "utf8")).toBe("his own\n");
	expect(text).not.toContain(join(paths.jocastaConfigDir, "jocasta.yaml"));
	expect(text).toContain(`Windows Terminal scheme Tokyo Night: ${fragment()}`);
	expect(text).toContain("- his own");
	expect(JSON.parse(readFileSync(fragment(), "utf8"))).toEqual({ schemes: [TOKYO_NIGHT_SCHEME] });
});

test("the Terminal scheme comes from the one palette file, byte for byte what setup wrote before (0474)", () => {
	expect(TOKYO_NIGHT_SCHEME).toEqual({
		name: "Tokyo Night",
		background: "#1A1B26",
		foreground: "#C0CAF5",
		cursorColor: "#C0CAF5",
		selectionBackground: "#2D3650",
		black: "#24283B",
		red: "#F7768E",
		green: "#9ECE6A",
		yellow: "#E0AF68",
		blue: "#7AA2F7",
		purple: "#BB9AF7",
		cyan: "#7DCFFF",
		white: "#A9B1D6",
		brightBlack: "#414868",
		brightRed: "#F7768E",
		brightGreen: "#9ECE6A",
		brightYellow: "#E0AF68",
		brightBlue: "#7AA2F7",
		brightPurple: "#BB9AF7",
		brightCyan: "#7DCFFF",
		brightWhite: "#C0CAF5",
	});
});

test("a herdr.toml that differs from the shipped template shows its diff and is replaced only after `y` (0440)", async () => {
	const wanted = herdrConfig(readFileSync(join(ASSETS, "herdr.toml"), "utf8"), paths.dataDir);
	const old = wanted.replace("interval_seconds = 5", "interval_seconds = 2");
	put(herdrToml(), old);
	const no = await setup("n");
	expect(no.text).toContain(`Jocasta's herdr config, to the shipped template: ${herdrToml()}`);
	expect(no.text).toMatch(/^- .*interval_seconds = 2/m);
	expect(no.text).toMatch(/^\+ .*interval_seconds = 5/m);
	expect(readFileSync(herdrToml(), "utf8")).toBe(old);
	const yes = await setup("y");
	expect(yes.text).toContain(`wrote ${herdrToml()}`);
	expect(yes.text).toContain("Done: run `jocasta` to open Jocasta.");
	expect(readFileSync(herdrToml(), "utf8")).toBe(wanted);
	expect((await setup("y")).text).not.toContain(herdrToml());
});

test("a migration runs once per shipped version: his later edit stays until Jocasta ships a new one (0466)", async () => {
	const assets = ownAssets();
	await setup("y");
	const edited = `${readFileSync(herdrToml(), "utf8")}# his line\n`;
	put(herdrToml(), edited);
	expect((await setup("y")).text).toBe("Nothing to set up: everything Jocasta adds is in place.");
	expect(readFileSync(herdrToml(), "utf8")).toBe(edited);

	put(join(assets, "herdr.toml"), `${readFileSync(join(assets, "herdr.toml"), "utf8")}# shipped later\n`);
	const newer = await setup("y");
	expect(newer.text).toContain(`Jocasta's herdr config, to the shipped template: ${herdrToml()}`);
	expect(newer.text).toContain("- # his line");
	expect(readFileSync(herdrToml(), "utf8")).toEndWith("# shipped later\n");
	expect(readMarker(paths.jocastaConfigDir, "0005-herdr-config")?.fingerprint).toBe(
		fingerprint(readFileSync(herdrToml(), "utf8")),
	);
});

test("a failed migration stops the rest and says why; the ones before it are marked (0466)", async () => {
	run = async (argv) => {
		ran.push(argv);
		return { code: 3, output: "no omp found\n" };
	};
	const { code, text } = await setup("y");
	expect(code).toBe(1);
	expect(text).toContain(
		"jocasta setup: 0004-herdr-integration (herdr's omp integration (`herdr integration install omp`)) failed: herdr integration install omp exited 3: no omp found; the 8 changes after it wait.",
	);
	expect(existsSync(globalMcp())).toBe(true);
	expect(existsSync(herdrToml())).toBe(false);
	expect(readMarker(paths.jocastaConfigDir, "0003-omp-settings")).toBeDefined();
	expect(readMarker(paths.jocastaConfigDir, "0004-herdr-integration")).toBeUndefined();
});

test("without herdr setup says so and does the rest; a broken mcp.json is refused before anything is written", async () => {
	const { text } = await setup("y", null);
	expect(text).toStartWith("herdr isn't installed: install it from https://herdr.dev");
	expect(ran).toEqual([]);
	expect(existsSync(globalMcp())).toBe(true);
	expect(readMarker(paths.jocastaConfigDir, "0004-herdr-integration")).toBeUndefined();

	rmSync(root, { recursive: true, force: true });
	put(globalMcp(), "{ not json");
	await expect(setup("y")).rejects.toThrow(`${globalMcp()} is not JSON`);
	expect(existsSync(bossMcp())).toBe(false);
});

test("--refresh herdr.toml shows the diff, keeps a dated backup, writes the shipped file and reloads herdr (0467)", async () => {
	await setup("y");
	const shipped = readFileSync(herdrToml(), "utf8");
	put(herdrToml(), `${shipped}# his line\n`);
	const reloads: string[] = [];
	extra = {
		reloadHerdr: async () => {
			reloads.push("reload");
			return { code: 0, output: "" };
		},
	};
	const no = await refresh("herdr.toml", "n");
	expect(no.text).toContain("- # his line");
	expect(no.text).toEndWith("Nothing written.");
	expect(reloads).toEqual([]);

	const yes = await refresh("herdr.toml", "y");
	const backup = `${herdrToml()}.bak.20261023-090507`;
	expect(yes.code).toBe(0);
	expect(yes.text).toContain(`kept ${backup}`);
	expect(yes.text).toEndWith("herdr reloaded its config.");
	expect(readFileSync(backup, "utf8")).toBe(`${shipped}# his line\n`);
	expect(readFileSync(herdrToml(), "utf8")).toBe(shipped);
	expect(reloads).toEqual(["reload"]);
	expect((await refresh(herdrToml(), "y")).text).toBe(`${herdrToml()} already is the shipped version.`);
});

test("--refresh says to restart herdr when it can't reload, resets Jocasta's MCP entries, and refuses other files", async () => {
	await setup("y");
	put(herdrToml(), "# his\n");
	extra = { reloadHerdr: async () => ({ code: 1, output: "error: no running server for session jocasta\n" }) };
	expect((await refresh("herdr.toml", "y")).text).toEndWith(
		"herdr didn't reload its config (error: no running server for session jocasta): restart herdr to use it (close Jocasta, then run `jocasta`).",
	);

	const mcp = JSON.parse(readFileSync(globalMcp(), "utf8"));
	mcp.mcpServers.jocasta.args = ["mcp", "--his"];
	mcp.mcpServers.github = { type: "stdio", command: "gh-mcp" };
	put(globalMcp(), JSON.stringify(mcp));
	const reset = await refresh("mcp.json", "y");
	expect(reset.text).toContain(`${bossMcp()} already is the shipped version.`);
	expect(reset.text).toContain(`kept ${globalMcp()}.bak.20261023-090507`);
	expect(JSON.parse(readFileSync(globalMcp(), "utf8")).mcpServers).toEqual({
		jocasta: { type: "stdio", command: "C:\\Jocasta\\jocasta.exe", args: ["mcp"], timeout: 1_860_000 },
		whiteboard: whiteboardServer(),
		github: { type: "stdio", command: "gh-mcp" },
	});

	const refused = refresh("config.yml", "y");
	await expect(refused).rejects.toBeInstanceOf(UserError);
	await expect(refresh("config.yml", "y")).rejects.toThrow("isn't a file setup refreshes");
});

test("--refresh jocasta.json puts back the Terminal scheme he changed after setup, keeping his copy; herdr isn't reloaded (0467)", async () => {
	await setup("y");
	const shipped = readFileSync(fragment(), "utf8");
	put(fragment(), "his own\n");
	const reloads: string[] = [];
	extra = {
		reloadHerdr: async () => {
			reloads.push("reload");
			return { code: 0, output: "" };
		},
	};
	const yes = await refresh("JOCASTA.JSON", "y");
	const backup = `${fragment()}.bak.20261023-090507`;
	expect(yes.code).toBe(0);
	expect(yes.text).toContain(`Windows Terminal scheme Tokyo Night: ${fragment()}`);
	expect(yes.text).toContain(`kept ${backup}`);
	expect(readFileSync(backup, "utf8")).toBe("his own\n");
	expect(readFileSync(fragment(), "utf8")).toBe(shipped);
	expect(reloads).toEqual([]);
	expect((await refresh(fragment(), "y")).text).toBe(`${fragment()} already is the shipped version.`);
});

test("setup and --refresh write on y or yes in any case and spacing; any other answer writes nothing", async () => {
	for (const answer of ["yep", "ja", "", undefined]) {
		const { code, text } = await setup(answer);
		expect(code).toBe(0);
		expect(text).toEndWith("Nothing written.");
		expect(existsSync(globalMcp())).toBe(false);
	}
	await setup(" YES ");
	expect(existsSync(globalMcp())).toBe(true);
	put(herdrToml(), "# his\n");
	expect((await refresh("herdr.toml", "Y")).text).toContain(`kept ${herdrToml()}.bak.20261023-090507`);
});

test("backupStamp is local YYYYMMDD-HHMMSS", () => {
	expect(backupStamp(new Date(2026, 0, 2, 3, 4, 5))).toBe("20260102-030405");
});

test("setupUndo names what setup added from its markers only: a step without its marker isn't undone (0468)", async () => {
	extra = { mise: "mise.exe" };
	await setup("y");
	const undo = setupUndo(paths, false);
	const global = undo.find((u) => u.kind === "mcp" && u.path === globalMcp());
	expect(global).toEqual({
		kind: "mcp",
		path: globalMcp(),
		servers: ["jocasta", "whiteboard"],
		writes: {
			jocasta: { type: "stdio", command: "C:\\Jocasta\\jocasta.exe", args: ["mcp"], timeout: 1_860_000 },
			whiteboard: whiteboardServer(),
		},
		wrote: readMarker(paths.jocastaConfigDir, "0001-mcp")?.fingerprint as string,
	});
	expect(undo.some((u) => u.kind === "mcp" && u.path === bossMcp() && u.servers.join() === "jocasta")).toBe(true);
	expect(undo).toContainEqual({
		kind: "herdr-integration",
		path: join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION),
	});
	expect(undo).toContainEqual({ kind: "file", path: herdrToml() });
	expect(undo).toContainEqual({ kind: "dir", path: dirname(fragment()), file: "jocasta.json" });
	expect(undo).toContainEqual({ kind: "file", path: miseFile() });
	expect(undo).toContainEqual({ kind: "dir", path: dirname(miseSkill()), file: "SKILL.md" });
	// A file in a folder that goes whole isn't named again (0484).
	expect(undo).not.toContainEqual({ kind: "file", path: miseSkill() });
	// The three omp files Jocasta made (0479): only its keys, and the brief while it is still Jocasta's text.
	expect(undo).toContainEqual({
		kind: "keys",
		path: keybindings(),
		keys: { "app.live.toggle": [], "app.stt.toggle": "f9" },
		created: true,
	});
	expect(undo).toContainEqual({
		kind: "text",
		path: watchdog(),
		fingerprint: fingerprint(readFileSync(join(ASSETS, "WATCHDOG.md"), "utf8")),
	});
	expect(undo).toContainEqual({ kind: "keys", path: lsp(), keys: { idleTimeoutMs: 300000 }, created: true });
	// The plugin is unlinked, then its folder goes (0553).
	const pluginDir = join(paths.jocastaConfigDir, "herdr-plugin");
	expect(undo).toContainEqual({ kind: "herdr-plugin", id: "jocasta", path: pluginDir });
	expect(undo).toContainEqual({ kind: "dir", path: pluginDir, file: "herdr-plugin.toml" });
	// omp's config.yml stays: its keys are his settings.
	expect(JSON.stringify(undo)).not.toContain("config.yml");
	expect(new Set(undo.map((u) => JSON.stringify(u))).size).toBe(undo.length);

	// Without the markers nothing is setup's to undo, whatever is there, and a stored calendar secret only with its
	// marker (B1).
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	expect(setupUndo(paths, true)).toEqual([]);
});

test("herdr's omp integration that was there before setup isn't setup's to undo; one setup installed is (0468)", async () => {
	const extension = join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION);
	const integration = { kind: "herdr-integration", path: extension } as const;
	put(extension, "// another install's\n");
	await setup("y");
	const link = ["herdr.exe", "plugin", "link", join(paths.jocastaConfigDir, "herdr-plugin")];
	expect(ran).toEqual([link]);
	expect(setupUndo(paths, false)).not.toContainEqual(integration);

	rmSync(extension);
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	await setup("y");
	expect(ran).toEqual([link, ["herdr.exe", "integration", "install", "omp"], link]);
	expect(setupUndo(paths, false)).toContainEqual(integration);
});

test("keybindings.yml and lsp.json get only Jocasta's key, his other keys and his own value for it kept (0479)", async () => {
	put(keybindings(), "app.model.cycle:\n  - ctrl+k\n");
	put(lsp(), `${JSON.stringify({ servers: { biome: { disabled: true } }, idleTimeoutMs: 60000 }, null, 2)}\n`);
	const no = await setup("n");
	expect(no.text).toContain(
		`omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation: ${keybindings()}`,
	);
	expect(no.text).toMatch(/^\+ app\.live\.toggle: \[\]$/m);
	expect(no.text).toContain(`omp language servers: idle ones stop after 5 minutes (yours kept): ${lsp()}`);
	expect(no.text).toContain("  kept: idleTimeoutMs: yours 60000 kept (Jocasta ships 300000)");
	expect(readFileSync(keybindings(), "utf8")).toBe("app.model.cycle:\n  - ctrl+k\n");

	const yes = await setup("y");
	expect(yes.text).toContain(`kept ${lsp()}`);
	expect(Bun.YAML.parse(readFileSync(keybindings(), "utf8"))).toEqual({
		"app.model.cycle": ["ctrl+k"],
		"app.live.toggle": [],
		"app.stt.toggle": "f9",
	});
	expect(JSON.parse(readFileSync(lsp(), "utf8"))).toEqual({
		servers: { biome: { disabled: true } },
		idleTimeoutMs: 60000,
	});
	// Setup made neither file and added only its two keys: that is all uninstall may take out.
	const undo = setupUndo(paths, false);
	expect(undo).toContainEqual({
		kind: "keys",
		path: keybindings(),
		keys: { "app.live.toggle": [], "app.stt.toggle": "f9" },
		created: false,
	});
	expect(JSON.stringify(undo)).not.toContain(JSON.stringify(lsp()));
	expect((await setup("y")).text).not.toContain(keybindings());
});

test("his keybindings.yaml is the one merged; an old keybindings.json waits for omp to move it (0479)", async () => {
	const yaml = join(paths.ompAgentDir, "keybindings.yaml");
	put(yaml, "app.model.cycle: [ctrl+k]\n");
	await setup("y");
	expect(Bun.YAML.parse(readFileSync(yaml, "utf8"))).toEqual({
		"app.model.cycle": ["ctrl+k"],
		"app.live.toggle": [],
		"app.stt.toggle": "f9",
	});
	expect(existsSync(keybindings())).toBe(false);

	rmSync(yaml);
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	put(join(paths.ompAgentDir, "keybindings.json"), '{ "app.model.cycle": ["ctrl+k"] }\n');
	const { text } = await setup("y");
	expect(text).toContain("omp's keybindings.json is in its old format: start omp once");
	expect(existsSync(keybindings())).toBe(false);
});

test("WATCHDOG.md: written when missing, his own shown and kept, Jocasta's older text follows the new one (0479)", async () => {
	const assets = ownAssets();
	put(watchdog(), "# His brief\n");
	const his = await setup("y");
	expect(his.text).toContain(`the advisor's brief (yours kept; Jocasta's differs as shown): ${watchdog()}`);
	expect(his.text).toMatch(/^- # His brief$/m);
	expect(his.text).toMatch(/^\+ # Advisor brief \(Jocasta\)$/m);
	expect(his.text).toContain("  kept: yours stays as it is, never overwritten (Jocasta's text is config/WATCHDOG.md)");
	expect(his.text).toContain(`kept ${watchdog()}`);
	expect(readFileSync(watchdog(), "utf8")).toBe("# His brief\n");
	expect(JSON.stringify(setupUndo(paths, false))).not.toContain(JSON.stringify(watchdog()));
	// Shown once: the same shipped text never asks again.
	expect((await setup("y")).text).not.toContain(watchdog());
	// A newer shipped text is shown again, and his stays.
	put(join(assets, "WATCHDOG.md"), "# Advisor brief v2\n");
	expect((await setup("y")).text).toContain(
		`the advisor's brief (yours kept; Jocasta's differs as shown): ${watchdog()}`,
	);
	expect(readFileSync(watchdog(), "utf8")).toBe("# His brief\n");

	// Missing: Jocasta's text is written; a newer one replaces it after `y` while he hasn't changed it.
	rmSync(watchdog());
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	await setup("y");
	expect(readFileSync(watchdog(), "utf8")).toBe("# Advisor brief v2\n");
	put(join(assets, "WATCHDOG.md"), "# Advisor brief v3\n");
	const newer = await setup("y");
	expect(newer.text).toContain(`the advisor's brief: ${watchdog()}`);
	expect(newer.text).toMatch(/^- # Advisor brief v2$/m);
	expect(readFileSync(watchdog(), "utf8")).toBe("# Advisor brief v3\n");
	expect(setupUndo(paths, false)).toContainEqual({
		kind: "text",
		path: watchdog(),
		fingerprint: fingerprint("# Advisor brief v3\n"),
	});
});

test("secrets.yml: a list omp reads, written when missing, his own kept and never shown, Jocasta's older text follows (0594)", async () => {
	// omp warns at every start about a file that isn't a YAML list (`secrets/index.ts:299-301`).
	expect(Bun.YAML.parse(readFileSync(join(ASSETS, "secrets.yml"), "utf8"))).toEqual([]);
	const assets = ownAssets();
	const mine = "- type: plain\n  content: his-name-q7z\n";
	put(secrets(), mine);
	const his = await setup("y");
	expect(his.text).not.toContain("his-name-q7z");
	expect(his.text).not.toContain(secrets());
	expect(readFileSync(secrets(), "utf8")).toBe(mine);
	expect(JSON.stringify(setupUndo(paths, false))).not.toContain(JSON.stringify(secrets()));
	// A newer shipped text leaves his alone, unshown.
	put(join(assets, "secrets.yml"), "# v2\n[]\n");
	const newer = await setup("y");
	expect(newer.text).not.toContain("his-name-q7z");
	expect(readFileSync(secrets(), "utf8")).toBe(mine);

	// Missing: Jocasta's text is written; a newer one replaces it after `y` while he hasn't changed it.
	rmSync(secrets());
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	await setup("y");
	expect(readFileSync(secrets(), "utf8")).toBe("# v2\n[]\n");
	put(join(assets, "secrets.yml"), "# v3\n[]\n");
	const follows = await setup("y");
	expect(follows.text).toMatch(/^- # v2$/m);
	expect(readFileSync(secrets(), "utf8")).toBe("# v3\n[]\n");
	expect(setupUndo(paths, false)).toContainEqual({
		kind: "text",
		path: secrets(),
		fingerprint: fingerprint("# v3\n[]\n"),
	});
});

test("on Windows the shipped fence also names Windows' own folders, and a second run has nothing to do (0594)", async () => {
	const shipped = readFileSync(join(ASSETS, "omp-config.yml"), "utf8");
	const fence = (text: string) =>
		(Bun.YAML.parse(text) as { tools: { restrictedFolders: string[] } }).tools.restrictedFolders;
	expect(fence(shipped).length).toBeGreaterThan(0);
	expect(ompSettingsText(shipped, undefined)).toBe(shipped);
	expect(ompSettingsText("tools:\n  restrictedFolders: []\n", ["C:\\W"])).toBe(
		'tools:\n  restrictedFolders: ["C:\\\\W"]\n',
	);
	expect(() => ompSettingsText("tools: {}\n", ["C:\\W"])).toThrow("no one-line tools.restrictedFolders list");

	paths.systemFolders = ["C:\\WINDOWS", "D:\\Program Files"];
	await setup("y");
	expect(fence(readFileSync(ompConfig(), "utf8"))).toEqual([...fence(shipped), "C:\\WINDOWS", "D:\\Program Files"]);
	expect((await setup("y")).text).toBe("Nothing to set up: everything Jocasta adds is in place.");
});

test("setupToast: one line when setup has changes waiting, none when everything is in place (0466)", async () => {
	const set = {
		PI_CODING_AGENT_DIR: paths.ompAgentDir,
		JOCASTA_CODING_AGENT_DIR: paths.jocastaConfigDir,
		JOCASTA_DATA_DIR: paths.dataDir,
		LOCALAPPDATA: join(root, "local"),
		MISE_CONFIG_DIR: paths.miseConfigDir,
	};
	const saved = Object.fromEntries(Object.keys(set).map((name) => [name, process.env[name]]));
	Object.assign(process.env, set);
	try {
		expect(await setupToast()).toMatch(/^jocasta setup has \d+ changes waiting: run `jocasta setup` to see them\.$/);
		expect(existsSync(paths.ompAgentDir)).toBe(false);
	} finally {
		// Key by key: a replaced `process.env` is a plain object, and later workers and children no longer see changes
		// to it (the eval worker test's omp log on Linux CI).
		for (const [name, value] of Object.entries(saved)) {
			if (value === undefined) delete process.env[name];
			else process.env[name] = value;
		}
	}
});

test("herdrConfig writes the HUD's command per platform as a TOML string and leaves comments alone (0403, 0572)", () => {
	const template = '# {{hud}}\ncommand = "{{hud}}"\n';
	const windows = herdrConfig(template, 'C:\\D\\da"ta', true);
	expect(windows).toStartWith("# {{hud}}\n");
	expect(Bun.TOML.parse(windows)).toEqual({ command: 'type "C:\\D\\da"ta\\hud.txt"' });
	const posix = herdrConfig(template, "/home/x/it's", false);
	expect(Bun.TOML.parse(posix)).toEqual({ command: "cat '/home/x/it'\\''s/hud.txt'" });
	// The command a POSIX shell runs prints the file.
	const sh = Bun.which("sh");
	if (process.platform !== "win32" && sh) {
		const data = join(root, "it's");
		mkdirSync(data);
		writeFileSync(join(data, "hud.txt"), "2 awaiting you\n");
		const command = `cat '${root}/it'\\''s/hud.txt'`;
		expect(Bun.TOML.parse(herdrConfig(template, data, false))).toEqual({ command });
		expect(Bun.spawnSync([sh, "-c", command], { windowsHide: true }).stdout.toString()).toBe("2 awaiting you\n");
	}
});

test("lineDiff keeps, removes and adds lines in order; a CRLF file's last line is no change", () => {
	expect(lineDiff("a\nb\nc\n", "a\nx\nc\nd\n")).toEqual(["  a", "+ x", "- b", "  c", "+ d"]);
	expect(lineDiff("a: 1\r\nb: 2\r\n", "a: 1\nb: 2\n")).toEqual(["  a: 1", "  b: 2"]);
});

test("a file changed after the diff is refused, never overwritten; the changes after it wait (0584)", async () => {
	put(ompConfig(), "modelRoles:\n  default: anthropic/model-a\n");
	const { out, io: setupIo } = io([]);
	setupIo.ask = async () => {
		// omp saves a setting while the diff waits for his answer.
		put(ompConfig(), "modelRoles:\n  default: anthropic/model-b\n");
		return "y";
	};
	const code = await runSetup(paths, { herdr: "herdr.exe", run }, setupIo);
	expect(code).toBe(1);
	const failed = out.find((line) => line.startsWith("jocasta setup: 0003-omp-settings"));
	expect(failed).toStartWith(
		`jocasta setup: 0003-omp-settings (omp settings Jocasta ships) failed: ${ompConfig()} changed since the diff; run jocasta setup again; the `,
	);
	expect(failed).toMatch(/; the \d+ changes after it wait\.$/);
	expect(readFileSync(ompConfig(), "utf8")).toBe("modelRoles:\n  default: anthropic/model-b\n");
	expect(readMarker(paths.jocastaConfigDir, "0003-omp-settings")).toBeUndefined();
	// A file that appeared after the diff named it new is refused too.
	removeTree(root);
	const fresh = io([]);
	fresh.io.ask = async () => {
		put(globalMcp(), '{ "mcpServers": { "github": { "type": "stdio", "command": "gh-mcp" } } }\n');
		return "y";
	};
	expect(await runSetup(paths, { herdr: "herdr.exe", run }, fresh.io)).toBe(1);
	expect(fresh.out.join("\n")).toContain(`${globalMcp()} changed since the diff; run jocasta setup again`);
	expect(JSON.parse(readFileSync(globalMcp(), "utf8")).mcpServers).toEqual({
		github: { type: "stdio", command: "gh-mcp" },
	});
});

test("his keybindings.yml keeps his comments and formatting: only Jocasta's key lines change (0584)", async () => {
	const his =
		"# my bindings, tuned over months\napp.model.cycle: [ctrl+k] # cycle models\n\napp.live.toggle:\n  - ctrl+l\n";
	put(keybindings(), his);
	// Jocasta's earlier value for a key it claimed follows the new one; here his own `ctrl+l` stays and is shown.
	const no = await setup("n");
	expect(no.text).toContain("  kept: app.live.toggle: yours");
	expect(no.text).not.toContain("- # my bindings, tuned over months");
	await setup("y");
	expect(readFileSync(keybindings(), "utf8")).toBe(`${his}app.stt.toggle: f9\n`);

	// A key line is replaced in place when its value changes; the lines around it stay.
	expect(
		editConfig(
			keybindings(),
			"# mine\napp.live.toggle: [] # off\napp.stt.toggle: f9\n# end\n",
			{ "app.live.toggle": [], "app.stt.toggle": "f8" },
			["app.stt.toggle"],
			[],
		),
	).toBe("# mine\napp.live.toggle: [] # off\napp.stt.toggle: f8\n# end\n");
	// A CRLF file keeps its line ends; lines that can't give the keys are never rewritten whole: undefined.
	expect(editConfig(keybindings(), "a: 1\r\n", { a: 1, b: 2 }, ["b"], [])).toBe("a: 1\r\nb: 2\r\n");
	expect(editConfig(keybindings(), "{a: 1}\n", { a: 1, b: 2 }, ["b"], [])).toBeUndefined();
	// His comments alone left read as an empty file.
	expect(editConfig(keybindings(), "# mine\nb: 2\n", {}, [], ["b"])).toBe("# mine\n");
});

test("a keybindings.yml whose lines can't be edited stays byte for byte, and its migration fails saying why (SET-5, 0584)", async () => {
	const his = "{app.model.cycle: [ctrl+k]} # cycle models, flow style\n";
	put(keybindings(), his);
	const { code, text } = await setup("y");
	expect(code).toBe(1);
	expect(text).toContain(
		`failed: ${keybindings()} can't be edited line by line, so setup left it as it is; set app.live.toggle: [], app.stt.toggle: "f9" in it by hand, then run jocasta setup again`,
	);
	expect(readFileSync(keybindings(), "utf8")).toBe(his);
});

test("a merge that keeps only his values says kept, and config.yml stays byte for byte (SET-12)", async () => {
	const assets = ownAssets();
	put(join(assets, "omp-config.yml"), "compaction:\n  thresholdPercent: 60\n");
	put(ompConfig(), "# his\ncompaction:\n  thresholdPercent: 45\n");
	const { text } = await setup("y");
	expect(text).toContain(`kept ${ompConfig()}`);
	expect(text).not.toContain(`wrote ${ompConfig()}`);
	expect(readFileSync(ompConfig(), "utf8")).toBe("# his\ncompaction:\n  thresholdPercent: 45\n");
});

test("--refresh herdr.toml works beside a malformed lsp.json it doesn't reset (SET-13)", async () => {
	await setup("y");
	put(herdrToml(), "# his\n");
	put(lsp(), "{ broken");
	const { code, text } = await refresh("herdr.toml", "y");
	expect(code).toBe(0);
	expect(text).toContain(`kept ${herdrToml()}.bak.20261023-090507`);
	expect(readFileSync(herdrToml(), "utf8")).not.toBe("# his\n");
});

test("without herdr setup ends by saying so, never Done, even with its integration already there (0584)", async () => {
	put(join(paths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION), "// herdr\n");
	const { text } = await setup("y", null);
	const herdrLine = "herdr isn't installed: install it from https://herdr.dev, then run `jocasta setup` again.";
	expect(text).toStartWith(herdrLine);
	expect(text).toEndWith(herdrLine);
	expect(text).not.toContain("Done: run `jocasta`");
	expect(text.split(herdrLine)).toHaveLength(3);
	expect((await setup("y", null)).text).toBe(herdrLine);
});

test("setup reloads herdr after writing herdr.toml, and says so (0584)", async () => {
	const reloads: string[] = [];
	extra = {
		reloadHerdr: async () => {
			reloads.push("reload");
			return { code: 0, output: "" };
		},
	};
	const first = await setup("y");
	// Once for the plugin's link, once for the config, with the line.
	expect(reloads).toEqual(["reload", "reload"]);
	expect(first.text).toContain("herdr reloaded its config.\nDone: run `jocasta` to open Jocasta.");
	reloads.length = 0;
	const assets = ownAssets();
	put(join(assets, "herdr.toml"), `${readFileSync(join(assets, "herdr.toml"), "utf8")}# shipped later\n`);
	extra = { reloadHerdr: async () => ({ code: 1, output: "error: no running server for session jocasta\n" }) };
	expect((await setup("y")).text).toContain(
		"herdr didn't reload its config (error: no running server for session jocasta): restart herdr to use it (close Jocasta, then run `jocasta`).",
	);
});

test("the companion's logon Run value goes; another program's value of that name stays (0557)", async () => {
	const values: Record<string, string> = { Jocasta: '"C:\\Jocasta\\app\\bin\\launcher.exe"' };
	extra = {
		runValue: {
			read: (name) => values[name],
			remove: (name) => delete values[name],
		},
	};
	const { text } = await setup("y");
	expect(text).toContain("Jocasta no longer starts at logon: its Run value goes");
	expect(values).toEqual({});

	values.Jocasta = '"C:\\Other\\program.exe"';
	rmSync(markersDir(paths.jocastaConfigDir), { recursive: true });
	expect((await setup("y")).text).not.toContain("Run value");
	expect(values).toEqual({ Jocasta: '"C:\\Other\\program.exe"' });
});

/** A Credential Manager stand-in for the calendar's secret (0430). */
function calendarStore(fromEnv?: string): CalendarStore & { saved: string[] } {
	const saved: string[] = [];
	return { saved, stored: () => saved.length > 0, save: (secret) => void saved.push(secret), fromEnv };
}

async function setupWith(calendar: CalendarStore, answers: (string | undefined)[]) {
	const { out, asked, io: setupIo } = io(answers);
	await runSetup(paths, { herdr: "herdr.exe", run, calendar }, setupIo);
	return { text: out.join("\n"), asked };
}

test("the calendar's env secret moves into Credential Manager with the other changes, never shown (0430)", async () => {
	const calendar = calendarStore("https://calendar.example/secret-token.ics");
	const { text, asked } = await setupWith(calendar, ["y"]);
	expect(text).toContain("jocasta/calendar-ical");
	expect(text).not.toContain("secret-token");
	expect(calendar.saved).toEqual(["https://calendar.example/secret-token.ics"]);
	expect(asked).toHaveLength(1);
	expect(readMarker(paths.jocastaConfigDir, "calendar")?.undo).toEqual([
		{ kind: "credential", target: "jocasta/calendar-ical" },
	]);
	// Stored now: a second run neither proposes nor asks.
	const again = await setupWith(calendar, ["y"]);
	expect(again.text).not.toContain("jocasta/calendar-ical");
	expect(calendar.saved).toHaveLength(1);
});

test("the calendar's secret address is asked for as a secret, off the screen, where the console can (SEC-LOC-12)", async () => {
	const calendar = calendarStore();
	const secrets: string[] = [];
	const { io: setupIo, asked } = io(["y"]);
	await runSetup(
		paths,
		{ herdr: "herdr.exe", run, calendar },
		{
			...setupIo,
			askSecret: async (question) => {
				secrets.push(question);
				return "https://calendar.example/secret.ics";
			},
		},
	);
	expect(secrets).toEqual(["Your calendar's secret iCal address, for today's meetings (Enter to skip): "]);
	expect(asked).toEqual(["Write these changes? [y/N] "]);
	expect(calendar.saved).toEqual(["https://calendar.example/secret.ics"]);
});

test("with no calendar secret anywhere, setup asks once; Enter skips, an address is saved (0430)", async () => {
	const skipped = calendarStore();
	const first = await setupWith(skipped, ["y", ""]);
	expect(first.asked.at(-1)).toContain("iCal address");
	expect(skipped.saved).toEqual([]);
	const given = calendarStore();
	await setupWith(given, ["y", "not a url"]);
	expect(given.saved).toEqual([]);
	await setupWith(given, ["webcal://calendar.example/secret.ics"]);
	expect(given.saved).toEqual(["webcal://calendar.example/secret.ics"]);
	expect(readMarker(paths.jocastaConfigDir, "calendar")).toBeDefined();
});
