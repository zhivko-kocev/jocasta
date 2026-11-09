/**
 * `thread_finish` (0426, 0454, 0544): a thread's branch squash-merged into trunk, or pushed and opened as a PR by the
 * thread, then its worktree, workspace and branch removed; every refusal leaves everything in place.
 */
import { UserError } from "../../shared/errors.ts";
import { gitAsync } from "../../shared/git.ts";
import { runProgram } from "../../shared/process.ts";
import type { ServiceContext } from "./context.ts";
import { closeTask } from "./task-file.ts";
import { setPhase, subject, type Thread } from "./thread.ts";
import { asker, type Door, overlapSaid, person, prRequest } from "./wording.ts";
import { GIT_MS, gitOut, removeCheckout } from "./worktrees.ts";

/** `thread_finish` waits this long for a `/handoff` that hasn't written its entry yet (CO-2). */
const HANDOFF_HOLD_MS = 5 * 60_000;
/** `git merge --squash` in `thread_finish` (CO-10). */
const MERGE_TIMEOUT_MS = 60_000;
/** The squash commit, with the repo's hooks and signing (CO-10). */
const COMMIT_TIMEOUT_MS = 600_000;
/** How long one gh call may take before it is ended (CO-8). */
export const GH_TIMEOUT_MS = 60_000;

/** What Finish says about a thread before it asks (`finishable`). */
interface Finishable {
	name: string;
	trunk: string;
	ahead: number;
	failed: string | undefined;
	overlaps: { title: string; files: string[] }[];
}

/** One way to finish (0426): what must hold first, what his Allow asks, what stays when it doesn't happen, and doing it. */
interface FinishMode {
	nothing: string;
	/** Refuses when what it needs besides the thread isn't ready; asked before his Allow and again after it. */
	ready?(finisher: Finisher, thread: Thread, trunk: string): Promise<void>;
	question(f: Finishable): string;
	/** What the Allow window says it does; `who` asks, `commits` it takes. */
	detail(thread: Thread, f: Finishable, who: string, commits: string): string;
	run(finisher: Finisher, thread: Thread, trunk: string, door: Door): Promise<string>;
}

const FINISH: Record<"merge" | "pr", FinishMode> = {
	merge: {
		nothing: "nothing was merged",
		ready: (finisher, thread, trunk) => finisher.mainReady(thread, trunk),
		question: (f) => `Merge ${f.name} into ${f.trunk}?`,
		detail: (thread, f, who, commits) =>
			`${who} asks to squash-merge branch ${thread.branch} (${commits}) into ${f.trunk} in ${thread.project.name}'s main checkout, then remove the thread's worktree and workspace and delete the branch.`,
		run: (finisher, thread, trunk) => finisher.merge(thread, trunk),
	},
	pr: {
		nothing: "no PR was opened",
		question: (f) => `Open a PR for ${f.name}?`,
		detail: (thread, f, who, commits) =>
			`${who} asks to have the thread push branch ${thread.branch} (${commits}) and open a pull request against ${f.trunk} with gh; its worktree and workspace go once the PR is open.`,
		run: (finisher, thread, trunk, door) => finisher.openPr(thread, trunk, door),
	},
};

export class Finisher {
	constructor(private readonly c: ServiceContext) {}

	/**
	 * `thread_finish(thread, how)`, called only on his word: `merge` squash-merges the branch into trunk in the main
	 * checkout, then removes the worktree and workspace and deletes the branch (when main moved since the review, it is
	 * merged into the branch, re-tested and re-reviewed first, 0454); `pr` has the thread push its branch and open a PR
	 * with gh, and its turn's end removes the worktree (`prOpened`). The reply says when the last review wasn't a pass
	 * (0453) and names the open threads touching the same files (0456). Every refusal leaves everything in place. The
	 * boss's call and `jocasta thread finish` wait for his Allow (0484, 0544), then look again, as the rules hold for
	 * every door (0544, SVC-5); his Finish key's Merge or PR, typed with its code, is his confirmation (0544, 0568).
	 */
	async finish(thread: Thread, how: string, door: Door = "boss"): Promise<string> {
		if (how !== "merge" && how !== "pr") {
			throw new UserError(`${door === "cli" ? "jocasta thread finish" : "thread_finish"}: how is "merge" or "pr".`);
		}
		const mode = FINISH[how];
		let found = await this.finishable(thread, door);
		await mode.ready?.(this, thread, found.trunk);
		if (door !== "key") {
			const commits = `${found.ahead} commit${found.ahead === 1 ? "" : "s"}`;
			await this.c.allow.ask(
				{
					message: mode.question(found),
					detail: [
						mode.detail(thread, found, asker(door, "thread finish"), commits),
						...(found.failed ? [`Its last review wasn't a pass (${found.failed}).`] : []),
						...(found.overlaps.length > 0
							? [`Open threads touch the same files: ${overlapSaid(found.overlaps)}.`]
							: []),
					].join("\n"),
				},
				mode.nothing,
				door,
			);
			// His Allow can take minutes: the thread may have closed, started a run or moved meanwhile.
			if (!this.c.threads.current(thread)) throw new UserError(`${found.name} closed meanwhile; ${mode.nothing}.`);
			found = await this.finishable(thread, door);
			await mode.ready?.(this, thread, found.trunk);
		}
		const reply = await mode.run(this, thread, found.trunk, door);
		const notes = [
			...(found.failed && !thread.mergeAfterReview ? [`its last review wasn't a pass (${found.failed})`] : []),
			...(found.overlaps.length > 0 ? [`open threads touch the same files: ${overlapSaid(found.overlaps)}`] : []),
		];
		return notes.length > 0 ? `${reply} Note: ${notes.join("; ")}.` : reply;
	}

	/**
	 * Finish's refusals before any merge or PR (0426, 0544): not while it works or awaits him, mid-review, mid-PR or
	 * mid-handoff, without a trunk or branch, with its worktree on another branch than the thread's (SEC-LOC-7), with
	 * uncommitted changes, or with nothing ahead of trunk. Otherwise what Finish says about it: its trunk, its commits
	 * ahead, a review that wasn't a pass, threads touching the same files. git is asked off the event loop, at once
	 * (PERF-5); the texts are worded for the door (0581).
	 */
	async finishable(thread: Thread, door: Door): Promise<Finishable> {
		const name = subject(thread);
		const waits = await this.c.lifecycle.awaitingHim(thread);
		if (thread.state === "working" || waits) {
			const why =
				waits === "plan" ? `its plan waits on ${person(door).him}` : waits === "question" ? "blocked" : thread.state;
			throw new UserError(`${name} isn't idle (${why}); finish it once it is.`);
		}
		if (thread.phase === "review" || thread.phase === "finish") {
			throw new UserError(`${name} is still in its ${thread.phase === "review" ? "review" : "PR"} turn; wait for it.`);
		}
		// Closing the workspace mid-`/handoff` would lose the task's summary (CO-2); a handoff that never comes stops
		// holding it after 5 minutes.
		if (
			thread.phase === "handoff" &&
			!this.c.lifecycle.handedOff(thread) &&
			Date.now() - (thread.handoffAt ?? 0) < HANDOFF_HOLD_MS
		) {
			throw new UserError(`${name} is still writing its handoff; try again in a minute.`);
		}
		const trunk = await this.c.places.trunk(thread.project);
		if (!trunk || !thread.branch) {
			throw new UserError(`${name}: ${trunk ? "the thread has no branch" : "the project has no trunk branch"}.`);
		}
		const main = thread.project.mainPath;
		const [onBranch, status, count, overlaps] = await Promise.all([
			gitOut(thread.path, ["symbolic-ref", "--short", "-q", "HEAD"]),
			gitOut(thread.path, ["status", "--porcelain"]),
			gitOut(main, ["rev-list", "--count", `${trunk}..${thread.branch}`]),
			this.c.status.overlaps(thread),
		]);
		// The review covered the worktree's HEAD: a worktree switched to another branch isn't what Finish would merge.
		if (onBranch !== thread.branch) {
			throw new UserError(
				`${name}'s worktree is on ${onBranch ?? "a detached HEAD"}, not ${thread.branch}; finish it on its branch.`,
			);
		}
		if (status) {
			throw new UserError(`${name} has uncommitted changes in its worktree; have it commit or drop them first.`);
		}
		const ahead = Number(count ?? 0);
		if (ahead === 0) throw new UserError(`${name} has no commits ahead of ${trunk}; nothing to finish.`);
		return { name, trunk, ahead, failed: this.c.review.failed(thread), overlaps };
	}

	/**
	 * `thread_finish` merge (0426, 0454): when trunk gained commits the branch doesn't hold, trunk is merged into the
	 * branch in its worktree and the review step runs again; the squash follows only once that review passes
	 * (`afterReview`). A conflict aborts the merge and stops. Otherwise the branch is squash-merged now.
	 */
	async merge(thread: Thread, trunk: string): Promise<string> {
		const name = subject(thread);
		await this.mainReady(thread, trunk);
		if ((await gitOut(thread.project.mainPath, ["merge-base", "--is-ancestor", trunk, thread.branch])) !== undefined) {
			return this.squash(thread, trunk);
		}
		if (!this.c.panes.hunk()) {
			throw new UserError(
				`${trunk} moved since ${name}'s review and no new review can run (hunk isn't installed); nothing was merged.`,
			);
		}
		const merged = await gitAsync(thread.path, ["merge", "--no-edit", trunk], MERGE_TIMEOUT_MS);
		if (merged.code !== 0) {
			const conflicts = ((await gitOut(thread.path, ["diff", "--name-only", "--diff-filter=U"])) ?? "")
				.split("\n")
				.filter(Boolean);
			await gitAsync(thread.path, ["merge", "--abort"], GIT_MS);
			throw new UserError(
				`${trunk} moved since ${name}'s review, and bringing it into ${thread.branch} conflicts${conflicts.length > 0 ? ` in ${conflicts.join(", ")}` : ""}; that merge was aborted and everything stays.`,
			);
		}
		thread.mergeAfterReview = true;
		thread.head = await gitOut(thread.path, ["rev-parse", "HEAD"]);
		await this.c.review.taskEnded(thread, `${trunk} merged in`);
		return `${trunk} moved since ${name}'s review: it was merged into ${thread.branch}, which is being tested and reviewed again; it merges into ${trunk} only when that passes, and you hear either way.`;
	}

	/** Refuses when the main checkout has uncommitted changes or isn't on trunk (0426). */
	async mainReady(thread: Thread, trunk: string): Promise<void> {
		const main = thread.project.mainPath;
		const [status, current] = await Promise.all([
			gitOut(main, ["status", "--porcelain", "--untracked-files=no"]),
			gitOut(main, ["symbolic-ref", "--short", "-q", "HEAD"]),
		]);
		if (status) {
			throw new UserError(`the main checkout of ${thread.project.name} has uncommitted changes; nothing was merged.`);
		}
		if (current !== trunk) {
			throw new UserError(
				`the main checkout of ${thread.project.name} is on ${current ?? "a detached HEAD"}, not ${trunk}; nothing was merged.`,
			);
		}
	}

	/**
	 * The merge once its new review passed (0454): squashed now, unless trunk moved again meanwhile; said as one event
	 * with what the review found, `reviewed`.
	 */
	async afterReview(thread: Thread, reviewed: string): Promise<void> {
		const trunk = await this.c.places.trunk(thread.project);
		const passed = `its new review passed (${reviewed})`;
		let said: string;
		try {
			if (!trunk) throw new UserError("the project has no trunk branch; nothing was merged.");
			await this.mainReady(thread, trunk);
			const ancestor = await gitOut(thread.project.mainPath, ["merge-base", "--is-ancestor", trunk, thread.branch]);
			if (ancestor === undefined) {
				throw new UserError(`${trunk} moved again during its review; nothing was merged (finish it again).`);
			}
			said = await this.squash(thread, trunk);
		} catch (error) {
			await this.c.events.thread(thread, `${passed}; not merged: ${(error as Error).message}`, "normal");
			await this.c.review.handoff(thread);
			return;
		}
		await this.c.events.thread(thread, `${passed}; ${said.charAt(0).toLowerCase()}${said.slice(1)}`, "normal");
	}

	/** Squash-merges the branch into trunk in the main checkout, then removes the thread (0426). */
	private async squash(thread: Thread, trunk: string): Promise<string> {
		const name = subject(thread);
		const main = thread.project.mainPath;
		const [count, log] = await Promise.all([
			gitOut(main, ["rev-list", "--count", `${trunk}..${thread.branch}`]),
			gitOut(main, ["log", "--reverse", "--format=- %s", `${trunk}..${thread.branch}`]),
		]);
		const ahead = Number(count ?? 0);
		const subjects = log ?? "";
		const squash = await gitAsync(main, ["merge", "--squash", thread.branch], MERGE_TIMEOUT_MS);
		if (squash.code !== 0) {
			const conflicts = ((await gitOut(main, ["diff", "--name-only", "--diff-filter=U"])) ?? "")
				.split("\n")
				.filter(Boolean);
			await gitAsync(main, ["reset", "--merge"], GIT_MS);
			throw new UserError(
				`${name} doesn't merge cleanly into ${trunk}${conflicts.length > 0 ? ` (conflicts in ${conflicts.join(", ")})` : ""}; the merge was aborted and everything stays.`,
			);
		}
		// The repo's hooks and signing run here: off the event loop, bounded (CO-10).
		const committed = await gitAsync(
			main,
			["commit", "-q", "-m", thread.title, "-m", `Squash-merged from ${thread.branch}:\n${subjects}`],
			COMMIT_TIMEOUT_MS,
		);
		if (committed.code !== 0) {
			await gitAsync(main, ["reset", "--merge"], GIT_MS);
			if (committed.code === null) {
				throw new UserError("the merge commit's hooks didn't finish in 10 minutes; nothing was merged.");
			}
			const why = committed.stderr.trim() || committed.stdout.trim() || `exit ${committed.code}`;
			throw new UserError(`${name}: the squash commit failed (${why}); nothing was merged.`);
		}
		const commit = await gitOut(main, ["rev-parse", "--short", "HEAD"]);
		this.c.day.landed(thread.paneId, thread.title, thread.project.id, `merged into ${trunk} as ${commit}`);
		// The project's code index follows its main checkout (0562): re-indexed in the background, never failing here.
		void this.c.options.codeIndex?.refresh(thread.project);
		const merged = `Merged ${name} into ${trunk} as ${commit} (${ahead} commit${ahead === 1 ? "" : "s"} squashed)`;
		const removed = await this.removeThread(thread);
		if (removed) return `${merged}, but ${removed}.`;
		if ((await gitAsync(main, ["branch", "-D", thread.branch], GIT_MS)).code !== 0) {
			return `${merged}; its worktree and workspace are removed, but branch ${thread.branch} couldn't be deleted.`;
		}
		return `${merged}; its worktree, workspace and branch are removed.`;
	}

	/**
	 * Removes a finished thread's workspace and worktree; what stays and why when it couldn't. herdr's own
	 * `worktree.remove` stops the workspace's terminals and on Windows restarts them as shells in the worktree, which
	 * keep the folder locked (F-02). So the workspace is closed first, then git removes the checkout once the panes'
	 * processes are gone, trying again for a while. While it runs, the refresh tick neither finds the thread again nor
	 * reports its workspace closed (CR-11).
	 */
	private async removeThread(thread: Thread): Promise<string | undefined> {
		const { threads, finishing } = this.c;
		threads.drop(thread.paneId);
		finishing.set(thread.workspaceId, "removing");
		// Its summaries are read after the folder is gone: they still belong to this project (CO-2).
		this.c.places.removed(thread.path, thread.project);
		try {
			await this.c.herdr.closeWorkspace(thread.workspaceId);
		} catch (error) {
			finishing.delete(thread.workspaceId);
			threads.add(thread);
			return `herdr couldn't close its workspace (${(error as Error).message}); its worktree, workspace and branch stay`;
		}
		// Kept until a refresh no longer sees the workspace: one read during the closing may still list its panes.
		finishing.set(thread.workspaceId, "removed");
		this.c.day.threadUnblocked(thread.paneId);
		const failed = await removeCheckout(thread.project.mainPath, thread.path);
		if (failed) {
			return `its workspace is closed, but its worktree folder ${thread.path} couldn't be removed (${failed}); the folder and branch stay`;
		}
		this.c.reviewed.mark(thread.path, undefined);
		this.c.numbers.forget(thread.path);
		this.c.status.forget(thread);
		if (thread.task) closeTask(thread.task, "finished", (error) => this.c.warn(error));
		return undefined;
	}

	/**
	 * Runs gh in `cwd` (never prompting, at most a minute, CO-8); undefined when gh isn't installed, `code: null` when
	 * it didn't answer in time.
	 */
	private async gh(args: string[], cwd: string): Promise<{ code: number | null; out: string } | undefined> {
		try {
			const run = await runProgram([...(this.c.options.gh ?? ["gh"]), ...args], {
				cwd,
				env: { ...process.env, GH_PROMPT_DISABLED: "1" },
				timeoutMs: this.c.options.ghTimeoutMs ?? GH_TIMEOUT_MS,
			});
			return { code: run.code, out: `${run.stdout}${run.stderr}`.trim() };
		} catch {
			// Bun throws at once when the program isn't on PATH.
			return undefined;
		}
	}

	async openPr(thread: Thread, trunk: string, door: Door): Promise<string> {
		const name = subject(thread);
		const auth = await this.gh(["auth", "status"], thread.path);
		if (!auth)
			throw new UserError("gh isn't installed: install GitHub CLI (https://cli.github.com), then gh auth login.");
		if (auth.code === null) throw new UserError("gh didn't answer in a minute; nothing was changed.");
		if (auth.code !== 0) {
			const said = `gh isn't logged in (gh auth status: ${auth.out.split("\n")[0]})`;
			// Worded for the door (0581): the boss tells him; his key and the CLI tell him what to do.
			const then = {
				boss: "; he runs gh auth login.",
				key: ": run gh auth login, then Finish again.",
				cli: ": run gh auth login, then jocasta thread finish again.",
			}[door];
			throw new UserError(`${said}${then}`);
		}
		const remotes = ((await gitOut(thread.path, ["remote"])) ?? "").split("\n").filter(Boolean);
		const remote = remotes.includes("origin") ? "origin" : remotes[0];
		if (!remote) throw new UserError(`${thread.project.name} has no git remote to push ${thread.branch} to.`);
		setPhase(thread, "finish", "it is asked to open its PR");
		try {
			await this.c.herdr.prompt(thread.paneId, prRequest(remote, thread.branch, trunk));
		} catch (error) {
			setPhase(thread, "task", "it didn't take the PR request");
			throw new UserError(`${name} didn't take the request: ${(error as Error).message}`);
		}
		return `Asked ${name} to push ${thread.branch} and open a PR against ${trunk}; once it's open, its worktree and workspace are removed and you hear it.`;
	}

	/** The PR turn ended (0426): an open PR on the pushed branch removes the worktree and workspace; else all stays. */
	async prOpened(thread: Thread): Promise<void> {
		setPhase(thread, "task", "its PR turn ended");
		const view = await this.gh(["pr", "view", thread.branch, "--json", "url,state,headRefOid"], thread.path);
		let pr: { url?: string; state?: string; headRefOid?: string } | undefined;
		try {
			if (view?.code === 0) pr = JSON.parse(view.out) as typeof pr;
		} catch {
			// Not JSON: reported below with gh's own first line.
		}
		let reason: string | undefined;
		if (!pr) {
			reason =
				view?.code === null
					? "gh didn't answer in a minute"
					: `gh finds no PR for ${thread.branch}${view ? `: ${view.out.split("\n")[0]}` : ""}`;
		} else if (pr.state !== "OPEN") reason = `its PR is ${pr.state?.toLowerCase() ?? "not open"}`;
		else if (pr.headRefOid !== (await gitOut(thread.path, ["rev-parse", "HEAD"]))) {
			reason = "its PR doesn't have the branch's last commit (not pushed?)";
		}
		if (!pr || reason) {
			await this.c.events.thread(thread, `no PR: ${reason}; everything stays`, "normal");
			return;
		}
		const removed = await this.removeThread(thread);
		this.c.day.landed(thread.paneId, thread.title, thread.project.id, "PR open");
		await this.c.events.thread(
			thread,
			`PR open${removed ? `, but ${removed}` : "; its worktree and workspace are removed, the branch stays"} · ${pr.url}`,
			"normal",
		);
	}
}
