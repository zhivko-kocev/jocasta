/**
 * The install layout (appendix I): `<install dir>/jocasta.exe`, `config/`, `node/`, `whiteboard/`, `whiteboard-canvas/`,
 * `editor.worker.js`, `hunk/`, and `install.json` written by the installer. One resolver for every caller: bare
 * `jocasta`, the service it starts, setup and a source run.
 */
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

/** Compiled into `jocasta.exe`, modules live in Bun's embedded file system. */
export const COMPILED = /\$bunfs|~BUN|%7EBUN/.test(import.meta.url);

/** The repo root in a source run (this file is `src/shared/install.ts`). */
export const REPO = resolve(import.meta.dir, "..", "..");

const JOCASTA_EXE = process.platform === "win32" ? "jocasta.exe" : "jocasta";

/** The folder holding `jocasta.exe`: the running executable's own; undefined in a source run. */
export function installDir(execPath = process.execPath): string | undefined {
	const exeDir = dirname(execPath);
	return existsSync(join(exeDir, JOCASTA_EXE)) ? exeDir : undefined;
}

/** The shipped `config/` (boss manual, boss.yml, templates): the install's, or the repo's in a source run. */
export function shippedConfigDir(execPath = process.execPath): string {
	const dir = installDir(execPath);
	return dir ? join(dir, "config") : join(REPO, "config");
}

/** How another program starts this Jocasta: the install's `jocasta.exe`, or `[bun, <repo>/src/entry.ts]` in a source run. */
export function selfCommand(execPath = process.execPath): string[] {
	const dir = installDir(execPath);
	return dir ? [join(dir, JOCASTA_EXE)] : [execPath, join(REPO, "src", "entry.ts")];
}

/** The installer's marker in the install dir: the folder it installed into. */
export const INSTALL_MARKER = "install.json";
