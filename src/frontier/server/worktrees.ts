/**
 * Thread worktrees on disk (0414, 0445, 0455, 0457): where a new one goes, its own variables and setup files, its
 * number, the last commit a review covered, removing one, and the leftovers no open thread holds (0456).
 */
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { readJsonObject, removeTreeAsync, writeAtomically } from "../../shared/fsx.ts";
import { git, gitAsync } from "../../shared/git.ts";
import { pathKey, realPath, sameDir } from "../../shared/pathkey.ts";
import { dataDir } from "../../shared/paths.ts";
import { processesIn } from "../../shared/proctree.ts";
import type { Project } from "../../shared/projects.ts";
import type { Leftover } from "../protocol.ts";
import { fallbackSlug } from "./wording.ts";

/** One git call the service runs off the event loop (CO-10, PERF-1). */
export const GIT_MS = 30_000;
/** One `git worktree list` for the leftovers (0456). */
const WORKTREE_LIST_MS = 30_000;
/** How long removing a closed thread's checkout keeps trying while its panes' processes exit (F-02). */
const REMOVE_WAIT_MS = 20_000;
/** One `git worktree remove` (CO-10). */
const WORKTREE_REMOVE_MS = 60_000;
/** How often removing a checkout tries again while something holds it. */
const REMOVE_RETRY_MS = 500;
/** At most this many processes are named when a folder is still in use. */
const HOLDERS_NAMED = 5;

/** git's trimmed stdout in `dir` without holding the event loop (CO-10), or undefined when it failed or ran too long. */
export async function gitOut(dir: string, args: string[]): Promise<string | undefined> {
	const run = await gitAsync(dir, args, GIT_MS);
	return run.code === 0 ? run.stdout.trim() : undefined;
}

/**
 * A new thread's branch and worktree folder (0414, 0445, 0455): `feature/<slug>` in `<parent>/<project folder>.wt/<slug>`;
 * when either is taken, the first `-2`, `-3`… free for both, so the branch matches its folder and a new thread never
 * picks up an old thread's branch. A title with no slug (another script, 0581) takes `thread-<YYYYMMDD>-<n>`, the first
 * `n` free.
 */
export function threadPlace(mainPath: string, slug: string): { branch: string; path: string } {
	const root = join(dirname(mainPath), `${basename(mainPath)}.wt`);
	const today = new Date();
	for (let n = 1; ; n++) {
		const name = !slug ? fallbackSlug(today, n) : n === 1 ? slug : `${slug}-${n}`;
		const place = { branch: `feature/${name}`, path: join(root, name) };
		if (!existsSync(place.path) && !git(mainPath, ["rev-parse", "--verify", "-q", `refs/heads/${place.branch}`])) {
			return place;
		}
	}
}

/**
 * A thread pane's own variables (0457): `JOCASTA_THREAD` (its title slug), `COMPOSE_PROJECT_NAME` (`<project
 * folder>-<slug>`, Compose's allowed characters only) and `JOCASTA_PORT_OFFSET` (10 × the thread's number, the lowest
 * free positive integer among the project's open threads), for a project to read so parallel threads don't share
 * ports or Compose projects.
 */
export function threadEnv(projectFolder: string, slug: string, number: number): Record<string, string> {
	const compose = `${projectFolder}-${slug}`
		.toLowerCase()
		.replace(/[^a-z0-9_-]+/g, "-")
		.replace(/^[^a-z0-9]+/, "");
	return { JOCASTA_THREAD: slug, COMPOSE_PROJECT_NAME: compose, JOCASTA_PORT_OFFSET: String(10 * number) };
}

/**
 * Copies a new worktree's setup files from the main checkout (0457): the files `.worktreeinclude` (Claude Code's
 * format, gitignore patterns) matches that are untracked and gitignored, such as `.env`; a file the worktree has
 * already stays. Returns the paths copied, relative to the checkout.
 */
export async function copyWorktreeInclude(main: string, worktree: string): Promise<string[]> {
	const include = join(main, ".worktreeinclude");
	if (!existsSync(include)) return [];
	const matched = ((await gitOut(main, ["ls-files", "--others", "--ignored", `--exclude-from=${include}`, "-z"])) ?? "")
		.split("\0")
		.filter(Boolean);
	if (matched.length === 0) return [];
	const ignored = await gitAsync(main, ["check-ignore", "--stdin", "-z"], GIT_MS, { stdin: `${matched.join("\0")}\0` });
	const copied: string[] = [];
	for (const file of ignored.stdout.split("\0").filter(Boolean)) {
		const to = join(worktree, file);
		if (existsSync(to)) continue;
		mkdirSync(dirname(to), { recursive: true });
		copyFileSync(join(main, file), to);
		copied.push(file);
	}
	return copied;
}

/**
 * `<data>/thread-numbers.json`: each open thread worktree's number (0457), so a respawned pane keeps its offset: the
 * lowest positive integer no other open thread of the project holds, given when first asked.
 */
export class ThreadNumbers {
	constructor(private readonly file = join(dataDir(), "thread-numbers.json")) {}

	/** The number of the thread in worktree `path`; `open` are the worktrees of the project's open threads. */
	of(path: string, open: string[]): number {
		const key = pathKey(path);
		const numbers = (readJsonObject(this.file) ?? {}) as Record<string, number>;
		const known = numbers[key];
		if (typeof known === "number") return known;
		const taken = new Set(
			open.map((other) => numbers[pathKey(other)]).filter((n): n is number => typeof n === "number"),
		);
		let number = 1;
		while (taken.has(number)) number++;
		numbers[key] = number;
		writeAtomically(this.file, `${JSON.stringify(numbers, null, 2)}\n`);
		return number;
	}

	/** Forgets a removed thread's number, so the next thread can take it. */
	forget(path: string): void {
		const numbers = readJsonObject(this.file);
		if (!numbers) return;
		delete numbers[pathKey(path)];
		writeAtomically(this.file, `${JSON.stringify(numbers, null, 2)}\n`);
	}
}

/** `<data>/reviewed.json`: the last reviewed commit per thread worktree (0394), so a restart knows what's unreviewed. */
export function reviewedFile(): string {
	return join(dataDir(), "reviewed.json");
}

/** The last reviewed commit per thread worktree, in `reviewedFile()`. */
export class ReviewedCommits {
	constructor(private readonly file = reviewedFile()) {}

	/** The last reviewed commit of the worktree at `path`, if any. */
	of(path: string): string | undefined {
		const commit = readJsonObject(this.file)?.[pathKey(realPath(path))];
		return typeof commit === "string" ? commit : undefined;
	}

	/** Records `commit` as the worktree's last reviewed one, or forgets the worktree (undefined). */
	mark(path: string, commit: string | undefined): void {
		const all = readJsonObject(this.file) ?? {};
		const key = pathKey(realPath(path));
		if (commit) all[key] = commit;
		else delete all[key];
		writeAtomically(this.file, `${JSON.stringify(all, null, 2)}\n`);
	}
}

/**
 * The worktrees `git worktree list --porcelain -z` names in the repo at `main` (SEC-LOC-6: NUL-separated, so a path is
 * never C-quoted), each with its branch; undefined when git failed, which is never "not listed".
 */
async function worktreeList(main: string): Promise<{ path: string; branch: string | null }[] | undefined> {
	const run = await gitAsync(main, ["worktree", "list", "--porcelain", "-z"], WORKTREE_LIST_MS);
	if (run.code !== 0) return undefined;
	const found: { path: string; branch: string | null }[] = [];
	let current: { path: string; branch: string | null } | undefined;
	for (const field of run.stdout.split("\0")) {
		if (field.startsWith("worktree ")) {
			current = { path: field.slice(9), branch: null };
			found.push(current);
		} else if (field.startsWith("branch refs/heads/") && current) current.branch = field.slice(18);
	}
	return found;
}

/** Why a folder can't go yet: the processes still working in it (0486), or undefined when none does. */
function inUse(path: string): string | undefined {
	const holders = processesIn(path);
	if (holders.length === 0) return undefined;
	const named = holders.slice(0, HOLDERS_NAMED).map((p) => `${p.name} (pid ${p.pid})`);
	if (holders.length > HOLDERS_NAMED) named.push(`${holders.length - HOLDERS_NAMED} more`);
	return `processes still work in it: ${named.join(", ")}`;
}

/**
 * Removes the linked worktree at `path` of the repo at `main` (`git worktree remove`, no force: a dirty checkout is
 * refused) once no process has the folder or one inside it as its working folder (0486), trying again every half
 * second for up to `waitMs` while a process works in it or something else holds it. A folder git positively doesn't
 * list, and that has no `.git` file of its own any more, is one git already let go of: it is deleted and pruned. git
 * failing to list its worktrees keeps the folder (SEC-LOC-6): it may hold uncommitted work. Returns undefined once the
 * folder and git's record are gone, else why not.
 */
export async function removeCheckout(main: string, path: string, waitMs = REMOVE_WAIT_MS): Promise<string | undefined> {
	const end = Date.now() + waitMs;
	let reason = "";
	for (;;) {
		try {
			const worktrees = await worktreeList(main);
			if (!worktrees) throw new Error(`git couldn't list ${basename(main)}'s worktrees, so the folder stays`);
			const listed = worktrees.some((w) => existsSync(w.path) && sameDir(w.path, path));
			const holders = existsSync(path) ? inUse(path) : undefined;
			if (holders) throw new Error(holders);
			if (listed) {
				const removed = await gitAsync(main, ["worktree", "remove", path], WORKTREE_REMOVE_MS);
				if (removed.code === null) throw new Error("git worktree remove didn't finish in a minute");
				if (removed.code !== 0)
					throw new Error(removed.stderr.trim() || `git worktree remove failed (exit ${removed.code})`);
			} else if (existsSync(join(path, ".git"))) {
				throw new Error("it still has its .git file, though git doesn't list it; it stays");
			} else if (existsSync(path)) await removeTreeAsync(path);
			if (!existsSync(path)) {
				await gitAsync(main, ["worktree", "prune"], GIT_MS);
				return undefined;
			}
		} catch (error) {
			reason = (error as Error).message.split("\n")[0] ?? "";
			// A dirty checkout won't become clean by waiting, nor git's own refusal to list.
			if (/modified or untracked|contains modified|couldn't list|still has its \.git/.test(reason)) return reason;
		}
		if (Date.now() >= end) return reason || "something still holds it";
		await Bun.sleep(REMOVE_RETRY_MS);
	}
}

/**
 * Leftover worktrees (0456): checkouts in a project's `<project>.wt/` folder that no open thread holds (closed in
 * herdr, still on disk), with their branch, commits ahead of trunk and changed files, read off the event loop; trunk
 * only for a project that has one (PERF-1). `open` are the open threads' worktrees. Never removed here.
 */
export async function findLeftovers(
	projects: Project[],
	open: string[],
	trunkOf: (project: Project) => Promise<string | undefined>,
): Promise<Leftover[]> {
	const held = new Set(open.map((path) => pathKey(realPath(path))));
	const found: Leftover[] = [];
	for (const project of projects) {
		if (!project.git) continue;
		const root = pathKey(join(dirname(project.mainPath), `${basename(project.mainPath)}.wt`));
		const worktrees = (await worktreeList(project.mainPath)) ?? [];
		const left = worktrees.filter(
			(w) => existsSync(w.path) && !held.has(pathKey(realPath(w.path))) && pathKey(dirname(w.path)).startsWith(root),
		);
		if (left.length === 0) continue;
		const trunk = await trunkOf(project);
		const read = await Promise.all(
			left.map(async (w) => {
				const [ahead, status] = await Promise.all([
					trunk ? gitOut(w.path, ["rev-list", "--count", `${trunk}..HEAD`]) : undefined,
					gitOut(w.path, ["status", "--porcelain"]),
				]);
				const dirty = (status ?? "").split("\n").filter(Boolean).length;
				return { project: project.name, folder: resolve(w.path), branch: w.branch, ahead: Number(ahead ?? 0), dirty };
			}),
		);
		found.push(...read);
	}
	return found;
}
