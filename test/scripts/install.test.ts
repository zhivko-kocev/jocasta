// The install scripts install a local build (JOCASTA_INSTALL_FROM, 0487) and refuse an archive whose SHA-256 doesn't
// match (0484). Everything happens in a temp folder: the install folder, the source folder, TEMP and LOCALAPPDATA;
// install.ps1 runs with JOCASTA_INSTALL_NO_PATH=1 and the user PATH is checked unchanged, and install.sh runs with a
// temp HOME. No network: only the local-folder path is exercised.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { rcPathLine, readInstallRecord } from "../../src/setup/uninstall.ts";

const SCRIPTS = resolve(import.meta.dir, "..", "..", "scripts");
const windows = process.platform === "win32";
const sh = Bun.which("sh");
const ZIP = "jocasta-windows-x64.zip";
const TAR = "jocasta-linux-x64.tar.gz";

let root = "";
let from = "";
let dir = "";

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), "jocasta-install-test-"));
	from = join(root, "from");
	dir = join(root, "install");
	mkdirSync(from);
	mkdirSync(join(root, "temp"));
	mkdirSync(join(root, "localappdata"));
});

afterEach(() => {
	rmSync(root, { recursive: true, force: true });
});

const sha256 = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");

/** Writes `<archive>.sha256` beside the archive as the build does: `<hash>  <name>`. */
function writeHash(archive: string, hash = sha256(join(from, archive))) {
	writeFileSync(join(from, `${archive}.sha256`), `${hash}  ${archive}\n`);
}

function run(cmd: string[], env: Record<string, string>) {
	const result = Bun.spawnSync(cmd, { env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });
	return { code: result.exitCode, out: result.stdout.toString(), err: result.stderr.toString() };
}

// --- install.ps1 ---

/** Reads the user PATH as stored in the registry. */
const USER_PATH_QUERY = ["reg", "query", "HKCU\\Environment", "/v", "Path"];

/** A fake windows-x64 zip: a harmless stand-in for jocasta.exe (whoami, which only complains about `--version`). */
function fakeZip() {
	const stage = join(root, "stage");
	mkdirSync(join(stage, "config"), { recursive: true });
	copyFileSync(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "whoami.exe"), join(stage, "jocasta.exe"));
	writeFileSync(join(stage, "config", "boss.md"), "fake config\n");
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

test.if(windows)(
	"install.ps1 installs a local build from JOCASTA_INSTALL_FROM into JOCASTA_INSTALL_DIR, leaving PATH alone",
	() => {
		fakeZip();
		writeHash(ZIP);
		const pathBefore = run(USER_PATH_QUERY, {}).out;
		const result = installPs1();
		expect(result.code).toBe(0);
		expect(result.out).toContain(`jocasta install: installing ${from}\\${ZIP}.`);
		expect(readFileSync(join(dir, "jocasta.exe"))).toEqual(readFileSync(join(root, "stage", "jocasta.exe")));
		expect(readFileSync(join(dir, "config", "boss.md"), "utf8")).toBe("fake config\n");
		expect(readInstallRecord(dir, join(root, "localappdata"), true)).toEqual({
			dir,
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
		const result = installPs1();
		expect(result.code).not.toBe(0);
		expect(result.err).toContain(
			`jocasta install: ${from} has no ${ZIP}.sha256; JOCASTA_INSTALL_FROM names a folder holding ${ZIP} and ${ZIP}.sha256.`,
		);
		expect(existsSync(dir)).toBe(false);
	},
	60_000,
);

// --- install.sh (wherever a POSIX sh is on PATH; on Windows, Git's) ---

/** Forward slashes, which both MSYS's sh and POSIX take. */
const posix = (path: string) => path.replaceAll("\\", "/");

/** A fake linux-x64 archive and a `uname` that reports Linux x86_64, so the test runs on any platform. */
function fakeTar() {
	const stage = join(root, "stage");
	const bin = join(root, "bin");
	mkdirSync(stage);
	mkdirSync(bin);
	writeFileSync(join(stage, "jocasta"), '#!/bin/sh\necho "jocasta 0.0.0-test"\n');
	writeFileSync(
		join(bin, "uname"),
		'#!/bin/sh\ncase "$1" in -s) echo Linux ;; -m) echo x86_64 ;; *) echo Linux ;; esac\n',
	);
	// Relative archive paths: GNU tar reads `C:/…` as a remote host.
	const packed = run(
		[
			sh as string,
			"-c",
			`chmod +x "$1/uname" "$2/jocasta" && cd "$2" && tar -czf "../from/${TAR}" jocasta`,
			"sh",
			posix(bin),
			posix(stage),
		],
		{},
	);
	expect(packed.err).toBe("");
	expect(packed.code).toBe(0);
	return bin;
}

function installSh(bin: string, home: string) {
	// The fake `uname` goes first on PATH inside sh: Git's sh.exe puts its own /usr/bin first when it starts, and
	// MSYS wants the folder as a POSIX path (cygpath, where there is one).
	const prefixPath = 'bin="$(cygpath -u "$1" 2>/dev/null || printf %s "$1")"; PATH="$bin:$PATH" exec sh "$2"';
	return run([sh as string, "-c", prefixPath, "sh", posix(bin), posix(join(SCRIPTS, "install.sh"))], {
		HOME: posix(home),
		SHELL: "",
		// No TMPDIR: mktemp's default keeps $tmp a POSIX path, which tar needs (see fakeTar).
		JOCASTA_INSTALL_DIR: posix(dir),
		JOCASTA_INSTALL_FROM: posix(from),
	});
}

test.if(sh !== null)(
	"install.sh installs a local build from JOCASTA_INSTALL_FROM and adds its PATH line to the temp HOME's rc file",
	() => {
		const bin = fakeTar();
		writeHash(TAR);
		const home = join(root, "home");
		mkdirSync(home);
		const result = installSh(bin, home);
		expect(result.err).toBe("");
		expect(result.code).toBe(0);
		expect(result.out).toContain(`jocasta install: installing ${posix(from)}/${TAR}.`);
		expect(result.out).toContain("jocasta 0.0.0-test");
		const record = JSON.parse(readFileSync(join(dir, "install.json"), "utf8")) as { dir: string; rc: string };
		// MSYS shows HOME as a POSIX path (/tmp/…), so only the file's place in HOME is compared.
		expect(record.rc.endsWith("/home/.profile")).toBe(true);
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
		mkdirSync(home);
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
