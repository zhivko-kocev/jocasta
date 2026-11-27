// `mise run omp-seams` (0693): does the omp on PATH still know every setting Jocasta writes? Reads the keys of the
// shipped omp files (the snapshot `config/omp/config.yml`, `boss.yml`, `guard.yml`, `headless.yml`, `heavy.yml`,
// `review.yml`) and of the project overlay (`omp-seams-overlay-keys.txt`, kept equal to `overlay_text`'s keys by
// overlay.rs's tests), and looks each up in `omp config list --json`. A key under a map setting (`modelRoles.<role>`,
// `retry.fallbackChains.<role>`) is checked at the map's level. omp runs with a scratch agent folder, so nothing of
// the user's is read or written. Prints the omp version and the count; exits 1 naming every key omp doesn't know, or
// when omp can't be run.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, relative } from "node:path";
import { removeTree } from "../test/shared/remove.ts";

const ROOT = join(import.meta.dir, "..");
const FILES = [
	"config/omp/config.yml",
	"config/boss.yml",
	"config/guard.yml",
	"config/headless.yml",
	"config/heavy.yml",
	"config/review.yml",
];
const OVERLAY_KEYS = "scripts/omp-seams-overlay-keys.txt";

/** omp's registered settings by dotted key, with each one's `type` (`record` for a map setting). */
export type Registered = Record<string, { type?: string }>;

/** The dotted paths of a parsed settings file's leaves; a list or a scalar is a leaf. */
export function leafKeys(value: unknown, prefix = ""): string[] {
	if (typeof value !== "object" || value === null || Array.isArray(value)) return prefix === "" ? [] : [prefix];
	return Object.entries(value).flatMap(([key, child]) => leafKeys(child, prefix === "" ? key : `${prefix}.${key}`));
}

/** The keys omp doesn't know: neither registered themselves nor under a registered map (`record`) setting. */
export function unknownKeys(keys: readonly string[], registered: Registered): string[] {
	return keys.filter((key) => {
		if (key in registered) return false;
		const parts = key.split(".");
		for (let n = parts.length - 1; n > 0; n--)
			if (registered[parts.slice(0, n).join(".")]?.type === "record") return false;
		return true;
	});
}

/** Runs omp with `args` in a scratch agent folder; exits 1 with omp's own words when it fails. */
function omp(args: string[]): string {
	const exe = Bun.which("omp");
	if (exe === null) {
		console.error("omp-seams: omp isn't on PATH");
		process.exit(1);
	}
	const scratch = mkdtempSync(join(tmpdir(), "jocasta-omp-seams-"));
	try {
		const run = spawnSync(exe, args, {
			encoding: "utf8",
			maxBuffer: 64 * 1024 * 1024,
			// npm's Windows shim is a .cmd, which only a shell runs.
			shell: /\.(cmd|bat)$/i.test(exe),
			// omp reads PI_CONFIG_DIR relative to the home folder (as test/preload.ts sets it).
			env: { ...process.env, PI_CODING_AGENT_DIR: join(scratch, "agent"), PI_CONFIG_DIR: relative(homedir(), scratch) },
		});
		if (run.status !== 0) {
			console.error(`omp-seams: omp ${args.join(" ")} failed (exit ${run.status}): ${run.stderr || run.error}`);
			process.exit(1);
		}
		return run.stdout;
	} finally {
		removeTree(scratch);
	}
}

if (import.meta.main) {
	const version = omp(["--version"]).trim().replace("/", " ");
	const registered = JSON.parse(omp(["config", "list", "--json"])) as Registered;
	const checked = FILES.flatMap((file) =>
		leafKeys(Bun.YAML.parse(readFileSync(join(ROOT, file), "utf8"))).map((key) => ({ file, key })),
	);
	for (const line of readFileSync(join(ROOT, OVERLAY_KEYS), "utf8").split(/\r?\n/)) {
		const key = line.trim();
		if (key !== "" && !key.startsWith("#")) checked.push({ file: "overlay.rs overlay_text", key });
	}
	const keys = checked.map((entry) => entry.key);
	const unknown = new Set(unknownKeys(keys, registered));
	const broken = checked.filter((entry) => unknown.has(entry.key));
	if (broken.length > 0) {
		for (const entry of broken) console.error(`omp-seams: ${entry.file}: ${version} doesn't know ${entry.key}`);
		process.exit(1);
	}
	console.log(`omp-seams: ${version} knows all ${checked.length} keys Jocasta writes`);
}
