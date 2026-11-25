// Packaging (0043, 0044, 0552, 0636, 0679): `mise run build` runs `cargo build --release --locked` (the `jocasta` product
// and `jocasta-bench`), then this script, which lays out `dist/<target>/`: `target/release/jocasta` beside the shipped
// `config/` (0392) and, on Windows, the ConPTY runtime (0661), packs it and writes the archive's `.sha256`.
// `jocasta-bench` goes to `dist/<target>/bench/` for the VM and stays out of the archive.
// Each target builds on its own OS (0043): the target is this machine's.
// Usage: mise run build
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, parse, resolve } from "node:path";
import { $ } from "bun";

const TARGETS = ["windows-x64", "linux-x64", "darwin-arm64"] as const;
type Target = (typeof TARGETS)[number];

const root = resolve(import.meta.dir, "..");
const dist = join(root, "dist");

/** The target this machine builds natively: the build's binaries are the ones cargo made for it. */
const HOST: Target | undefined =
	process.platform === "win32" && process.arch === "x64"
		? "windows-x64"
		: process.platform === "linux" && process.arch === "x64"
			? "linux-x64"
			: process.platform === "darwin" && process.arch === "arm64"
				? "darwin-arm64"
				: undefined;

/** Downloads (ConPTY) are cached here. */
const CACHE = join(root, "node_modules", ".cache");

function fail(message: string): never {
	console.error(`build: ${message}`);
	process.exit(1);
}

/** Windows ships bsdtar in System32; Git's GNU tar earlier on PATH would read `D:` as a host. */
const TAR = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";

/** The pin of Microsoft's ConPTY runtime (0661, kept by 0683): the package, its files and notices, each with its SHA-256. */
const CONPTY_PIN = join(root, "packaging", "windows", "conpty.json");

type ConptyPin = {
	package: { id: string; version: string; url: string; sha256: string };
	bundles: Record<string, { files: { source: string; destination: string; sha256: string }[] }>;
	notices: { source: string; destination: string; sha256: string }[];
};

/**
 * The Windows build's `conpty\` folder and its notices (0661), laid out as the herdr fork's packaging script did:
 * the vendored pty (`crates/portable-pty`) loads the folder only when every file and the marker are there and match
 * their hashes. The package is downloaded once into the cache and checked against its pinned SHA-256.
 */
async function shipConpty(out: string): Promise<void> {
	const pin = JSON.parse(readFileSync(CONPTY_PIN, "utf8")) as ConptyPin;
	const bundle = pin.bundles.x86_64 ?? fail(`${CONPTY_PIN} has no x86_64 bundle`);
	const cached = join(CACHE, "conpty", `${pin.package.id}.${pin.package.version}.nupkg`);
	if (!existsSync(cached)) {
		console.log(`build: downloading ${pin.package.url}`);
		let response: Response;
		try {
			response = await fetch(pin.package.url);
		} catch (error) {
			fail(
				`couldn't download ConPTY ${pin.package.version}: ${error instanceof Error ? error.message : String(error)}`,
			);
		}
		if (!response.ok) fail(`${pin.package.url} answered ${response.status}`);
		mkdirSync(dirname(cached), { recursive: true });
		writeFileSync(`${cached}.part`, Buffer.from(await response.arrayBuffer()));
		renameSync(`${cached}.part`, cached);
	}
	const sha = createHash("sha256").update(readFileSync(cached)).digest("hex");
	if (sha !== pin.package.sha256) {
		rmSync(cached);
		fail(`${parse(cached).base} has SHA-256 ${sha}, not the pinned ${pin.package.sha256}; removed it`);
	}
	const staging = join(CACHE, "conpty", "unpacking");
	rmSync(staging, { recursive: true, force: true });
	mkdirSync(staging, { recursive: true });
	await $`${TAR} -xf ${cached} -C ${staging} ${bundle.files.map((file) => file.source)}`;
	const copyChecked = (from: string, to: string, sha256: string) => {
		const got = createHash("sha256").update(readFileSync(from)).digest("hex");
		if (got !== sha256) fail(`${from} has SHA-256 ${got}, not the pinned ${sha256}`);
		mkdirSync(dirname(join(out, to)), { recursive: true });
		cpSync(from, join(out, to));
	};
	for (const file of bundle.files) copyChecked(join(staging, file.source), file.destination, file.sha256);
	rmSync(staging, { recursive: true, force: true });
	const files = Object.fromEntries(
		[...bundle.files].sort((a, b) => a.destination.localeCompare(b.destination)).map((f) => [f.destination, f.sha256]),
	);
	const marker = {
		architecture: "x86_64",
		files,
		package: pin.package.id,
		schema_version: 1,
		version: pin.package.version,
	};
	writeFileSync(join(out, "conpty", "herdr-conpty.json"), `${JSON.stringify(marker, null, 2)}\n`);
	for (const notice of pin.notices) {
		copyChecked(join(root, notice.source), notice.destination, notice.sha256);
	}
}

/**
 * Empties `dist/<target>`. A file there held open (a `jocasta.exe` or its server run from it,
 * started from it) fails with the file and, on Windows, the processes running from the folder, instead of a raw EBUSY.
 */
function clearOut(out: string): void {
	try {
		rmSync(out, { recursive: true, force: true });
	} catch (error) {
		const { code, path } = error as NodeJS.ErrnoException;
		if (code !== "EBUSY" && code !== "EPERM" && code !== "ENOTEMPTY") throw error;
		let holders = "";
		if (process.platform === "win32") {
			const script = `Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith('${out.replaceAll("'", "''")}', 'OrdinalIgnoreCase') } | ForEach-Object { "  $($_.Id) $($_.Path)" }`;
			holders = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).stdout.trim();
		}
		fail(
			`${path ?? out} is in use (${code}), so ${out} can't be replaced.${holders ? ` Running from it:\n${holders}\n` : " "}Close what runs from there (\`jocasta\` and its server, or an omp session's \`jocasta mcp\`), or build a copy elsewhere, then build again.`,
		);
	}
}

async function build(): Promise<void> {
	if (!HOST) fail(`${process.platform}-${process.arch} builds none of ${TARGETS.join(", ")}`);
	const target = HOST;
	const out = join(dist, target);
	const ext = target.startsWith("windows") ? ".exe" : "";
	// The last build's archive goes first: a build that fails later leaves nothing installable with
	// JOCASTA_INSTALL_FROM=dist.
	if (existsSync(dist)) {
		for (const name of readdirSync(dist)) if (name.startsWith(`jocasta-${target}.`)) rmSync(join(dist, name));
	}
	const release = join(root, "target", "release");
	for (const name of ["jocasta", "jocasta-bench"]) {
		const exePath = join(release, `${name}${ext}`);
		if (!existsSync(exePath)) fail(`${exePath} is missing: run \`mise run build\`, which runs cargo build first`);
	}
	clearOut(out);
	mkdirSync(out, { recursive: true });
	cpSync(join(release, `jocasta${ext}`), join(out, `jocasta${ext}`));
	// The VM's benchmarks run it; the archive leaves it out (see `pack`).
	mkdirSync(join(out, BENCH), { recursive: true });
	cpSync(join(release, `jocasta-bench${ext}`), join(out, BENCH, `jocasta-bench${ext}`));

	cpSync(join(root, "README.md"), join(out, "README.md"));
	cpSync(join(root, "CHANGELOG.md"), join(out, "CHANGELOG.md"));
	// The boss manual and the templates `jocasta setup` writes from, beside the exe (0392, 0396, 0401).
	cpSync(join(root, "config"), join(out, "config"), { recursive: true });

	if (target === "windows-x64") {
		await shipConpty(out);
		// The release build runs as shipped too (the footprint bench, live checks): the vendored pty loads the bundled
		// runtime only from `conpty\` beside the exe, else it falls back to Windows' own conhost.
		await shipConpty(release);
	}

	await pack(target, out);
}

/** The folder in `dist/<target>/` holding `jocasta-bench`: never in the archive. */
const BENCH = "bench";

async function pack(target: Target, out: string): Promise<void> {
	const archive = target.startsWith("windows") ? `jocasta-${target}.zip` : `jocasta-${target}.tar.gz`;
	const archivePath = join(dist, archive);
	rmSync(archivePath, { force: true });
	const entries = readdirSync(out).filter((name) => name !== BENCH);
	if (archive.endsWith(".zip")) {
		if (process.platform === "win32") await $`${TAR} -a -c -f ${archivePath} ${entries}`.cwd(out);
		else await $`zip -qr ${archivePath} ${entries}`.cwd(out);
	} else {
		await $`tar -czf ${archivePath} ${entries}`.cwd(out);
	}
	const hash = createHash("sha256").update(readFileSync(archivePath)).digest("hex");
	writeFileSync(`${archivePath}.sha256`, `${hash}  ${archive}\n`);
	console.log(`build: ${archive} ${hash}`);
}

await build();
