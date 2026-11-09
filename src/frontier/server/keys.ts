/**
 * His thread keys in herdr (0544, 0554, 0581): Finish, Stop, Review and New thread on the focused thread or the one
 * picked in `today`. Their own key and code are his confirmation; every other rule holds.
 */
import { UserError } from "../../shared/errors.ts";
import { isTodayPane } from "./boss.ts";
import type { ServiceContext } from "./context.ts";
import { subject, type Thread } from "./thread.ts";
import { briefTitle, overlapSaid } from "./wording.ts";
import { gitOut } from "./worktrees.ts";

/** A key's outcome that is only half done (0581, C11): every such text says what landed, then ", but" what didn't. */
const HALF_DONE = /, but /;

/** The thread keys and the key `today` takes for each on its picked thread (0581, UX-16). */
const TODAY_KEY = { Finish: "f", Stop: "s", Review: "r" } as const;

export class Keys {
	constructor(
		private readonly c: ServiceContext,
		/** Runs one key's call counted like a tool call (SVC-13). */
		private readonly track: <T>(run: () => Promise<T>) => Promise<T>,
	) {}

	/**
	 * The thread `paneId` belongs to: its own pane, or another pane of its workspace (its hunk pane, a shell beside it).
	 * Refused for any other pane, the boss's among them; in `today`'s own pane the refusal names `today`'s key for the
	 * action (0581, UX-16).
	 */
	private async threadOfPane(paneId: string | undefined, action: keyof typeof TODAY_KEY): Promise<Thread> {
		const own = paneId ? this.c.threads.get(paneId) : undefined;
		if (own) return own;
		const pane = paneId ? (await this.c.herdr.panes()).find((p) => p.paneId === paneId) : undefined;
		const thread = pane && this.c.threads.all().find((t) => t.workspaceId === pane.workspaceId);
		if (thread) return thread;
		if (pane && isTodayPane(pane)) {
			throw new UserError(`${action}: in today, press ${TODAY_KEY[action]} on the picked thread.`);
		}
		throw new UserError(`${action}: the focused pane isn't a thread's; focus a thread or pick one in today.`);
	}

	/** Stop (0544): one Escape into the thread's omp, while it works or asks him. */
	stop(paneId: string | undefined): Promise<string> {
		return this.track(async () => {
			const thread = await this.threadOfPane(paneId, "Stop");
			const name = subject(thread);
			if (thread.state !== "working" && thread.state !== "blocked") {
				throw new UserError(`Stop: ${name} isn't working; nothing to stop.`);
			}
			await this.c.herdr.stop(thread.paneId);
			return `Stopped ${name}'s run.`;
		});
	}

	/** Finish's checks before it asks merge or PR (0544): the thread's pane, its name, its trunk and what Finish would take. */
	async finishable(
		paneId: string | undefined,
	): Promise<{ paneId: string; name: string; detail: string; trunk: string }> {
		const thread = await this.threadOfPane(paneId, "Finish");
		const { name, trunk, ahead, failed, overlaps } = await this.c.finisher.finishable(thread, "key");
		const detail = [
			`${ahead} commit${ahead === 1 ? "" : "s"} on ${thread.branch}, onto ${trunk}.`,
			...(failed ? [`Its last review wasn't a pass (${failed}).`] : []),
			...(overlaps.length > 0 ? [`Open threads touch the same files: ${overlapSaid(overlaps)}.`] : []),
		].join("\n");
		return { paneId: thread.paneId, name, detail, trunk };
	}

	/**
	 * Finish with his choice (0426, 0544, 0568): Merge or PR typed with its code confirms, every other rule holds. A
	 * merge that landed but left something behind (its workspace, folder or branch) is half done: an error (0581, C11).
	 */
	async finish(paneId: string, how: "merge" | "pr"): Promise<{ text: string; isError: boolean }> {
		const thread = this.c.threads.get(paneId);
		if (!thread) throw new UserError("Finish: the thread closed meanwhile.");
		const text = await this.track(() => this.c.finisher.finish(thread, how, "key"));
		return { text, isError: HALF_DONE.test(text) };
	}

	/**
	 * Review (0394, 0406, 0554): the review step on demand, as at a task's end: hunk beside the thread at half width on
	 * `<trunk>...<branch>`, and the thread asked to test and review its commits. Not while it works, awaits him or is in
	 * another step; nothing to review without commits ahead of trunk.
	 */
	review(paneId: string | undefined): Promise<string> {
		return this.track(async () => {
			const { c } = this;
			const thread = await this.threadOfPane(paneId, "Review");
			const name = subject(thread);
			const waits = await c.lifecycle.awaitingHim(thread);
			if (thread.state === "working" || waits) {
				throw new UserError(
					`Review: ${name} isn't idle (${waits === "plan" ? "its plan waits on you" : waits === "question" ? "blocked" : thread.state}); review it once it is.`,
				);
			}
			if (thread.phase !== "task") throw new UserError(`Review: ${name} is in its ${thread.phase} step; wait for it.`);
			if (!c.panes.hunk()) throw new UserError("Review: hunk isn't installed, so nothing can show the diff.");
			const [trunk, head] = await Promise.all([
				c.places.trunk(thread.project),
				gitOut(thread.path, ["rev-parse", "HEAD"]),
			]);
			if (!trunk || !head || !thread.branch) {
				throw new UserError(`Review: ${name} has ${trunk ? "no branch" : "no trunk branch in its project"}.`);
			}
			if (Number((await gitOut(thread.path, ["rev-list", "--count", `${trunk}..HEAD`])) ?? 0) === 0) {
				throw new UserError(`Review: ${name} has no commits ahead of ${trunk}; nothing to review.`);
			}
			const { said, subject: last } = await c.review.commitsSaid(thread, undefined, head);
			await c.review.taskEnded(thread, said, last);
			return `Reviewing ${name}: hunk beside it, the thread tests and reviews ${said}.`;
		});
	}

	/**
	 * New thread's form (0554, 0581): his git projects with one always picked: the project of `paneId` (a thread picked
	 * in `today`, or the focused pane), else the one of the pane he last focused, else the last one a thread started in,
	 * else the first.
	 */
	async newThreadForm(paneId?: string): Promise<{ projects: string[]; project: string }> {
		const { c } = this;
		const panes = await c.herdr.panes();
		c.starts.noteFocus(panes);
		const projects = c.places
			.listed()
			.filter((p) => p.git)
			.map((p) => p.name);
		const first = projects[0];
		if (!first) throw new UserError("New thread: no git project to start one in; add one with `jocasta project add`.");
		const pane = paneId ? panes.find((p) => p.paneId === paneId) : undefined;
		const ofPane =
			(paneId ? c.threads.get(paneId)?.project : undefined) ??
			c.threads.all().find((t) => t.workspaceId === pane?.workspaceId)?.project ??
			(paneId ? c.starts.paneProject(paneId) : undefined) ??
			(pane?.cwd ? c.places.projectAt(pane.cwd) : undefined);
		const project = [ofPane, c.starts.focusedProject, c.starts.lastStartedIn]
			.map((p) => p?.name)
			.find((name): name is string => name !== undefined && projects.includes(name));
		return { projects, project: project ?? first };
	}

	/**
	 * New thread from his form (0544, 0554): its title from the brief; it opens in plan mode and waits for him. A start
	 * whose brief wasn't sent is half done: an error (0581, C11).
	 */
	async start(project: string, brief: string): Promise<{ text: string; isError: boolean }> {
		const started = await this.track(() => this.c.starts.start(project, briefTitle(brief), brief, "key"));
		return { text: started.text, isError: !started.done };
	}
}
