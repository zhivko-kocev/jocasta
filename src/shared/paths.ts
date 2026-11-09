import { homedir } from "node:os";
import { join } from "node:path";
import { privateDir } from "./fsx.ts";

/** A leading `~` as the home folder, for the `JOCASTA_*_DIR` and `PI_CODING_AGENT_DIR` overrides (0098). */
function expandTilde(path: string): string {
	if (path === "~") return homedir();
	if (path.startsWith("~/") || (process.platform === "win32" && path.startsWith("~\\"))) {
		return join(homedir(), path.slice(2));
	}
	return path;
}

/** Folders made private in this process: each is made (or tightened) once. */
const made = new Set<string>();

/**
 * `dir`, made 0700 on Linux and macOS the first time this process names it (0484); on Windows it is only named. A
 * folder that can't be made is left to the caller that writes in it, which says why.
 */
function privateOnPosix(dir: string): string {
	if (process.platform === "win32" || made.has(dir)) return dir;
	try {
		privateDir(dir);
		made.add(dir);
	} catch {
		// Said by the caller's own write.
	}
	return dir;
}

/** Jocasta's config dir (0054, 0387); `JOCASTA_CODING_AGENT_DIR` overrides it (0098). 0700 on Linux and macOS (0484). */
export function configDir(): string {
	const env = process.env.JOCASTA_CODING_AGENT_DIR;
	return privateOnPosix(env ? expandTilde(env) : join(homedir(), ".jocasta", "agent"));
}

/** The OS data dir (0095); `JOCASTA_DATA_DIR` overrides it (0098). 0700 on Linux and macOS (0484). */
export function dataDir(): string {
	const env = process.env.JOCASTA_DATA_DIR;
	return privateOnPosix(env ? expandTilde(env) : defaultDataDir());
}

/**
 * The data dir without `JOCASTA_DATA_DIR`: `%LOCALAPPDATA%\jocasta\data` on Windows, `~/Library/Application Support`
 * on macOS, `$XDG_DATA_HOME` (or `~/.local/share`) elsewhere (0095). An empty variable counts as unset.
 */
export function defaultDataDir(): string {
	if (process.platform === "win32") {
		return join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "jocasta", "data");
	}
	if (process.platform === "darwin") return join(homedir(), "Library", "Application Support", "jocasta", "data");
	return join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "jocasta", "data");
}

/**
 * omp's agent dir (0387), as omp resolves it (`@oh-my-pi/pi-utils` `dirs.ts` `getAgentDir`): a named `profile`'s is
 * `<root>/profiles/<name>/agent` whatever `PI_CODING_AGENT_DIR` says; else `PI_CODING_AGENT_DIR` (tilde-expanded), else
 * `<root>/agent`; `<root>` is `~/.omp`, or `~/<PI_CONFIG_DIR>`. Setup writes there; the service reads its `sessions/`.
 */
export function ompAgentDir(profile?: string): string {
	const root = join(homedir(), process.env.PI_CONFIG_DIR || ".omp");
	if (profile !== undefined) return join(root, "profiles", profile, "agent");
	const env = process.env.PI_CODING_AGENT_DIR;
	return env ? expandTilde(env) : join(root, "agent");
}

/** `~/.jocasta/agent/projects.yaml` (0109). */
export function projectsFile(): string {
	return join(configDir(), "projects.yaml");
}

/** `<data>/catalog.db` (0096). */
export function catalogPath(): string {
	return join(dataDir(), "catalog.db");
}

/** `<data>/projects/<pid>` (0096). */
export function projectDir(projectId: string): string {
	return join(dataDir(), "projects", projectId);
}

/**
 * `<data>/projects/<pid>/lessons` (0459): the project's lessons, outside client repos. Its `SKILL.md` is the index omp
 * lists as the skill `lessons`; each lesson is its own file beside it.
 */
export function lessonsDir(projectId: string): string {
	return join(projectDir(projectId), "lessons");
}
