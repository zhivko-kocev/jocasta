import type { Database } from "bun:sqlite";
import { type Dirent, existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { nowIso } from "./db.ts";
import { UserError } from "./errors.ts";
import { gitAsync } from "./git.ts";
import { defaultProjectName, identityKey, projectIdFor } from "./identity.ts";
import { log } from "./log.ts";
import { pathKey, realPath } from "./pathkey.ts";
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

/** `project add`'s result: the project, and the linked worktree he named when it was its main checkout that got added. */
export interface AddedProject extends Project {
	mainCheckoutOf?: string;
}

interface PathRow {
	path: string;
	project_id: string;
	role: PathRole;
	git_config_mtime: number | null;
	added_at: string;
}

/** How many paths have their identity resolved at once (each is one or two git calls). */
const GIT_PARALLEL = 4;

/** The home dir and filesystem or drive roots are never projects (0106). */
function isNeverProject(path: string): boolean {
	const real = realPath(path);
	return dirname(real) === real || pathKey(real) === pathKey(realPath(homedir()));
}

/** A checkout's git layout, read without git (0104, 0108, SHR-3). */
interface GitLayout {
	/** The common git dir: `<main>/.git` for a checkout and its linked worktrees. */
	common: string;
	/** A linked worktree: its `.git` is a file whose git dir holds `commondir` (a submodule's has none). */
	linked: boolean;
	/** mtime of the common dir's `config` (0108), or null when it can't be read. */
	mtime: number | null;
}

/** `dir`'s git layout, or undefined when it has no `.git`. */
function gitLayout(dir: string): GitLayout | undefined {
	const dotGit = join(dir, ".git");
	let common = dotGit;
	let linked = false;
	try {
		if (statSync(dotGit).isFile()) {
			const gitdir = /^gitdir:\s*(.+)$/m.exec(readFileSync(dotGit, "utf8"))?.[1]?.trim();
			if (!gitdir) return { common, linked, mtime: null };
			const gitDir = isAbsolute(gitdir) ? gitdir : resolve(dir, gitdir);
			const commondir = join(gitDir, "commondir");
			linked = existsSync(commondir);
			common = linked ? resolve(gitDir, readFileSync(commondir, "utf8").trim()) : gitDir;
		}
	} catch {
		return undefined;
	}
	try {
		return { common, linked, mtime: Math.floor(statSync(join(common, "config")).mtimeMs) };
	} catch {
		return { common, linked, mtime: null };
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
			// Name order on every OS (NTFS lists it; ext4 and APFS don't), so "the first one found" is the same path (0104).
			entries = readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
				a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
			);
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

/** `work` over `items`, at most `limit` at a time, results in order. */
async function mapLimit<T, R>(items: readonly T[], limit: number, work: (item: T) => Promise<R>): Promise<R[]> {
	const results: R[] = new Array(items.length);
	let next = 0;
	const lane = async () => {
		while (next < items.length) {
			const i = next++;
			results[i] = await work(items[i] as T);
		}
	};
	await Promise.all(Array.from({ length: Math.min(limit, items.length) }, lane));
	return results;
}

/** One path's catalog row as the sync decided it, before its write. */
interface PathPlan {
	key: string;
	path: string;
	mtime: number | null;
	identity: string;
	linked: boolean;
}

/**
 * Brings `catalog.db` in line with the projects file plus the scan (0108, 0109): every launch lists the roots, and runs
 * git only for new paths, a changed `.git/config`, or a git checkout still keyed by its path because it was first seen
 * before its first commit (SHR-2), so it becomes `root:<sha>` once it has one (0103). Git runs asynchronously before
 * the write; the write is one short transaction with no git in it (SHR-3).
 */
export async function syncCatalog(db: Database, file: ProjectsFile = readProjectsFile()): Promise<void> {
	const excluded = new Set(file.exclude.map((p) => pathKey(realPath(p))));
	const candidates = new Map<string, string>();
	const consider = (path: string) => {
		const key = pathKey(path);
		if (!excluded.has(key) && !isNeverProject(path)) candidates.set(key, path);
	};
	for (const root of file.roots) for (const path of scanRoot(root.path, root.depth, root.ignore)) consider(path);
	for (const added of file.add) {
		const path = realPath(added);
		if (existsSync(path)) consider(path);
	}

	const known = db
		.query<PathRow & { identity: string }, []>(
			"SELECT project_paths.*, projects.identity FROM project_paths JOIN projects ON projects.id = project_paths.project_id",
		)
		.all();
	const byKey = new Map(known.map((row) => [pathKey(row.path), row]));
	const plans = await mapLimit([...candidates], GIT_PARALLEL, async ([key, path]): Promise<PathPlan | undefined> => {
		const layout = gitLayout(path);
		const mtime = layout?.mtime ?? null;
		const row = byKey.get(key);
		const pathKeyed = row?.identity.startsWith("path:") ?? false;
		if (row && row.git_config_mtime === mtime && !(layout && pathKeyed)) return undefined;
		let identity: string;
		try {
			identity = layout ? await identityKey(path) : `path:${key}`;
		} catch (error) {
			// A git that hangs or can't run leaves this path as it was until the next sync.
			log("warn", "projects", `${path}: ${(error as Error).message}`);
			return undefined;
		}
		if (row && row.git_config_mtime === mtime && row.identity === identity) return undefined;
		return { key, path, mtime, identity, linked: layout?.linked ?? false };
	});
	// Worktree paths that are gone, checked before the write so the transaction does no I/O.
	const missing = new Set(known.filter((row) => !existsSync(row.path)).map((row) => pathKey(row.path)));

	const now = nowIso();
	db.transaction(() => {
		// Read again inside the write: another sync may have written since the plan was made.
		const rows = db.query<PathRow, []>("SELECT * FROM project_paths").all();
		const current = new Map(rows.map((row) => [pathKey(row.path), row]));
		for (const plan of plans) {
			if (!plan) continue;
			const row = current.get(plan.key);
			const projectId = projectIdFor(plan.identity);
			db.query("INSERT OR IGNORE INTO projects (id, identity, created_at) VALUES (?, ?, ?)").run(
				projectId,
				plan.identity,
				now,
			);
			const hasMain = db
				.query("SELECT 1 FROM project_paths WHERE project_id = ? AND role = 'main' AND path != ?")
				.get(projectId, row?.path ?? plan.path);
			const role: PathRole = plan.linked ? "worktree" : hasMain ? "clone" : "main";
			if (row) {
				db.query("UPDATE project_paths SET project_id = ?, role = ?, git_config_mtime = ? WHERE path = ?").run(
					projectId,
					role,
					plan.mtime,
					row.path,
				);
			} else {
				db.query(
					"INSERT INTO project_paths (path, project_id, role, git_config_mtime, added_at) VALUES (?, ?, ?, ?, ?)",
				).run(plan.path, projectId, role, plan.mtime, now);
			}
		}
		// Paths no longer wanted: main and clone paths outside the file and scan; worktrees that are gone.
		for (const row of rows) {
			const key = pathKey(row.path);
			const gone = row.role === "worktree" ? missing.has(key) || excluded.has(key) : !candidates.has(key);
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

/**
 * `jocasta project add <path>` (0111): registers a folder, persisting it under `add` when the scan doesn't find it. A
 * linked worktree adds its main checkout instead (0582), named in `mainCheckoutOf`. The file is written only once the
 * path is known to register.
 */
export async function addProject(db: Database, path: string, file = projectsFile()): Promise<AddedProject> {
	const real = realPath(path);
	if (!existsSync(real) || !statSync(real).isDirectory()) throw new UserError(`${real} is not a folder.`);
	if (isNeverProject(real)) throw new UserError(`${real} can't be a project: home and drive roots never are.`);
	const top = await gitAsync(real, ["rev-parse", "--show-toplevel"], 30_000);
	let target = top.code === 0 && top.stdout.trim() ? realPath(top.stdout.trim()) : real;
	let mainCheckoutOf: string | undefined;
	const layout = isGitDir(target) ? gitLayout(target) : undefined;
	if (layout?.linked) {
		// A worktree's main checkout holds its common dir as `.git`; a bare repository has no main checkout.
		const main = dirname(layout.common);
		if (basename(layout.common) !== ".git" || !isGitDir(main)) {
			throw new UserError(
				`${target} is a worktree of the bare repository ${layout.common}; add a checkout of it instead.`,
			);
		}
		mainCheckoutOf = target;
		target = realPath(main);
	}
	if (isNeverProject(target)) throw new UserError(`${target} can't be a project: home and drive roots never are.`);
	const contents = readProjectsFile(file);
	const unexcluded = { ...contents, exclude: contents.exclude.filter((p) => pathKey(realPath(p)) !== pathKey(target)) };
	let found = await findPath(db, target, unexcluded);
	const listed = !found;
	if (!found) found = await findPath(db, target, { ...unexcluded, add: [...unexcluded.add, target] });
	if (!found) throw new UserError(`${target} can't be a project; ${file} is left as it was.`, 1);
	removeFromList("exclude", target, file);
	if (listed) addToList("add", target, file);
	return mainCheckoutOf ? { ...found, mainCheckoutOf } : found;
}

async function findPath(db: Database, path: string, contents: ProjectsFile): Promise<Project | undefined> {
	await syncCatalog(db, contents);
	return listProjects(db, contents).find((p) => p.paths.some((row) => pathKey(row.path) === pathKey(path)));
}

/** `jocasta project remove <name|id>` (0111): drops it from the catalog and excludes its paths; files are never touched. */
export async function removeProject(db: Database, nameOrId: string, file = projectsFile()): Promise<Project> {
	const contents = readProjectsFile(file);
	await syncCatalog(db, contents);
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
	await syncCatalog(db, readProjectsFile(file));
	return project;
}
