// Per-target compile and packaging (0043, 0044, 0552): `jocasta.exe` (the command, the Jocasta service it starts and
// `jocasta mcp`, with whiteboard's canvas page inside) and the shipped `config/` beside the exe (0392); whiteboard's
// runtime with its Node 24, its canvas build and Monaco's worker (0393, 0411, 0417), and hunk (0394).
// Usage: bun run build --target <windows-x64|linux-x64|darwin-arm64>
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	cpSync,
	existsSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	realpathSync,
	renameSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, parse, resolve } from "node:path";
import { $, type BuildConfig } from "bun";

const TARGETS = ["windows-x64", "linux-x64", "darwin-arm64"] as const;
type Target = (typeof TARGETS)[number];

const root = resolve(import.meta.dir, "..");
const dist = join(root, "dist");

function parseTarget(argv: string[]): Target {
	const i = argv.indexOf("--target");
	const value = i >= 0 ? argv[i + 1] : undefined;
	if (!value || !(TARGETS as readonly string[]).includes(value)) {
		console.error(`build: --target must be one of ${TARGETS.join(", ")}`);
		process.exit(2);
	}
	return value as Target;
}

const uiIndex = join(root, "src/frontier/canvas/index.html");
const sha1 = (path: string) => createHash("sha1").update(readFileSync(path)).digest("hex");

/**
 * Bun.build with a guard. Bun has overwritten a page's `index.html` with the built page when a build's output
 * sat on another drive than its sources (twice on 2026-10-09), so every output stays on the repo's drive and the build
 * fails if that file changed.
 */
async function guardedBuild(config: BuildConfig): Promise<void> {
	const drive = (path: string) => parse(resolve(path)).root.toLowerCase();
	for (const path of [
		config.outdir,
		config.root,
		config.compile && typeof config.compile === "object" ? config.compile.outfile : undefined,
	]) {
		if (path && drive(path) !== drive(root)) {
			console.error(`build: ${path} is not on the repo's drive (${drive(root)}); Bun can overwrite sources then`);
			process.exit(1);
		}
	}
	const before = sha1(uiIndex);
	const result = await Bun.build(config);
	if (sha1(uiIndex) !== before) {
		console.error(`build: ${uiIndex} changed during the build; restore it before building again`);
		process.exit(1);
	}
	if (!result.success) {
		for (const log of result.logs) console.error(log);
		process.exit(1);
	}
}

/** The target this machine builds natively: whiteboard's dependencies and hunk come installed for it only. */
const HOST: Target | undefined =
	process.platform === "win32" && process.arch === "x64"
		? "windows-x64"
		: process.platform === "linux" && process.arch === "x64"
			? "linux-x64"
			: process.platform === "darwin" && process.arch === "arm64"
				? "darwin-arm64"
				: undefined;

/** whiteboard v0.2.4, a plain clone at the tag until it becomes a submodule (0393, 0417); never modified. */
const WHITEBOARD = join(root, "forks", "whiteboard");
/** The commit tag v0.2.4 names (0383, 0417): a clone anywhere else is refused (SEC-7). */
const WHITEBOARD_COMMIT = "5b00f0e66d034e187a29d270b3690a37b57ccf7b";
/**
 * The pnpm whiteboard's `packageManager` names (`pnpm@11.1.2`), with the SHA-512 of its npm tarball (npm's `dist.integrity`
 * as hex), so Corepack verifies what it downloads (0430). It goes into a Corepack home of the build's own before the
 * clone's `pnpm` runs, so the unhashed field in the clone resolves to this verified copy.
 */
const PNPM =
	"pnpm@11.1.2+sha512.415a1cc25974731e75455c1468371be74c5aa5fb7621b50d4056d222451609f11412f23fd602e6169f1e060466641f798597e1be961a10688836a67b16569499";
const WHITEBOARD_PACKAGE = join(WHITEBOARD, "packages", "review");
const CANVAS_BUILD = join(WHITEBOARD_PACKAGE, "app", "dist", "desktop");
const CACHE = join(root, "node_modules", ".cache");

/**
 * Node for whiteboard's runtime (0393; plan appendix B): the newest 24.x LTS at F3 (2026-10-13), each archive's
 * SHA-256 copied from https://nodejs.org/dist/v24.21.0/SHASUMS256.txt.
 */
const NODE_VERSION = "24.21.0";
const NODE_ARCHIVES: Record<Target, { file: string; sha256: string; bin: string }> = {
	"windows-x64": {
		file: `node-v${NODE_VERSION}-win-x64.zip`,
		sha256: "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541",
		bin: "node.exe",
	},
	"linux-x64": {
		file: `node-v${NODE_VERSION}-linux-x64.tar.xz`,
		sha256: "fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6",
		bin: "bin/node",
	},
	"darwin-arm64": {
		file: `node-v${NODE_VERSION}-darwin-arm64.tar.gz`,
		sha256: "bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057",
		bin: "bin/node",
	},
};

function fail(message: string): never {
	console.error(`build: ${message}`);
	process.exit(1);
}

/** Windows ships bsdtar in System32; Git's GNU tar earlier on PATH would read `D:` as a host. */
const TAR = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";

/**
 * The clone's commit, checked against `WHITEBOARD_COMMIT`; without git (no `git` or no `.git`), undefined and the
 * clone's content identifies it instead.
 */
function whiteboardCommit(): string | undefined {
	const result = spawnSync("git", ["-C", WHITEBOARD, "rev-parse", "HEAD"], { encoding: "utf8" });
	const head = result.status === 0 ? result.stdout.trim() : "";
	if (!head) return undefined;
	if (head !== WHITEBOARD_COMMIT) {
		fail(`${WHITEBOARD} is at ${head}, not whiteboard v0.2.4 (${WHITEBOARD_COMMIT}): check it out at the tag (0417)`);
	}
	return head;
}

/** A hash of every file in the clone outside `node_modules`, `.git` and `dist` folders: its rebuild key without git. */
function whiteboardContent(): string {
	const hash = createHash("sha256");
	const walk = (dir: string) => {
		for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
			const path = join(dir, entry.name);
			if (entry.isDirectory()) {
				if (!["node_modules", ".git", "dist"].includes(entry.name)) walk(path);
			} else if (entry.isFile()) {
				hash.update(`${path.slice(WHITEBOARD.length)}\0`).update(readFileSync(path));
			}
		}
	};
	walk(WHITEBOARD);
	return hash.digest("hex");
}

/**
 * Builds the clone with its own toolchain (0393): pnpm 11 through Corepack on Node 24, `install --frozen-lockfile`,
 * the runtime package, its tutorial's assets (`tutorial/pins.json` and the sample repo's git stub, which the tutorial
 * the canvas's empty state opens needs; 0419), then the canvas (that order: the canvas imports the runtime's build).
 * Reused when the clone's commit (or, without git, its content after the last build) matches and every output is
 * there.
 */
function buildWhiteboard(): void {
	if (!existsSync(join(WHITEBOARD, "pnpm-workspace.yaml"))) {
		fail(
			`${WHITEBOARD} is missing: clone devdotfast/whiteboard at tag v0.2.4 there (0417): git clone --depth 1 --branch v0.2.4 https://github.com/devdotfast/whiteboard.git forks/whiteboard`,
		);
	}
	const commit = whiteboardCommit();
	const key = () => JSON.stringify(commit ? { head: commit } : { content: whiteboardContent() });
	const stampFile = join(CACHE, "whiteboard", "built.json");
	const built =
		existsSync(join(WHITEBOARD_PACKAGE, "dist", "cli.js")) &&
		existsSync(join(WHITEBOARD_PACKAGE, "tutorial", "pins.json")) &&
		existsSync(join(CANVAS_BUILD, ".vite", "manifest.json"));
	if (built && existsSync(stampFile) && readFileSync(stampFile, "utf8") === key()) {
		console.log("build: whiteboard is built from this clone; reusing it");
		return;
	}
	const node = spawnSync("node", ["--version"], { encoding: "utf8", shell: true }).stdout.trim();
	if (!node.startsWith("v24."))
		fail(`whiteboard builds on Node 24 with Corepack; \`node --version\` is ${node || "missing"}`);
	const shims = join(CACHE, "whiteboard", "pnpm-shim");
	mkdirSync(shims, { recursive: true });
	const env = {
		...process.env,
		PATH: `${shims}${process.platform === "win32" ? ";" : ":"}${process.env.PATH ?? ""}`,
		// The build's own Corepack home: pnpm in it is the hash-checked one, never another project's download.
		COREPACK_HOME: join(CACHE, "whiteboard", "corepack"),
		DO_NOT_TRACK: "1",
		COREPACK_ENABLE_DOWNLOAD_PROMPT: "0",
	};
	const run = (argv: string[]) => {
		console.log(`build: ${argv.join(" ")}`);
		// A step that outlives 20 minutes is hung (a pnpm install once finished its work and never exited): fail loudly.
		const result = spawnSync(argv[0] as string, argv.slice(1), {
			cwd: WHITEBOARD,
			env,
			stdio: "inherit",
			shell: true,
			timeout: 20 * 60_000,
		});
		if (result.status !== 0) fail(`\`${argv.join(" ")}\` exited ${result.status}`);
	};
	run(["corepack", "enable", "--install-directory", JSON.stringify(shims), "pnpm"]);
	run(["corepack", "install", "--global", PNPM]);
	// Never prompt: with no TTY a modules-purge question would hang the build.
	run(["pnpm", "install", "--frozen-lockfile", "--config.confirm-modules-purge=false"]);
	run(["pnpm", "--filter", "@dev.fast/whiteboard", "build"]);
	run(["pnpm", "--filter", "@dev.fast/whiteboard", "build:tutorial-assets"]);
	run(["pnpm", "--filter", "@dev.fast/review-canvas", "build"]);
	mkdirSync(dirname(stampFile), { recursive: true });
	// Without git the key is taken after the build, so the outputs it wrote outside `dist` are part of it.
	writeFileSync(stampFile, key());
}

/** A package's directory as Node would resolve `name` from `from`, through the clone's pnpm links. */
function resolvePackage(name: string, from: string): string | undefined {
	for (let dir = from; dir.startsWith(WHITEBOARD); dir = dirname(dir)) {
		const candidate = join(dir, "node_modules", name);
		if (existsSync(join(candidate, "package.json"))) return realpathSync(candidate);
		if (dirname(dir) === dir) break;
	}
	return undefined;
}

interface PackageJson {
	name?: string;
	version?: string;
	dependencies?: Record<string, string>;
	optionalDependencies?: Record<string, string>;
}

const readPackage = (dir: string): PackageJson => JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));

/**
 * whiteboard's runtime package as npm would install it (0393): its published files and its production dependencies,
 * copied from the clone's frozen install (so the versions are the lockfile's) into a hoisted `node_modules`; a second
 * version of a name nests under its dependent. Optional dependencies are copied where installed (this platform's
 * binaries: diffr, sharp).
 */
function shipWhiteboardRuntime(dest: string): void {
	const pkg = readPackage(WHITEBOARD_PACKAGE);
	const files = [
		"package.json",
		"LICENSE",
		"README.md",
		"THIRD_PARTY_NOTICES.md",
		"onboarding.md",
		"dist",
		"instructions",
		"tutorial",
		"src",
	];
	for (const file of files) {
		cpSync(join(WHITEBOARD_PACKAGE, file), join(dest, file), {
			recursive: true,
			// npm's `files`: no tutorial bundle, no tests or test helpers.
			filter: (path) => !/[\\/]tutorial[\\/]\.bundle([\\/]|$)|\.test\.ts$|test-utils/.test(path),
		});
	}
	const placed = new Map<string, string>();
	const copy = (name: string, from: string, into: string, optional: boolean) => {
		const source = resolvePackage(name, from);
		if (!source) {
			if (optional) return;
			fail(`whiteboard's dependency ${name} isn't installed under ${from}`);
		}
		const version = readPackage(source).version ?? "";
		const top = placed.get(name);
		if (top === version) return;
		const target = top === undefined ? join(dest, "node_modules", name) : join(into, "node_modules", name);
		if (top === undefined) placed.set(name, version);
		cpSync(source, target, {
			recursive: true,
			dereference: true,
			filter: (path) => !path.slice(source.length).split(/[\\/]/).includes("node_modules"),
		});
		const manifest = readPackage(source);
		for (const dep of Object.keys(manifest.dependencies ?? {})) copy(dep, source, target, false);
		for (const dep of Object.keys(manifest.optionalDependencies ?? {})) copy(dep, source, target, true);
	};
	for (const dep of Object.keys(pkg.dependencies ?? {})) copy(dep, WHITEBOARD_PACKAGE, dest, false);
	for (const dep of Object.keys(pkg.optionalDependencies ?? {})) copy(dep, WHITEBOARD_PACKAGE, dest, true);
}

/**
 * Node for the target, downloaded once into the cache and checked against its pinned SHA-256, with its LICENSE. The
 * archive is unpacked into a temp folder and renamed into place with a marker naming its hash, so an interrupted
 * unpack is never reused.
 */
async function shipNode(target: Target, dest: string): Promise<void> {
	const archive = NODE_ARCHIVES[target];
	const cached = join(CACHE, "node", archive.file);
	if (!existsSync(cached)) {
		const url = `https://nodejs.org/dist/v${NODE_VERSION}/${archive.file}`;
		console.log(`build: downloading ${url}`);
		let response: Response;
		try {
			response = await fetch(url);
		} catch (error) {
			fail(`couldn't download Node ${NODE_VERSION}: ${error instanceof Error ? error.message : String(error)}`);
		}
		if (!response.ok) fail(`${url} answered ${response.status}`);
		mkdirSync(dirname(cached), { recursive: true });
		writeFileSync(`${cached}.part`, Buffer.from(await response.arrayBuffer()));
		renameSync(`${cached}.part`, cached);
	}
	const sha = createHash("sha256").update(readFileSync(cached)).digest("hex");
	if (sha !== archive.sha256) {
		rmSync(cached);
		fail(`${archive.file} has SHA-256 ${sha}, not the pinned ${archive.sha256}; removed it`);
	}
	const name = archive.file.replace(/\.(zip|tar\.xz|tar\.gz)$/, "");
	const unpacked = join(CACHE, "node", name);
	const marker = join(unpacked, ".jocasta-archive-sha256");
	const complete =
		existsSync(marker) && readFileSync(marker, "utf8") === sha && existsSync(join(unpacked, archive.bin));
	if (!complete) {
		rmSync(unpacked, { recursive: true, force: true });
		const staging = join(CACHE, "node", `${name}.unpacking`);
		rmSync(staging, { recursive: true, force: true });
		mkdirSync(staging, { recursive: true });
		await $`${TAR} -xf ${cached} -C ${staging}`;
		writeFileSync(join(staging, name, ".jocasta-archive-sha256"), sha);
		renameSync(join(staging, name), unpacked);
		rmSync(staging, { recursive: true, force: true });
	}
	mkdirSync(dest, { recursive: true });
	cpSync(join(unpacked, archive.bin), join(dest, parse(archive.bin).base));
	cpSync(join(unpacked, "LICENSE"), join(dest, "LICENSE"));
}

/** hunk's binary for the target (0394), from its pinned npm platform package, with its LICENSE. */
function shipHunk(target: Target, dest: string): void {
	const platform = { "windows-x64": "windows-x64", "linux-x64": "linux-x64", "darwin-arm64": "darwin-arm64" }[target];
	const pkg = join(root, "node_modules", `hunkdiff-${platform}`);
	const bin = target.startsWith("windows") ? "hunk.exe" : "hunk";
	if (!existsSync(join(pkg, "bin", bin))) fail(`${pkg} has no bin/${bin}: run \`bun install\` on a ${target} machine`);
	mkdirSync(dest, { recursive: true });
	cpSync(join(pkg, "bin", bin), join(dest, bin));
	cpSync(join(pkg, "LICENSE"), join(dest, "LICENSE"));
}

/**
 * Empties `dist/<target>`. A file there held open (a `jocasta.exe` or the service run from it, or omp's `jocasta mcp`
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
			`${path ?? out} is in use (${code}), so ${out} can't be replaced.${holders ? ` Running from it:\n${holders}\n` : " "}Close what runs from there (the Jocasta service, \`jocasta\`, or an omp session's \`jocasta mcp\`), or build a copy elsewhere, then build again.`,
		);
	}
}

async function build(target: Target): Promise<void> {
	const out = join(dist, target);
	clearOut(out);
	mkdirSync(out, { recursive: true });
	const exe = join(out, target.startsWith("windows") ? "jocasta.exe" : "jocasta");

	// The command, its service and the service's whiteboard canvas page (an HTML import, bundled into the exe), with
	// the push-to-talk hook's worker as its own entry so the exe carries it (0552, 0556).
	await guardedBuild({
		entrypoints: [join(root, "src/entry.ts"), join(root, "src/frontier/server/ptt-worker.ts")],
		root,
		compile: { target: `bun-${target}`, outfile: exe, autoloadBunfig: false },
		minify: true,
		define: { "process.env.NODE_ENV": '"production"' },
	});

	cpSync(join(root, "README.md"), join(out, "README.md"));
	cpSync(join(root, "CHANGELOG.md"), join(out, "CHANGELOG.md"));
	// The boss manual and the templates `jocasta setup` writes from, beside the exe (0392, 0396, 0401).
	cpSync(join(root, "config"), join(out, "config"), { recursive: true });

	// whiteboard (0393) and hunk (0394) ship native pieces installed for this machine's platform only, so they join
	// the target this machine is; elsewhere `jocasta setup` says whiteboard isn't there.
	buildWhiteboard();
	if (target === HOST) {
		shipWhiteboardRuntime(join(out, "whiteboard"));
		await shipNode(target, join(out, "node"));
		shipHunk(target, join(out, "hunk"));
	} else {
		console.log(`build: ${target} isn't this machine's platform; whiteboard, Node and hunk are left out`);
	}
	// whiteboard's canvas build and its LICENSE, served to the canvas page from beside the exe (0393, whiteboard.ts).
	cpSync(CANVAS_BUILD, join(out, "whiteboard-canvas"), { recursive: true });
	cpSync(join(WHITEBOARD, "LICENSE"), join(out, "whiteboard-canvas", "LICENSE"));
	// Monaco's editor worker (0411), its own module beside the exe; the stylesheet its imports drag in is dead.
	await guardedBuild({
		entrypoints: [join(root, "node_modules/monaco-editor/esm/vs/editor/editor.worker.js")],
		root,
		outdir: out,
		target: "browser",
		format: "esm",
		minify: true,
		naming: { entry: "editor.worker.[ext]", asset: "[name]-[hash].[ext]" },
	});
	rmSync(join(out, "editor.worker.css"), { force: true });

	await pack(target, out);
}

async function pack(target: Target, out: string): Promise<void> {
	const archive = target.startsWith("windows") ? `jocasta-${target}.zip` : `jocasta-${target}.tar.gz`;
	const archivePath = join(dist, archive);
	rmSync(archivePath, { force: true });
	if (archive.endsWith(".zip")) {
		if (process.platform === "win32") await $`${TAR} -a -c -f ${archivePath} *`.cwd(out);
		else await $`zip -qr ${archivePath} .`.cwd(out);
	} else {
		await $`tar -czf ${archivePath} .`.cwd(out);
	}
	const hash = createHash("sha256").update(readFileSync(archivePath)).digest("hex");
	writeFileSync(`${archivePath}.sha256`, `${hash}  ${archive}\n`);
	console.log(`build: ${archive} ${hash}`);
}

await build(parseTarget(process.argv.slice(2)));
