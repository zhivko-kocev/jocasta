import { afterEach, beforeEach, expect, test } from "bun:test";
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	realpathSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { ServiceState } from "../../src/frontier/server/launch.ts";
import { markersDir } from "../../src/setup/markers.ts";
import { HERDR_OMP_EXTENSION, runSetup, type SetupPaths, setupUndo } from "../../src/setup/setup.ts";
import {
	describeProcess,
	type InstallRecord,
	installContents,
	rcPathLine,
	readInstallRecord,
	removeAfterExit,
	removeRcLine,
	runUninstall,
	type UninstallDeps,
	type UninstallPaths,
	unsafeFolder,
} from "../../src/setup/uninstall.ts";
import { UserError } from "../../src/shared/errors.ts";
import { removeTree } from "../../src/shared/fsx.ts";

const ASSETS = resolve(import.meta.dir, "..", "..", "config");
let root = "";
let setupPaths: SetupPaths;
let calls: string[];

const put = (path: string, text: string) => {
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, text);
};

beforeEach(async () => {
	root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-uninstall-")));
	calls = [];
	setupPaths = {
		ompAgentDir: join(root, "omp", "agent"),
		jocastaConfigDir: join(root, "jocasta", "agent"),
		dataDir: join(root, "local", "jocasta", "data"),
		assetsDir: ASSETS,
		terminalFragmentsDir: join(root, "Fragments"),
		miseConfigDir: join(root, "mise"),
		jocastaCommand: ["C:\\Jocasta\\jocasta.exe"],
		whiteboard: undefined,
	};
	// A machine set up by `jocasta setup`, with his own MCP server beside Jocasta's and the calendar's secret stored.
	put(join(setupPaths.ompAgentDir, "mcp.json"), JSON.stringify({ mcpServers: { github: { command: "gh-mcp" } } }));
	let stored = false;
	await runSetup(
		setupPaths,
		{
			herdr: "herdr.exe",
			mise: "mise.exe",
			run: async () => {
				put(join(setupPaths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION), "// herdr\n");
				return { code: 0, output: "" };
			},
			calendar: {
				stored: () => stored,
				save: () => {
					stored = true;
				},
				fromEnv: "https://calendar.example.org/secret.ics",
			},
		},
		{ print: () => {}, ask: async () => "y" },
	);
	put(join(setupPaths.dataDir, "catalog.db"), "db");
	put(join(setupPaths.jocastaConfigDir, "compose.yaml"), "services: {}\n");
	put(join(setupPaths.miseConfigDir, "config.jocasta-p1.override.toml"), "[tools]\n");
	put(join(setupPaths.miseConfigDir, "config.toml"), "# his\n");
});
afterEach(() => removeTree(root));

function install(): InstallRecord {
	const dir = join(root, "Programs", "jocasta");
	put(join(dir, "jocasta.exe"), "exe");
	put(join(dir, "install.json"), JSON.stringify({ dir }));
	return { dir, path: dir, autostart: "Jocasta", appData: join(root, "local", "local.jocasta.frontier") };
}

function uninstallPaths(record?: InstallRecord, runningFromInstall = false): UninstallPaths {
	return {
		jocastaConfigDir: setupPaths.jocastaConfigDir,
		dataDir: setupPaths.dataDir,
		miseConfigDir: setupPaths.miseConfigDir,
		composeFile: join(setupPaths.jocastaConfigDir, "compose.yaml"),
		install: record,
		runningFromInstall,
	};
}

function deps(state: ServiceState = "down"): UninstallDeps {
	return {
		serviceState: async () => state,
		herdr: "herdr.exe",
		session: "jocasta-test",
		run: async (argv) => {
			calls.push(argv.join(" "));
			if (argv.includes("integration")) rmSync(join(setupPaths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION));
			return { code: 0, output: "" };
		},
		setupUndo: setupUndo(setupPaths, true),
		composeDown: async () => {
			calls.push("compose down");
			return { code: 0, output: "" };
		},
		dockerRunning: () => true,
		removePathEntry: (entry) => {
			calls.push(`path ${entry}`);
			return true;
		},
		removeRunValue: (name, launcher) => {
			calls.push(`run ${name} ${launcher}`);
			return true;
		},
		deleteCredential: (target) => {
			calls.push(`credential ${target}`);
			return true;
		},
		removeAfterExit: (dir, entries) => void calls.push(`after exit ${dir} ${entries.join(",")}`),
	};
}

async function uninstall(answers: string[], paths = uninstallPaths(), d = deps()) {
	const out: string[] = [];
	const asked: string[] = [];
	const code = await runUninstall(paths, d, {
		print: (l) => void out.push(l),
		ask: async (q) => {
			asked.push(q);
			return answers.shift();
		},
	});
	return { code, text: out.join("\n"), asked };
}

test("a running Jocasta is refused; `n` removes nothing", async () => {
	await expect(uninstall(["y"], uninstallPaths(), deps("up"))).rejects.toBeInstanceOf(UserError);
	const { code, text, asked } = await uninstall(["n"]);
	expect(code).toBe(0);
	expect(asked).toEqual(["Remove these? [y/N] "]);
	expect(text).toContain("herdr session `jocasta-test` (stopped, its panes closed, then deleted)");
	expect(text).toContain("the `jocasta` herdr plugin (unlinked from herdr)");
	expect(text).toContain(`MCP servers "jocasta", "whiteboard" in ${join(setupPaths.ompAgentDir, "mcp.json")}`);
	expect(text).toContain(
		`Jocasta's mise config for a project: ${join(setupPaths.miseConfigDir, "config.jocasta-p1.override.toml")}`,
	);
	expect(text).toContain("Windows Credential Manager: jocasta/calendar-ical");
	expect(text).toContain("Keys setup merged into omp's config.yml stay: they are your settings.");
	expect(text).toEndWith("Nothing removed.");
	expect(calls).toEqual([]);
	expect(existsSync(join(setupPaths.jocastaConfigDir, "herdr.toml"))).toBe(true);
});

test("`y` removes what install and setup added and keeps his own; data and containers only on their own `y` (0468)", async () => {
	const record = install();
	put(join(record.appData as string, "stable", "WebView2", "x"), "x");
	const { code, text, asked } = await uninstall(["y", "n", "n"], uninstallPaths(record));
	expect(code).toBe(0);
	expect(asked).toEqual([
		"Remove these? [y/N] ",
		`Also remove Jocasta's data folder ${setupPaths.dataDir} (memory, models, logs)? [y/N] `,
		"Also remove Jocasta's containers, images and volumes (Graphiti and FalkorDB, with the memory graph)? [y/N] ",
	]);
	expect(text).toEndWith("Jocasta is uninstalled.");
	expect(calls).toEqual([
		// The plugin goes first, while the session can still run the server unlinking needs (0558).
		"herdr.exe --session jocasta-test plugin unlink jocasta",
		"herdr.exe session stop jocasta-test",
		"herdr.exe session delete jocasta-test",
		"credential jocasta/calendar-ical",
		"herdr.exe integration uninstall omp",
		`run Jocasta ${join(record.dir, "app", "bin", "launcher.exe")}`,
		`path ${record.dir}`,
	]);
	// His MCP server, omp settings and mise config stay; Jocasta's are gone.
	expect(JSON.parse(readFileSync(join(setupPaths.ompAgentDir, "mcp.json"), "utf8"))).toEqual({
		mcpServers: { github: { command: "gh-mcp" } },
	});
	expect(existsSync(join(setupPaths.ompAgentDir, "config.yml"))).toBe(true);
	expect(existsSync(join(setupPaths.miseConfigDir, "config.toml"))).toBe(true);
	for (const gone of [
		join(setupPaths.dataDir, "home", ".omp", "mcp.json"),
		join(setupPaths.ompAgentDir, "skills", "mise"),
		join(setupPaths.miseConfigDir, "conf.d", "jocasta.toml"),
		// The three omp files setup made, with nothing of his in them (0479).
		join(setupPaths.ompAgentDir, "keybindings.yml"),
		join(setupPaths.ompAgentDir, "WATCHDOG.md"),
		join(setupPaths.ompAgentDir, "lsp.json"),
		join(setupPaths.miseConfigDir, "config.jocasta-p1.override.toml"),
		join(root, "Fragments", "jocasta"),
		setupPaths.jocastaConfigDir,
		record.appData as string,
		record.dir,
	]) {
		expect(existsSync(gone)).toBe(false);
	}
	expect(existsSync(join(setupPaths.dataDir, "catalog.db"))).toBe(true);
});

test("uninstall takes out only what setup added to the three omp files: his keys and his brief stay (0479)", async () => {
	const agent = setupPaths.ompAgentDir;
	// Files setup made that he changed since.
	put(join(agent, "keybindings.yml"), "app.live.toggle: []\napp.model.cycle: [ctrl+k]\n");
	put(join(agent, "WATCHDOG.md"), "# His brief\n");
	put(join(agent, "lsp.json"), '{ "idleTimeoutMs": 60000 }\n');
	const { code, text } = await uninstall(["y", "n", "n"]);
	expect(code).toBe(0);
	expect(text).toContain(`- Jocasta's app.live.toggle, app.stt.toggle in ${join(agent, "keybindings.yml")}`);
	expect(text).toContain(`- ${join(agent, "WATCHDOG.md")} (while it is still Jocasta's text)`);
	expect(text).toContain(`removed app.live.toggle from ${join(agent, "keybindings.yml")}`);
	expect(text).toContain(`kept ${join(agent, "WATCHDOG.md")}: you changed it`);
	expect(text).toContain(
		`${join(agent, "lsp.json")} has none of Jocasta's keys left; kept idleTimeoutMs (you changed it)`,
	);
	expect(Bun.YAML.parse(readFileSync(join(agent, "keybindings.yml"), "utf8"))).toEqual({
		"app.model.cycle": ["ctrl+k"],
	});
	expect(readFileSync(join(agent, "WATCHDOG.md"), "utf8")).toBe("# His brief\n");
	expect(JSON.parse(readFileSync(join(agent, "lsp.json"), "utf8"))).toEqual({ idleTimeoutMs: 60000 });
});

test("uninstall's key removal keeps his keybindings.yml comments and formatting: only Jocasta's lines go (0584)", async () => {
	const file = join(setupPaths.ompAgentDir, "keybindings.yml");
	put(
		file,
		"# my bindings, tuned over months\napp.model.cycle: [ctrl+k] # cycle models\napp.live.toggle: []\napp.stt.toggle: f9\n",
	);
	const { text } = await uninstall(["y", "n", "n"]);
	expect(text).toContain(`removed app.live.toggle, app.stt.toggle from ${file}`);
	expect(readFileSync(file, "utf8")).toBe(
		"# my bindings, tuned over months\napp.model.cycle: [ctrl+k] # cycle models\n",
	);
});

test("a keybindings.yml setup made stays when his comment is left in it; only Jocasta's lines go (0584)", async () => {
	const file = join(setupPaths.ompAgentDir, "keybindings.yml");
	put(file, "# F9 dictation is how I like it\napp.live.toggle: []\napp.stt.toggle: f9\n");
	const { text } = await uninstall(["y", "n", "n"]);
	expect(text).toContain(`removed app.live.toggle, app.stt.toggle from ${file}`);
	expect(readFileSync(file, "utf8")).toBe("# F9 dictation is how I like it\n");
});

test("a keybindings.yml whose lines can't be edited stays byte for byte; uninstall says what to remove by hand (SET-5, 0584)", async () => {
	const file = join(setupPaths.ompAgentDir, "keybindings.yml");
	const his = '{app.live.toggle: [], app.stt.toggle: f9, app.model.cycle: ["ctrl+k"]}\n';
	put(file, his);
	const { text } = await uninstall(["y", "n", "n"]);
	expect(text).toContain(
		`couldn't remove Jocasta's app.live.toggle, app.stt.toggle in ${file}: ${file} can't be edited line by line, so uninstall left it as it is; remove app.live.toggle, app.stt.toggle from it by hand`,
	);
	expect(readFileSync(file, "utf8")).toBe(his);
});

test("a file of his that setup added a key to stays when only that key goes (0479)", async () => {
	setupPaths = {
		...setupPaths,
		ompAgentDir: join(root, "omp2", "agent"),
		jocastaConfigDir: join(root, "jocasta2", "agent"),
	};
	const lsp = join(setupPaths.ompAgentDir, "lsp.json");
	put(lsp, '{ "servers": { "biome": { "disabled": true } } }\n');
	await runSetup(
		setupPaths,
		{ herdr: undefined, run: async () => ({ code: 0, output: "" }) },
		{ print: () => {}, ask: async () => "y" },
	);
	expect(JSON.parse(readFileSync(lsp, "utf8"))).toEqual({
		servers: { biome: { disabled: true } },
		idleTimeoutMs: 300000,
	});
	// He drops his servers: the file setup didn't make stays, emptied of Jocasta's key.
	put(lsp, '{ "idleTimeoutMs": 300000 }\n');
	const { text } = await uninstall(["y", "n", "n"]);
	expect(text).toContain(`removed idleTimeoutMs from ${lsp}`);
	expect(JSON.parse(readFileSync(lsp, "utf8"))).toEqual({});
});

test("the data folder and the containers go on their own `y`; a running install's folder goes after exit", async () => {
	const record = install();
	const { text } = await uninstall(["y", "y", "y"], uninstallPaths(record, true));
	expect(calls).toContain("compose down");
	expect(existsSync(setupPaths.dataDir)).toBe(false);
	// `<LOCALAPPDATA>\jocasta` held only the data folder.
	expect(existsSync(dirname(setupPaths.dataDir))).toBe(false);
	expect(calls.at(-1)).toBe(`after exit ${record.dir} jocasta.exe`);
	expect(existsSync(record.dir)).toBe(true);
	expect(text).toContain(`${record.dir} goes when this command exits.`);
});

test("a step that fails is said and the rest still go; the exit code says so", async () => {
	const failing = deps();
	failing.run = async (argv) => {
		calls.push(argv.join(" "));
		return argv.includes("delete") ? { code: 1, output: "session is running\n" } : { code: 0, output: "" };
	};
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(), failing);
	expect(code).toBe(1);
	expect(text).toContain("couldn't remove herdr session `jocasta-test`");
	expect(text).toContain("herdr session delete jocasta-test exited 1: session is running");
	expect(text).toEndWith("Done, except 1 thing above; remove it by hand.");
	expect(existsSync(setupPaths.jocastaConfigDir)).toBe(false);
});

test("unlink takes herdr's real replies: no server starts one, then a plugin it doesn't have was already unlinked (0558)", async () => {
	const unlinking = deps();
	let started = false;
	unlinking.startHerdr = async () => {
		started = true;
	};
	unlinking.run = async (argv) => {
		calls.push(argv.join(" "));
		if (!argv.includes("unlink")) return { code: 0, output: "" };
		// herdr 0.9.5's replies, as seen on Windows (2026-11-09).
		return started
			? {
					code: 0,
					output: '{"id":"cli:plugin","result":{"plugin_id":"jocasta","removed":false,"type":"plugin_unlinked"}}\n',
				}
			: {
					code: 1,
					output:
						'{"id":"cli:plugin","error":{"code":"server_not_running","message":"no herdr server is running at C:\\\\h\\\\herdr.sock; run `herdr` to start or attach it"}}\n',
				};
	};
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(), unlinking);
	expect(code).toBe(0);
	expect(started).toBe(true);
	expect(text).toContain("the jocasta herdr plugin was already unlinked");
});

test("without setup's markers nothing outside Jocasta's own folders is listed or touched (B1)", async () => {
	rmSync(markersDir(setupPaths.jocastaConfigDir), { recursive: true });
	const mcp = join(setupPaths.ompAgentDir, "mcp.json");
	const his = readFileSync(mcp, "utf8");
	const listed = (await uninstall(["n"])).text.split("\n").filter((line) => line.startsWith("- "));
	expect(listed).toEqual([
		"- herdr session `jocasta-test` (stopped, its panes closed, then deleted)",
		`- Jocasta's mise config for a project: ${join(setupPaths.miseConfigDir, "config.jocasta-p1.override.toml")}`,
		`- Jocasta's config folder: ${setupPaths.jocastaConfigDir}`,
	]);
	const { code } = await uninstall(["y", "n", "n"]);
	expect(code).toBe(0);
	expect(calls).toEqual(["herdr.exe session stop jocasta-test", "herdr.exe session delete jocasta-test"]);
	expect(readFileSync(mcp, "utf8")).toBe(his);
	for (const kept of [
		join(setupPaths.ompAgentDir, "extensions", HERDR_OMP_EXTENSION),
		join(setupPaths.ompAgentDir, "keybindings.yml"),
		join(setupPaths.ompAgentDir, "skills", "mise", "SKILL.md"),
		join(setupPaths.miseConfigDir, "conf.d", "jocasta.toml"),
		join(root, "Fragments", "jocasta", "jocasta.json"),
		join(setupPaths.dataDir, "home", ".omp", "mcp.json"),
	]) {
		expect(existsSync(kept)).toBe(true);
	}
});

test("an MCP entry that isn't what setup wrote is kept and said; one still as its marker recorded goes (0468)", async () => {
	const mcp = join(setupPaths.ompAgentDir, "mcp.json");
	const config = JSON.parse(readFileSync(mcp, "utf8"));
	// He changed Jocasta's entry; `whiteboard` is another install's (setup wrote none: whiteboard wasn't built).
	config.mcpServers.jocasta.args = ["mcp", "--his"];
	config.mcpServers.whiteboard = { type: "stdio", command: "C:\\Programs\\other\\node.exe", args: ["cli.js", "mcp"] };
	put(mcp, JSON.stringify(config));
	// Jocasta moved since setup ran: the boss's entry differs from today's but still is what setup wrote.
	setupPaths = { ...setupPaths, jocastaCommand: ["D:\\Moved\\jocasta.exe"] };
	const boss = join(setupPaths.dataDir, "home", ".omp", "mcp.json");
	const { code, text } = await uninstall(["y", "n", "n"]);
	expect(code).toBe(0);
	expect(text).toContain(`- MCP servers "jocasta", "whiteboard" in ${mcp} (each while it is still what setup wrote)`);
	expect(text).toContain(
		`${mcp} has none of Jocasta's MCP servers left; kept "jocasta", "whiteboard" (not what setup wrote)`,
	);
	expect(JSON.parse(readFileSync(mcp, "utf8"))).toEqual(config);
	expect(text).toContain(`removed ${boss}`);
	expect(existsSync(boss)).toBe(false);
});

test("with Docker not running the containers stay and that is said; the exit code says a thing is left (P5)", async () => {
	const d = deps();
	d.dockerRunning = () => false;
	const { code, text } = await uninstall(["y", "n", "y"], uninstallPaths(), d);
	expect(code).toBe(1);
	expect(text).toContain(
		"couldn't remove Jocasta's containers, images and volumes: Docker isn't running, so they stay",
	);
	expect(calls).not.toContain("compose down");
});

test("his private-names list stays in Jocasta's config folder; everything else there goes", async () => {
	const list = join(setupPaths.jocastaConfigDir, "private-names.txt");
	put(list, "example-pattern\n");
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths());
	expect(code).toBe(0);
	expect(text).toContain(`(kept: ${list}, yours, not Jocasta's)`);
	expect(readdirSync(setupPaths.jocastaConfigDir)).toEqual(["private-names.txt"]);
	expect(readFileSync(list, "utf8")).toBe("example-pattern\n");
});

test("install.json: today's names what the installer added; an older Windows one gets today's", () => {
	const dir = join(root, "inst");
	put(join(dir, "install.json"), JSON.stringify({ dir }));
	expect(readInstallRecord(dir, "C:\\L", true)).toEqual({
		dir,
		path: dir,
		autostart: "Jocasta",
		appData: join("C:\\L", "local.jocasta.frontier"),
	});
	expect(readInstallRecord(dir, undefined, false)).toEqual({ dir });
	put(join(dir, "install.json"), JSON.stringify({ dir, rc: "/home/u/.bashrc" }));
	expect(readInstallRecord(dir, undefined, false)).toEqual({ dir, rc: "/home/u/.bashrc" });
	expect(readInstallRecord(join(root, "none"), undefined)).toBeUndefined();
});

test("install.json is trusted only for its own folder, and the window data only directly in %LOCALAPPDATA% (0484)", () => {
	const dir = join(root, "inst");
	put(join(dir, "install.json"), JSON.stringify({ dir: join(root, "elsewhere") }));
	expect(readInstallRecord(dir, "C:\\L", true)).toBeUndefined();
	const local = join(root, "local");
	put(join(dir, "install.json"), JSON.stringify({ dir, appData: join(root, "Documents") }));
	expect(readInstallRecord(dir, local, true)).toEqual({ dir });
	put(join(dir, "install.json"), JSON.stringify({ dir, appData: join(local, "local.jocasta.frontier") }));
	expect(readInstallRecord(dir, local, true)).toEqual({ dir, appData: join(local, "local.jocasta.frontier") });
});

test("install.json's entries are kept as plain names only; a list with a path in it is no list (0579)", () => {
	const dir = join(root, "inst");
	put(join(dir, "install.json"), JSON.stringify({ dir, entries: ["jocasta", "config"], rc: "/home/u/.bashrc" }));
	expect(readInstallRecord(dir, undefined, false)).toEqual({
		dir,
		entries: ["jocasta", "config"],
		rc: "/home/u/.bashrc",
	});
	for (const bad of ["..", "../x", "a/b", "C:\\x", ""]) {
		put(join(dir, "install.json"), JSON.stringify({ dir, entries: ["config", bad], rc: "/r" }));
		expect(readInstallRecord(dir, undefined, false)).toEqual({ dir, rc: "/r" });
	}
});

test("uninstall removes only the install's listed entries; a folder holding his files stays with them (0579)", async () => {
	const dir = join(root, "bin");
	for (const name of ["jocasta", "config/boss.md", "node/node", "his-tool", "notes/his.txt", "jocasta.old-20261109"]) {
		put(join(dir, name), "x");
	}
	put(join(dir, "install.json"), JSON.stringify({ dir, entries: ["jocasta", "config", "node"] }));
	const record = readInstallRecord(dir, undefined, false) as InstallRecord;
	expect(installContents(record, false)).toEqual({
		ours: ["config", "jocasta", "jocasta.old-20261109", "node"],
		others: ["his-tool", "notes"],
	});
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(record));
	expect(code).toBe(0);
	expect(text).toContain(`removed Jocasta's files from ${dir}; kept the folder (it holds other files)`);
	expect(readdirSync(dir).sort()).toEqual(["his-tool", "notes"]);

	// Only Jocasta's entries: the folder goes with them.
	const own = join(root, "own");
	put(join(own, "jocasta"), "x");
	put(join(own, "install.json"), JSON.stringify({ dir: own, entries: ["jocasta", "hunk"] }));
	const again = await uninstall(["y", "n", "n"], uninstallPaths(readInstallRecord(own, undefined, false)));
	expect(again.text).toContain(`removed ${own}`);
	expect(existsSync(own)).toBe(false);
});

test("an install from before the list goes whole only when every entry is a release's; else it all stays (0579)", async () => {
	const record = install();
	put(join(record.dir, "his-tool.exe"), "his");
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(record));
	expect(code).toBe(1);
	expect(text).toContain(
		`couldn't remove ${record.dir}: kept ${record.dir}: it holds files that aren't Jocasta's (his-tool.exe); remove Jocasta's files from it by hand`,
	);
	expect(readdirSync(record.dir).sort()).toEqual(["his-tool.exe", "install.json", "jocasta.exe"]);
});

test("omp sessions running programs from the install are named and refused before anything is asked (0579)", async () => {
	const record = install();
	const d = deps();
	d.processesUnder = (dir) => [
		{ pid: 4120, path: `${dir}\\jocasta.exe`, commandLine: `"${dir}\\jocasta.exe" mcp` },
		{
			pid: 5332,
			path: `${dir}\\node\\node.exe`,
			commandLine: `"${dir}\\node\\node.exe" "${dir}\\whiteboard\\dist\\cli.js" mcp`,
		},
	];
	const refused = uninstall(["y"], uninstallPaths(record), d);
	await expect(refused).rejects.toThrow(
		`omp sessions still run programs from ${record.dir}: jocasta.exe mcp (pid 4120), node.exe whiteboard mcp (pid 5332). Close them, then run jocasta uninstall again.`,
	);
	expect(calls).toEqual([]);
	expect(existsSync(join(record.dir, "jocasta.exe"))).toBe(true);
});

test("describeProcess names the program and its words, a path inside the install as its entry", () => {
	const dir = "C:\\Users\\x\\AppData\\Local\\Programs\\jocasta";
	expect(describeProcess(dir, { pid: 7, path: `${dir}\\hunk\\hunk.exe`, commandLine: "hunk.exe diff --watch" })).toBe(
		"hunk.exe diff --watch (pid 7)",
	);
	expect(
		describeProcess(dir, {
			pid: 8,
			path: `${dir}\\jocasta.exe`,
			commandLine: `${dir}\\jocasta.exe mcp --boss C:\\elsewhere\\x.js`,
		}),
	).toBe("jocasta.exe mcp --boss C:\\elsewhere\\x.js (pid 8)");
});

test.if(process.platform === "win32")(
	"removeAfterExit removes the listed entries once the process has exited, then install.json and the empty folder (0579)",
	async () => {
		const dir = join(root, "Programs", "jocasta");
		put(join(dir, "jocasta.exe"), "exe");
		put(join(dir, "config", "boss.md"), "x");
		put(join(dir, "install.json"), JSON.stringify({ dir }));
		const waiter = Bun.spawn(["ping", "-n", "2", "127.0.0.1"], { stdout: "ignore", windowsHide: true });
		removeAfterExit(dir, ["jocasta.exe", "config"], waiter.pid);
		await waiter.exited;
		// The remover is a detached PowerShell with nothing to await: its effect on the folder is the signal.
		for (let waited = 0; existsSync(dir) && waited < 20_000; waited += 250) await Bun.sleep(250);
		expect(existsSync(dir)).toBe(false);
	},
	30_000,
);

test("a recorded folder that no longer holds Jocasta's marker is kept and said (0484)", async () => {
	const record = install();
	rmSync(join(record.dir, "install.json"));
	// Window data without WebView2's folder in it.
	put(join(record.appData as string, "his.txt"), "his");
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(record));
	expect(code).toBe(1);
	expect(text).toContain(
		`couldn't remove ${record.dir}: kept ${record.dir}: it no longer holds install.json naming it`,
	);
	expect(text).toContain(`kept ${record.appData}: it no longer holds the companion's WebView2 data`);
	expect(existsSync(join(record.dir, "jocasta.exe"))).toBe(true);
	expect(existsSync(join(record.appData as string, "his.txt"))).toBe(true);

	// Run from such an install, its folder isn't handed over for removal either.
	calls = [];
	const again = await uninstall(["y", "n", "n"], uninstallPaths(record, true));
	expect(again.code).toBe(1);
	expect(calls.some((c) => c.startsWith("after exit"))).toBe(false);
});

test("a drive root or his home folder is never removed whole (0484)", () => {
	expect(unsafeFolder(resolve("/"))).toBe("it is a drive root");
	expect(unsafeFolder(homedir())).toBe("it is your home folder or holds it");
	expect(unsafeFolder(dirname(homedir()))).toBe("it is your home folder or holds it");
	expect(unsafeFolder(join(homedir(), ".jocasta", "agent"))).toBeUndefined();
	expect(unsafeFolder(root)).toBeUndefined();
});

test("a folder setup didn't make stays: only setup's file in it goes (0484)", async () => {
	// Fresh setup on a machine where his skills/mise folder already held his own file.
	rmSync(setupPaths.jocastaConfigDir, { recursive: true, force: true });
	rmSync(join(setupPaths.ompAgentDir, "skills", "mise"), { recursive: true, force: true });
	put(join(setupPaths.ompAgentDir, "skills", "mise", "notes.md"), "his");
	await runSetup(
		setupPaths,
		{ herdr: undefined, mise: "mise.exe", run: async () => ({ code: 0, output: "" }) },
		{ print: () => {}, ask: async () => "y" },
	);
	const undo = setupUndo(setupPaths, false);
	const skill = join(setupPaths.ompAgentDir, "skills", "mise");
	expect(undo).toContainEqual({ kind: "file", path: join(skill, "SKILL.md") });
	expect(undo.some((u) => u.kind === "dir" && u.path === skill)).toBe(false);
	const d = deps();
	d.setupUndo = undo;
	const { code } = await uninstall(["y", "n", "n"], uninstallPaths(), d);
	expect(code).toBe(0);
	expect(existsSync(join(skill, "SKILL.md"))).toBe(false);
	expect(readFileSync(join(skill, "notes.md"), "utf8")).toBe("his");
});

test("a folder setup recorded before 0484 goes only when empty; one it made goes only with its file in it", async () => {
	const legacy = join(root, "legacy");
	put(join(legacy, "his.txt"), "his");
	const made = join(root, "made");
	put(join(made, "other.txt"), "x");
	const d = deps();
	d.setupUndo = [
		{ kind: "dir", path: legacy },
		{ kind: "dir", path: made, file: "jocasta.json" },
	];
	const { code, text } = await uninstall(["y", "n", "n"], uninstallPaths(), d);
	expect(code).toBe(1);
	expect(text).toContain(`kept ${legacy}: setup may not have made it, and it isn't empty`);
	expect(text).toContain(`kept ${made}: it no longer holds jocasta.json`);
	expect(existsSync(join(legacy, "his.txt"))).toBe(true);
	expect(existsSync(join(made, "other.txt"))).toBe(true);
});

test("removeRcLine takes install.sh's PATH line out of the rc file and nothing else", () => {
	const rc = join(root, ".bashrc");
	put(rc, `alias ll='ls -l'\n\n${rcPathLine("/opt/jocasta")}\n`);
	expect(removeRcLine(rc, "/opt/jocasta")).toBe(true);
	expect(readFileSync(rc, "utf8")).toBe("alias ll='ls -l'\n");
	expect(removeRcLine(rc, "/opt/jocasta")).toBe(false);
});
