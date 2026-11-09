// The install scripts install a local build (JOCASTA_INSTALL_FROM, 0487), refuse an archive whose SHA-256 doesn't
// match (0484), refuse a folder that isn't Jocasta's (0579) and upgrade without leaving stale files (0579, CR-06).
// Everything happens in a temp folder: the install folder, the source folder, TEMP and LOCALAPPDATA; install.ps1 runs
// with JOCASTA_INSTALL_NO_PATH=1 and the user PATH is checked unchanged, and install.sh runs with a temp HOME. No
// network: only the local-folder path is exercised.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	realpathSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { rcPathLine, readInstallRecord } from "../../src/setup/uninstall.ts";
import { removeTree } from "../../src/shared/fsx.ts";

const SCRIPTS = resolve(import.meta.dir, "..", "..", "scripts");
const windows = process.platform === "win32";
const sh = Bun.which("sh");
const ZIP = "jocasta-windows-x64.zip";
const TAR = "jocasta-linux-x64.tar.gz";
const SYSTEM32 = join(process.env.SystemRoot ?? "C:\\Windows", "System32");

let root = "";
let from = "";
let dir = "";
/** Processes a test started, ended after it. */
let held: { kill(): void; exited: Promise<number> }[] = [];

beforeEach(() => {
	// The long path: %TEMP% can be an 8.3 name (`RUNNER~1` on CI), and PowerShell prints the long one.
	root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-install-test-")));
	from = join(root, "from");
	dir = join(root, "install");
	mkdirSync(from);
	mkdirSync(join(root, "temp"));
	mkdirSync(join(root, "localappdata"));
	held = [];
});

afterEach(async () => {
	for (const child of held) child.kill();
	await Promise.all(held.map((child) => child.exited));
	removeTree(root);
});

const sha256 = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");

const put = (path: string, text: string) => {
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, text);
};

/** Writes `<archive>.sha256` beside the archive as the build does: `<hash>  <name>`. */
function writeHash(archive: string, hash = sha256(join(from, archive))) {
	writeFileSync(join(from, `${archive}.sha256`), `${hash}  ${archive}\n`);
}

function run(cmd: string[], env: Record<string, string>) {
	const merged: Record<string, string | undefined> = { ...process.env, ...env };
	// Started from PowerShell 7 (CI's shell), PSModulePath lists PowerShell 7's modules first, and Windows PowerShell
	// then can't load its own `Get-FileHash`; without the variable it builds its default.
	for (const key of Object.keys(merged)) if (key.toLowerCase() === "psmodulepath") delete merged[key];
	const result = Bun.spawnSync(cmd, { env: merged, stdout: "pipe", stderr: "pipe" });
	return { code: result.exitCode, out: result.stdout.toString(), err: result.stderr.toString() };
}

// --- install.ps1 ---

/** Reads the user PATH as stored in the registry. */
const USER_PATH_QUERY = ["reg", "query", "HKCU\\Environment", "/v", "Path"];

/**
 * A fake windows-x64 zip: a harmless stand-in for jocasta.exe (whoami, which only complains about `--version`), a
 * config file with `config` in it, and with `hunk` a copy of PING.EXE as hunk.exe, which a test can keep running.
 */
function fakeZip(config = "fake config\n", hunk = false) {
	const stage = join(root, "stage");
	removeTree(stage);
	removeTree(join(from, ZIP));
	mkdirSync(join(stage, "config"), { recursive: true });
	copyFileSync(join(SYSTEM32, "whoami.exe"), join(stage, "jocasta.exe"));
	writeFileSync(join(stage, "config", "boss.md"), config);
	if (hunk) {
		mkdirSync(join(stage, "hunk"));
		copyFileSync(join(SYSTEM32, "PING.EXE"), join(stage, "hunk", "hunk.exe"));
	}
	const zipped = run(
		[
			"powershell",
			"-NoProfile",
			"-Command",
			`Compress-Archive -Path '${stage}\\*' -DestinationPath '${join(from, ZIP)}'`,
		],
		{},
	);
	expect(zipped.code).toBe(0);
	writeHash(ZIP);
}

/** install.ps1 as `irm … | iex` runs it, with a refusal printed as its bare message (no console wrapping). */
function installPs1() {
	const script = join(SCRIPTS, "install.ps1");
	const command = `try { & '${script}' } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }; exit 0`;
	return run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
		JOCASTA_INSTALL_DIR: dir,
		JOCASTA_INSTALL_FROM: from,
		JOCASTA_INSTALL_NO_PATH: "1",
		LOCALAPPDATA: join(root, "localappdata"),
		TEMP: join(root, "temp"),
		TMP: join(root, "temp"),
	});
}

/** Starts `exe` (a copy of PING.EXE) so it runs for a while, holding its file. */
function holdRunning(exe: string) {
	const child = Bun.spawn([exe, "-n", "60", "127.0.0.1"], { stdout: "ignore", stderr: "ignore", windowsHide: true });
	held.push(child);
	return child;
}

test.if(windows)(
	"install.ps1 installs a local build from JOCASTA_INSTALL_FROM into JOCASTA_INSTALL_DIR, leaving PATH alone",
	() => {
		fakeZip();
		const pathBefore = run(USER_PATH_QUERY, {}).out;
		const result = installPs1();
		expect(result.err.trim()).toBe("");
		expect(result.code).toBe(0);
		expect(result.out).toContain(`jocasta install: installing ${from}\\${ZIP}.`);
		expect(readFileSync(join(dir, "jocasta.exe"))).toEqual(readFileSync(join(root, "stage", "jocasta.exe")));
		expect(readFileSync(join(dir, "config", "boss.md"), "utf8")).toBe("fake config\n");
		// The release's top-level entries are recorded: they are what uninstall takes out (0579).
		expect(readInstallRecord(dir, join(root, "localappdata"), true)).toEqual({
			dir,
			entries: ["config", "jocasta.exe"],
			autostart: "Jocasta",
			appData: join(root, "localappdata", "local.jocasta.frontier"),
		});
		expect(existsSync(join(root, "localappdata", "Programs"))).toBe(false);
		expect(run(USER_PATH_QUERY, {}).out).toBe(pathBefore);
	},
	60_000,
);

test.if(windows)(
	"install.ps1 refuses a zip whose SHA-256 doesn't match its .sha256 and installs nothing",
	() => {
		fakeZip();
		writeHash(ZIP, "0".repeat(64));
		const result = installPs1();
		expect(result.code).not.toBe(0);
		expect(result.err).toContain(
			`jocasta install: ${ZIP} doesn't match the SHA-256 in ${from}\\${ZIP}.sha256 (expected ${"0".repeat(64)}, got ${sha256(join(from, ZIP))}); nothing was installed.`,
		);
		expect(existsSync(dir)).toBe(false);
	},
	60_000,
);

test.if(windows)(
	"install.ps1 refuses a JOCASTA_INSTALL_FROM folder without the zip's .sha256",
	() => {
		fakeZip();
		removeTree(join(from, `${ZIP}.sha256`));
		const result = installPs1();
		expect(result.code).not.toBe(0);
		expect(result.err).toContain(
			`jocasta install: ${from} has no ${ZIP}.sha256; JOCASTA_INSTALL_FROM names a folder holding ${ZIP} and ${ZIP}.sha256.`,
		);
		expect(existsSync(dir)).toBe(false);
	},
	60_000,
);

test.if(windows)(
	"install.ps1 refuses a folder with files and no install.json naming it, a jocasta.exe there included (0579)",
	() => {
		fakeZip();
		put(join(dir, "jocasta.exe"), "his copy");
		put(join(dir, "tool.exe"), "his");
		const refusal = `jocasta install: ${dir} holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR.`;
		expect(installPs1().err).toContain(refusal);
		// An install.json naming another folder is no claim either.
		put(join(dir, "install.json"), JSON.stringify({ dir: join(root, "elsewhere") }));
		expect(installPs1().err).toContain(refusal);
		expect(readdirSync(dir).sort()).toEqual(["install.json", "jocasta.exe", "tool.exe"]);
		expect(readFileSync(join(dir, "jocasta.exe"), "utf8")).toBe("his copy");
	},
	60_000,
);

test.if(windows)(
	"install.ps1 upgrades over running programs: a busy hunk.exe moves aside, an old companion stops, install.json stays (CR-06)",
	async () => {
		fakeZip("fake config\n", true);
		expect(installPs1().code).toBe(0);
		// An omp session's hunk pane, and an earlier install's companion under app\.
		const hunk = holdRunning(join(dir, "hunk", "hunk.exe"));
		mkdirSync(join(dir, "app", "bin"), { recursive: true });
		copyFileSync(join(SYSTEM32, "PING.EXE"), join(dir, "app", "bin", "launcher.exe"));
		const companion = holdRunning(join(dir, "app", "bin", "launcher.exe"));

		fakeZip("the newer config\n", true);
		const upgrade = installPs1();
		expect(upgrade.err.trim()).toBe("");
		expect(upgrade.code).toBe(0);
		expect(upgrade.out).toContain("jocasta install: stopping the companion app an earlier install left running.");
		expect(upgrade.out).toContain("omp sessions and hunk panes started before the upgrade run the old build");
		await companion.exited;
		expect(existsSync(join(dir, "app"))).toBe(false);
		expect(readFileSync(join(dir, "config", "boss.md"), "utf8")).toBe("the newer config\n");
		expect(readdirSync(join(dir, "hunk")).some((name) => /^hunk\.exe\.old-\d+$/.test(name))).toBe(true);
		expect(readInstallRecord(dir, join(root, "localappdata"), true)?.dir).toBe(dir);

		// Once hunk exits, the next install removes what moved aside.
		hunk.kill();
		await hunk.exited;
		expect(installPs1().code).toBe(0);
		expect(readdirSync(join(dir, "hunk"))).toEqual(["hunk.exe"]);
	},
	120_000,
);

test.if(windows)(
	"install.ps1 puts the old install back when the copy fails (CR-06)",
	async () => {
		fakeZip();
		expect(installPs1().code).toBe(0);
		const before = readFileSync(join(dir, "jocasta.exe"));
		// A file held open for reading only: it can be copied aside, but robocopy can't replace it.
		const holder = Bun.spawn(
			[
				"powershell",
				"-NoProfile",
				"-Command",
				`$f = [IO.File]::Open('${join(dir, "config", "boss.md")}', 'Open', 'Read', 'Read'); [Console]::Out.WriteLine('held'); Start-Sleep -Seconds 60`,
			],
			{ stdout: "pipe", stderr: "ignore", windowsHide: true },
		);
		held.push(holder);
		const reader = holder.stdout.getReader();
		await reader.read();
		reader.releaseLock();

		fakeZip("the newer config\n");
		const failed = installPs1();
		expect(failed.code).not.toBe(0);
		expect(failed.err).toContain("jocasta install: the copy failed (");
		expect(failed.err).toContain("); the installed Jocasta is as it was.");
		holder.kill();
		await holder.exited;
		expect(readFileSync(join(dir, "jocasta.exe"))).toEqual(before);
		expect(readFileSync(join(dir, "config", "boss.md"), "utf8")).toBe("fake config\n");
		expect(readInstallRecord(dir, join(root, "localappdata"), true)?.dir).toBe(dir);
	},
	120_000,
);

// --- install.sh (wherever a POSIX sh is on PATH; on Windows, Git's) ---

/** Forward slashes, which both MSYS's sh and POSIX take. */
const posix = (path: string) => path.replaceAll("\\", "/");

/**
 * A fake linux-x64 archive of `files` (`jocasta` prints a version) and a `uname` that reports Linux x86_64, so the test
 * runs on any platform.
 */
function fakeTar(files: Record<string, string> = {}) {
	const stage = join(root, "stage");
	const bin = join(root, "bin");
	removeTree(stage);
	removeTree(join(from, TAR));
	mkdirSync(stage);
	mkdirSync(bin, { recursive: true });
	put(join(stage, "jocasta"), '#!/bin/sh\necho "jocasta 0.0.0-test"\n');
	for (const [name, text] of Object.entries(files)) put(join(stage, name), text);
	writeFileSync(
		join(bin, "uname"),
		'#!/bin/sh\ncase "$1" in -s) echo Linux ;; -m) echo x86_64 ;; *) echo Linux ;; esac\n',
	);
	// Relative archive paths: GNU tar reads `C:/…` as a remote host.
	const packed = run(
		[
			sh as string,
			"-c",
			`chmod +x "$1/uname" "$2/jocasta" && cd "$2" && tar -czf "../from/${TAR}" *`,
			"sh",
			posix(bin),
			posix(stage),
		],
		{},
	);
	expect(packed.err).toBe("");
	expect(packed.code).toBe(0);
	writeHash(TAR);
	return bin;
}

function installSh(bin: string, home: string, extra: Record<string, string> = {}) {
	// The fake `uname` goes first on PATH inside sh: Git's sh.exe puts its own /usr/bin first when it starts, and
	// MSYS wants the folder as a POSIX path (cygpath, where there is one).
	const prefixPath = 'bin="$(cygpath -u "$1" 2>/dev/null || printf %s "$1")"; PATH="$bin:$PATH" exec sh "$2"';
	mkdirSync(home, { recursive: true });
	return run([sh as string, "-c", prefixPath, "sh", posix(bin), posix(join(SCRIPTS, "install.sh"))], {
		HOME: posix(home),
		SHELL: "",
		// No TMPDIR: mktemp's default keeps $tmp a POSIX path, which tar needs (see fakeTar).
		JOCASTA_INSTALL_DIR: posix(dir),
		JOCASTA_INSTALL_FROM: posix(from),
		...extra,
	});
}

test.if(sh !== null)(
	"install.sh installs a local build from JOCASTA_INSTALL_FROM and adds its PATH line to the temp HOME's rc file",
	() => {
		const bin = fakeTar({ "config/boss.md": "fake config\n" });
		const home = join(root, "home");
		const result = installSh(bin, home);
		expect(result.err).toBe("");
		expect(result.code).toBe(0);
		expect(result.out).toContain(`jocasta install: installing ${posix(from)}/${TAR}.`);
		expect(result.out).toContain("jocasta 0.0.0-test");
		const record = JSON.parse(readFileSync(join(dir, "install.json"), "utf8")) as {
			dir: string;
			rc: string;
			entries: string[];
		};
		// MSYS shows HOME as a POSIX path (/tmp/…), so only the file's place in HOME is compared.
		expect(record.rc.endsWith("/home/.profile")).toBe(true);
		expect(record.entries.sort()).toEqual(["config", "jocasta"]);
		expect(readFileSync(join(home, ".profile"), "utf8")).toContain(rcPathLine(record.dir));
	},
	60_000,
);

test.if(sh !== null)(
	"install.sh refuses an archive whose SHA-256 doesn't match its .sha256 and installs nothing",
	() => {
		const bin = fakeTar();
		writeHash(TAR, "f".repeat(64));
		const home = join(root, "home");
		const result = installSh(bin, home);
		expect(result.code).not.toBe(0);
		expect(result.err).toContain(
			`jocasta install: ${TAR} doesn't match the SHA-256 in ${posix(from)}/${TAR}.sha256 (expected ${"f".repeat(64)}, got ${sha256(join(from, TAR))}); nothing was installed.`,
		);
		expect(existsSync(dir)).toBe(false);
		expect(existsSync(join(home, ".profile"))).toBe(false);
	},
	60_000,
);

test.if(sh !== null)(
	"install.sh refuses a folder that isn't Jocasta's and leaves it as it is (0579)",
	() => {
		const bin = fakeTar();
		put(join(dir, "his-tool"), "his");
		put(join(dir, "jocasta"), "his copy");
		const result = installSh(bin, join(root, "home"));
		expect(result.code).not.toBe(0);
		expect(result.err).toMatch(
			/^jocasta install: \S+install holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR\.$/m,
		);
		expect(readdirSync(dir).sort()).toEqual(["his-tool", "jocasta"]);
		expect(readFileSync(join(dir, "jocasta"), "utf8")).toBe("his copy");
		expect(readdirSync(root).some((name) => name.startsWith("install."))).toBe(false);
	},
	60_000,
);

test.if(sh !== null)(
	"install.sh upgrades its own install by a swap: a file the new release lacks is gone, nothing is left beside it (0579)",
	() => {
		const home = join(root, "home");
		expect(installSh(fakeTar({ "config/old-skill.md": "old\n" }), home, { JOCASTA_INSTALL_NO_PATH: "1" }).code).toBe(0);
		expect(existsSync(join(dir, "config", "old-skill.md"))).toBe(true);
		const upgrade = installSh(fakeTar({ "config/boss.md": "new\n" }), home, { JOCASTA_INSTALL_NO_PATH: "1" });
		expect(upgrade.err).toBe("");
		expect(upgrade.code).toBe(0);
		expect(readdirSync(join(dir, "config"))).toEqual(["boss.md"]);
		expect(readdirSync(root).filter((name) => name.startsWith("install"))).toEqual(["install"]);
		expect(JSON.parse(readFileSync(join(dir, "install.json"), "utf8")).entries.sort()).toEqual(["config", "jocasta"]);
	},
	60_000,
);
