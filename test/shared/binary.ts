import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

/** This repo's root. */
export const REPO_ROOT = resolve(import.meta.dir, "..", "..");

const built = new Map<string, string>();

/**
 * The debug build of the package's binary `name` (`jocasta` or `jocasta-bench`), built once per test process with
 * `cargo build --locked --bin <name>` (cargo's own lock makes parallel test files wait for each other). It is found in
 * `$CARGO_TARGET_DIR/debug`, else the repo's `target/debug`; the build fails the test when it fails.
 */
export function builtBinary(name: string): string {
	const known = built.get(name);
	if (known !== undefined) return known;
	const build = spawnSync("cargo", ["build", "--locked", "--bin", name], {
		cwd: REPO_ROOT,
		encoding: "utf8",
		windowsHide: true,
	});
	if (build.status !== 0) throw new Error(`cargo build --bin ${name} failed: ${build.error ?? build.stderr}`);
	const targetDir = process.env.CARGO_TARGET_DIR ?? join(REPO_ROOT, "target");
	const exe = join(resolve(REPO_ROOT, targetDir), "debug", process.platform === "win32" ? `${name}.exe` : name);
	if (!existsSync(exe)) throw new Error(`cargo built ${name}, but ${exe} is missing`);
	built.set(name, exe);
	return exe;
}
