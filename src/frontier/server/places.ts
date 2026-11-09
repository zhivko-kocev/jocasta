/**
 * Where folders sit (SVC-15): git places and the watched project holding a folder, each project's trunk, and the
 * project list kept through a malformed `projects.yaml` (CO-1). Kept 10 minutes, then looked up again; a catalog sync
 * forgets them at once.
 */
import type { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { UserError } from "../../shared/errors.ts";
import { git } from "../../shared/git.ts";
import { pathKey, realPath } from "../../shared/pathkey.ts";
import { listProjects, type Project, syncCatalog } from "../../shared/projects.ts";
import { gitOut } from "./worktrees.ts";

/** How long git places and projects found for a folder are kept before being looked up again. */
const PLACES_KEPT_MS = 10 * 60_000;

/** A checkout's place in git: its top-level, its main checkout and branch; undefined outside git. */
export interface Checkout {
	top: string;
	main: string;
	branch: string | undefined;
}

export class Places {
	private readonly checkouts = new Map<string, Checkout | null>();
	private readonly projectsByDir = new Map<string, Project | null>();
	/** Each watched project by its catalog paths' keys, for `projectAt`. */
	private projectPaths: Map<string, Project> | undefined;
	/** Each project's trunk, kept with the places (10 minutes, PERF-1). */
	private readonly trunks = new Map<string, string | null>();
	/** The last good project list, kept while `projects.yaml` is malformed (CO-1), and the problem said about it. */
	private lastProjects: Project[] = [];
	private projectsProblem: string | undefined;
	/** Worktrees of threads removed here, by path key, so their summaries still find their project (CO-2). */
	private readonly removedWorktrees = new Map<string, Project>();
	/** When the places were last emptied: they are kept 10 minutes. */
	private placesAt = Date.now();

	constructor(
		private readonly catalog: Database,
		private readonly hooks: {
			/** A problem with `projects.yaml`, said once until it changes. */
			problem(text: string): void;
			/** A project gone from the catalog at a sync. */
			gone(projectId: string): void;
			/** A sync ran: the panes are looked at again for threads (CR-12). */
			synced(): void;
		},
	) {}

	/**
	 * Where a directory sits in git, cached (until `forget`): the worktree's top-level, its main checkout and branch.
	 * One git call; a repo without a commit yet (no HEAD to name) takes two more.
	 */
	checkout(dir: string): Checkout | undefined {
		const key = pathKey(dir);
		let found = this.checkouts.get(key);
		if (found === undefined) {
			found = null;
			if (existsSync(dir)) {
				const places = ["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"];
				const out = git(dir, [...places, "--abbrev-ref", "HEAD"]);
				let [top, common, branch]: (string | undefined)[] = out?.split("\n") ?? [];
				if (out === undefined) {
					[top, common] = git(dir, places)?.split("\n") ?? [];
					branch = top ? git(dir, ["symbolic-ref", "--short", "-q", "HEAD"]) : undefined;
				}
				if (top && common) {
					const main = basename(common) === ".git" ? dirname(common) : top;
					found = { top, main, branch: branch && branch !== "HEAD" ? branch : undefined };
				}
			}
			this.checkouts.set(key, found);
		}
		return found ?? undefined;
	}

	/**
	 * The watched project holding `dir` (cached until `forget`): the project whose catalog path is its git top-level
	 * or, for a herdr worktree, its main checkout; outside git, the nearest registered non-git ancestor (0105). Only
	 * `checkout`'s one git call. A folder gone with a thread `thread_finish` removed still names that thread's project
	 * (CO-2), so its summary reaches the graph.
	 */
	projectAt(dir: string): Project | undefined {
		const key = pathKey(dir);
		let project = this.projectsByDir.get(key);
		if (!project && this.removedWorktrees.size > 0 && !existsSync(dir)) {
			for (let current = resolve(dir); ; current = dirname(current)) {
				const removed = this.removedWorktrees.get(pathKey(current));
				if (removed) {
					this.projectsByDir.set(key, removed);
					return removed;
				}
				if (dirname(current) === current) break;
			}
		}
		if (project === undefined) {
			this.projectPaths ??= new Map(
				this.listed().flatMap((p) => p.paths.map((path) => [pathKey(realPath(path.path)), p])),
			);
			const paths = this.projectPaths;
			const at = (path: string) => paths.get(pathKey(realPath(path)));
			const place = this.checkout(dir);
			if (place) project = at(place.top) ?? at(place.main) ?? null;
			else {
				project = null;
				for (let current = realPath(dir); ; current = dirname(current)) {
					const hit = paths.get(pathKey(current));
					if (hit && !hit.git) project = hit;
					if (project || dirname(current) === current) break;
				}
			}
			this.projectsByDir.set(key, project);
		}
		return project ?? undefined;
	}

	/** Records a removed thread's worktree, by both spellings of its folder, as its project's (CO-2). */
	removed(path: string, project: Project): void {
		this.removedWorktrees.set(pathKey(path), project);
		this.removedWorktrees.set(pathKey(realPath(path)), project);
	}

	/**
	 * The watched projects. A malformed `projects.yaml` (half-written, or mid-edit) keeps the last good list and is
	 * said once until it changes (CO-1): a timer that threw here would end the service.
	 */
	listed(): Project[] {
		try {
			this.lastProjects = listProjects(this.catalog);
			this.projectsProblem = undefined;
		} catch (error) {
			this.said(`projects.yaml: ${(error as Error).message}; showing the last good list.`);
		}
		return this.lastProjects;
	}

	private said(problem: string): void {
		if (problem === this.projectsProblem) return;
		this.projectsProblem = problem;
		this.hooks.problem(problem);
	}

	/** A project by id or name (any case); an unknown one is a refusal. */
	find(nameOrId: string): Project {
		const wanted = nameOrId.trim().toLowerCase();
		const project = listProjects(this.catalog).find((p) => p.id === nameOrId || p.name.toLowerCase() === wanted);
		if (!project) throw new UserError(`no project ${nameOrId}.`);
		return project;
	}

	/** Forgets cached git places, trunks and projects: the watched roots or a checkout's branch may have changed. */
	forget(): void {
		this.checkouts.clear();
		this.projectsByDir.clear();
		this.trunks.clear();
		this.projectPaths = undefined;
		this.placesAt = Date.now();
	}

	/** Forgets the places once they are older than they are kept, as of `now`. */
	expire(now: number): void {
		if (now - this.placesAt > PLACES_KEPT_MS) this.forget();
	}

	/**
	 * Projects from the watched roots into the catalog (0105), and the cached places with them. A sync that fails (a
	 * malformed `projects.yaml`, C8) is logged and said once; the catalog goes on as it was. A project gone from the
	 * catalog is told to `hooks.gone` (its code index server closes, SRV-B-20).
	 */
	async sync(): Promise<void> {
		const before = this.listed().map((p) => p.id);
		try {
			await syncCatalog(this.catalog);
		} catch (error) {
			this.said(`projects.yaml: ${(error as Error).message}; showing the last good list.`);
		}
		this.forget();
		this.hooks.synced();
		const now = new Set(this.listed().map((p) => p.id));
		for (const id of before) if (!now.has(id)) this.hooks.gone(id);
	}

	/**
	 * The trunk threads branch from and land on: the remote's default branch (`origin/HEAD`), else `main`, `master` or
	 * `trunk`, else the main checkout's branch; undefined when none of them exists. Asked of git off the event loop and
	 * kept with the places, 10 minutes (PERF-1).
	 */
	async trunk(project: Project): Promise<string | undefined> {
		const known = this.trunks.get(project.id);
		if (known !== undefined) return known ?? undefined;
		const main = project.mainPath;
		let found: string | undefined;
		const remote = await gitOut(main, ["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"]);
		if (remote) found = remote.replace(/^origin\//, "");
		for (const name of ["main", "master", "trunk"]) {
			if (found) break;
			if (await gitOut(main, ["rev-parse", "--verify", "-q", `refs/heads/${name}`])) found = name;
		}
		found ||= await gitOut(main, ["symbolic-ref", "--short", "-q", "HEAD"]);
		this.trunks.set(project.id, found || null);
		return found || undefined;
	}
}
