import type { Database } from "bun:sqlite";
import { type Dirent, existsSync, readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { nowIso } from "./db.ts";
import { UserError } from "./errors.ts";
import { git } from "./git.ts";
import { defaultProjectName, identityKey, normalizePathKey, projectIdFor } from "./identity.ts";
import { projectsFile } from "./paths.ts";
import { addToList, type ProjectsFile, readProjectsFile, removeFromList } from "./projects-file.ts";

type PathRole = "main" | "worktree" | "clone";

export interface Project {
	id: string;
	identity: string;
	name: string;
	mainPath: string;
	git: boolean;
	paths: { path: string; role: PathRole }[];
}

interface PathRow {
	path: string;
	project_id: string;
	role: PathRole;
	git_config_mtime: number | null;
	added_at: string;
}

/** The real, absolute form of a path (long names on Windows), or its resolved form when it doesn't exist. */
export function realPath(path: string): string {
	try {
		return realpathSync.native(resolve(path));
	} catch {
		return resolve(path);
	}
}

const keyOf = (path: string) => normalizePathKey(path);

/** The home dir and filesystem or drive roots are never projects (0106). */
function isNeverProject(path: string): boolean {
	const real = realPath(path);
	return dirname(real) === real || keyOf(real) === keyOf(realPath(homedir()));
}

/** mtime of the repo's `config` (in the common dir for a linked worktree), read without git (0108). */
function gitConfigMtime(dir: string): number | null {
	const dotGit = join(dir, ".git");
	try {
		let common = dotGit;
		if (statSync(dotGit).isFile()) {
			const gitdir = /^gitdir:\s*(.+)$/m.exec(readFileSync(dotGit, "utf8"))?.[1]?.trim();
			if (!gitdir) return null;
			const gitDir = isAbsolute(gitdir) ? gitdir : resolve(dir, gitdir);
			const commondir = join(gitDir, "commondir");
			common = existsSync(commondir) ? resolve(gitDir, readFileSync(commondir, "utf8").trim()) : gitDir;
		}
		return Math.floor(statSync(join(common, "config")).mtimeMs);
	} catch {
		return null;
	}
}

const isGitDir = (dir: string) => existsSync(join(dir, ".git"));

/** Git top-levels under a root, down to `depth` levels, skipping `ignore` globs and never-projects (0105, 0106). */
function scanRoot(rootPath: string, depth: number, ignore: string[]): string[] {
	const globs = ignore.map((pattern) => new Bun.Glob(pattern));
	const found: string[] = [];
	const walk = (dir: string, level: number) => {
		let entries: Dirent[];
		try {
			entries = readdirSync(dir, { withFileTypes: true });
		} catch {
			return; // unreadable folders (`System Volume Information`) are not projects
		}
		for (const entry of entries) {
			if (!entry.isDirectory() || globs.some((g) => g.match(entry.name))) continue;
			const child = join(dir, entry.name);
			if (isGitDir(child)) {
				if (!isNeverProject(child)) found.push(realPath(child));
			} else if (level < depth) {
				walk(child, level + 1);
			}
		}
	};
	walk(realPath(rootPath), 1);
	return found;
}

/** Whether `dir` is a linked worktree (its git dir differs from the common dir, 0104). */
function isLinkedWorktree(dir: string): boolean {
	const out = git(dir, ["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"]);
	const [gitDir, common] = out?.split(/\r?\n/) ?? [];
	return gitDir !== undefined && common !== undefined && keyOf(realPath(gitDir)) !== keyOf(realPath(common));
}

/**
 * Brings `catalog.db` in line with the projects file plus the scan (0108, 0109): every launch lists the roots,
 * and runs git only for new paths or a changed `.git/config`.
 */
export function syncCatalog(db: Database, file: ProjectsFile = readProjectsFile()): void {
	const excluded = new Set(file.exclude.map((p) => keyOf(realPath(p))));
	const candidates = new Map<string, string>();
	const consider = (path: string) => {
		const key = keyOf(path);
		if (!excluded.has(key) && !isNeverProject(path)) candidates.set(key, path);
	};
	for (const root of file.roots) for (const path of scanRoot(root.path, root.depth, root.ignore)) consider(path);
	for (const added of file.add) {
		const path = realPath(added);
		if (existsSync(path)) consider(path);
	}

	const rows = db.query<PathRow, []>("SELECT * FROM project_paths").all();
	const byKey = new Map(rows.map((row) => [keyOf(row.path), row]));
	const now = nowIso();
	db.transaction(() => {
		for (const [key, path] of candidates) {
			const git = isGitDir(path);
			const mtime = git ? gitConfigMtime(path) : null;
			const row = byKey.get(key);
			if (row && row.git_config_mtime === mtime) continue;
			const identity = git ? identityKey(path) : `path:${keyOf(path)}`;
			const projectId = projectIdFor(identity);
			db.query("INSERT OR IGNORE INTO projects (id, identity, created_at) VALUES (?, ?, ?)").run(
				projectId,
				identity,
				now,
			);
			const hasMain = db
				.query("SELECT 1 FROM project_paths WHERE project_id = ? AND role = 'main' AND path != ?")
				.get(projectId, row?.path ?? path);
			const role: PathRole = git && isLinkedWorktree(path) ? "worktree" : hasMain ? "clone" : "main";
			if (row) {
				db.query("UPDATE project_paths SET project_id = ?, role = ?, git_config_mtime = ? WHERE path = ?").run(
					projectId,
					role,
					mtime,
					row.path,
				);
			} else {
				db.query(
					"INSERT INTO project_paths (path, project_id, role, git_config_mtime, added_at) VALUES (?, ?, ?, ?, ?)",
				).run(path, projectId, role, mtime, now);
			}
		}
		// Paths no longer wanted: main and clone paths outside the file and scan; worktrees that are gone.
		for (const row of rows) {
			const key = keyOf(row.path);
			const gone = row.role === "worktree" ? !existsSync(row.path) || excluded.has(key) : !candidates.has(key);
			if (gone) db.query("DELETE FROM project_paths WHERE path = ?").run(row.path);
		}
		// A project without a main path promotes its oldest clone; one with neither is dropped.
		for (const { id } of db.query<{ id: string }, []>("SELECT id FROM projects").all()) {
			const roots = db
				.query<PathRow, [string]>(
					"SELECT * FROM project_paths WHERE project_id = ? AND role != 'worktree' ORDER BY role = 'main' DESC, added_at",
				)
				.all(id);
			const first = roots[0];
			if (!first) db.query("DELETE FROM projects WHERE id = ?").run(id);
			else if (first.role !== "main") db.query("UPDATE project_paths SET role = 'main' WHERE path = ?").run(first.path);
		}
	})();
}

/** Every project in the catalog, by display name (0110, 0112). */
export function listProjects(db: Database, file: ProjectsFile = readProjectsFile()): Project[] {
	const projects = db.query<{ id: string; identity: string }, []>("SELECT id, identity FROM projects").all();
	const paths = db.query<PathRow, []>("SELECT * FROM project_paths ORDER BY added_at").all();
	return projects
		.map((p): Project => {
			const own = paths.filter((row) => row.project_id === p.id);
			const mainPath = own.find((row) => row.role === "main")?.path ?? own[0]?.path ?? "";
			return {
				id: p.id,
				identity: p.identity,
				name: file.names[p.id] ?? defaultProjectName(p.identity, mainPath),
				mainPath,
				git: !p.identity.startsWith("path:"),
				paths: own.map((row) => ({ path: row.path, role: row.role })),
			};
		})
		.sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

/** `jocasta project add <path>` (0111): registers a folder, persisting it under `add` when the scan doesn't find it. */
export function addProject(db: Database, path: string, file = projectsFile()): Project {
	const real = realPath(path);
	if (!existsSync(real) || !statSync(real).isDirectory()) throw new UserError(`${real} is not a folder.`);
	if (isNeverProject(real)) throw new UserError(`${real} can't be a project: home and drive roots never are.`);
	const top = git(real, ["rev-parse", "--show-toplevel"]);
	const target = top ? realPath(top) : real;
	if (isNeverProject(target)) throw new UserError(`${target} can't be a project: home and drive roots never are.`);
	removeFromList("exclude", target, file);
	let found = findPath(db, target, file);
	if (!found) {
		addToList("add", target, file);
		found = findPath(db, target, file);
	}
	if (!found) throw new Error(`${target} was added to ${file} but not registered`);
	return found;
}

function findPath(db: Database, path: string, file: string): Project | undefined {
	const contents = readProjectsFile(file);
	syncCatalog(db, contents);
	return listProjects(db, contents).find((p) => p.paths.some((row) => keyOf(row.path) === keyOf(path)));
}

/** `jocasta project remove <name|id>` (0111): drops it from the catalog and excludes its paths; files are never touched. */
export function removeProject(db: Database, nameOrId: string, file = projectsFile()): Project {
	const contents = readProjectsFile(file);
	syncCatalog(db, contents);
	const projects = listProjects(db, contents);
	const byId = projects.find((p) => p.id === nameOrId);
	const byName = projects.filter((p) => p.name.toLowerCase() === nameOrId.toLowerCase());
	if (!byId && byName.length > 1) {
		throw new UserError(
			`${byName.length} projects are named ${nameOrId}: ${byName.map((p) => p.id).join(", ")}. Use the id.`,
		);
	}
	const project = byId ?? byName[0];
	if (!project) throw new UserError(`no project named ${nameOrId}. See jocasta project list.`);
	for (const p of project.paths) {
		if (p.role === "worktree") continue;
		removeFromList("add", p.path, file);
		addToList("exclude", p.path, file);
	}
	syncCatalog(db, readProjectsFile(file));
	return project;
}
