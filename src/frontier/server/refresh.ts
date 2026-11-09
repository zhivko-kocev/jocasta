/**
 * The refresh tick (0402, 0403, 0404, 0405, 0521, CR-09): every few seconds the boss pane, closed and new threads,
 * panes to start omp in again, missed states, plans, catch-up reviews, hand commits, spend, the sidebar and the HUD.
 * Each is its own step (SVC-3): one that throws is said once per reason and the rest still run.
 */
import { basename } from "node:path";
import { sameDir } from "../../shared/pathkey.ts";
import { closeHerdrDefault } from "./boss.ts";
import type { ServiceContext } from "./context.ts";
import { type HerdrSnapshot, lastRevivedAt, type PaneInfo } from "./herdr.ts";
import { OmpBackoff } from "./pane-launch.ts";
import { findTask } from "./task-file.ts";
import { isIdle, type ReviewResult, type Thread } from "./thread.ts";
import { gitOut } from "./worktrees.ts";

/** After herdr was started again (0431), an empty pane list this soon is herdr still restoring, not panes gone. */
const REVIVE_SETTLE_MS = 10_000;
/** Thread panes relaunched at once after herdr restored them (PERF-5): herdr drops connections under bursts (0517). */
const RELAUNCH_POOL = 3;

/**
 * What one tick works from: when herdr's list was read, the snapshot (read again after the tick changed the boss pane
 * or closed herdr's default workspace), its panes by id, and the threads known before the list was read (SVC-2).
 */
interface Tick {
	readAt: number;
	snapshot: HerdrSnapshot;
	byId: Map<string, PaneInfo>;
	known: [string, Thread][];
}

export class Refresher {
	private refreshing: Promise<void> | undefined;
	/** What each refresh step last warned of (SVC-3), so a failing step is said once per reason. */
	private readonly stepWarned = new Map<string, string>();
	/** The panes `discover` last looked at, so an unchanged pane set isn't looked at again (CR-12). */
	private discoveredKey = "";
	/** herdr's session was new at start (0495): its default workspace is looked at until it is closed or kept. */
	herdrDefault = false;
	private readonly backoff: OmpBackoff;

	constructor(private readonly c: ServiceContext) {
		this.backoff = new OmpBackoff(c);
	}

	/** The panes are looked at again for threads on the next tick (a catalog sync may add projects). */
	rediscover(): void {
		this.discoveredKey = "";
	}

	/**
	 * Every few seconds: the boss pane is there and runs omp (0404), threads closed in herdr are dropped with an
	 * `[event]` (0405), new ones found, panes herdr restored as shells or whose omp ended while they stayed open (0521)
	 * run omp again, as do idle panes started before panes had keys (0578), a state herdr's events missed is caught up
	 * (CR-09), found threads' unreviewed commits get their review (0394), each thread's ledger is read (0582, and its
	 * spend past $50 said once, 0541), and the sidebar tokens and the HUD are reported (0402, 0403). The tick reads
	 * herdr's snapshot (again after it changed the boss pane or closed herdr's default workspace) and asks herdr for the
	 * panes' roots, screens and prompts as the steps need them. One tick at a time: a call meanwhile gets the running one.
	 */
	refresh(): Promise<void> {
		this.refreshing ??= this.tick().finally(() => {
			this.refreshing = undefined;
		});
		return this.refreshing;
	}

	/** The tick still under way, for `stop` to wait for. */
	running(): Promise<void> | undefined {
		return this.refreshing;
	}

	/** One step of the tick (SVC-3): its result, or undefined once a throw was said (once per step and reason). */
	private async step<T>(name: string, run: () => Promise<T> | T): Promise<T | undefined> {
		try {
			const result = await run();
			this.stepWarned.delete(name);
			return result;
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			if (this.stepWarned.get(name) !== message) {
				this.stepWarned.set(name, message);
				this.c.warn(new Error(`refresh, ${name}: ${message}`));
			}
			return undefined;
		}
	}

	private async tick(): Promise<void> {
		const { c } = this;
		if (c.stopping) return;
		const readAt = Date.now();
		c.places.expire(readAt);
		// The threads known before herdr's list is read: one registered while the tick runs isn't in that list (SVC-2).
		const known = c.threads.entries();
		const read = await this.step("reading herdr's panes", () => c.herdr.snapshot());
		if (!read) return;
		// A herdr just started again (0431) may list no panes while it restores them: not every thread closed (CO-20).
		const revived = lastRevivedAt();
		if (read.panes.length === 0 && revived !== undefined && readAt - revived < REVIVE_SETTLE_MS) return;
		c.launched.seen(read.panes);
		let snapshot = (await this.step("the boss pane", () => this.bossPane(read))) ?? read;
		// A new session's default workspace closes while untouched, once Jocasta's exists (0495).
		if (this.herdrDefault) {
			const before = snapshot;
			snapshot = (await this.step("herdr's default workspace", () => this.herdrDefaultClosed(before))) ?? before;
		}
		const tick: Tick = { readAt, snapshot, byId: new Map(snapshot.panes.map((p) => [p.paneId, p])), known };
		c.status.workspaces = snapshot.workspaces;
		await this.step("the focused pane", () => c.starts.noteFocus(snapshot.panes));
		for (const [workspaceId, step] of c.finishing) {
			if (step === "removed" && !snapshot.workspaces.some((w) => w.id === workspaceId)) c.finishing.delete(workspaceId);
		}
		const bossPane = c.boss ? tick.byId.get(c.boss.paneId) : undefined;
		if (c.boss && bossPane?.sessionPath) c.boss.sessionPath = c.sessions.moved(bossPane.sessionPath);
		await this.step("closed threads", () => this.closedThreads(tick));
		await this.step("finding threads", () => this.discover(snapshot));
		await this.step("thread panes", () => this.threadPanes(tick));
		await this.step("the thread panes' omp", () => this.threadOmps(tick));
		await this.step("project panes", () => this.projectPanes(tick));
		// `today` beside the boss and the Agents view (0553), healed like the boss pane (0518, 0549).
		const plugin = c.options.plugin;
		if (plugin) await this.step("the plugin's panes", () => plugin.heal(snapshot, c.boss?.paneId));
		await this.step("missed states", () => c.lifecycle.reconcile(tick.byId, readAt));
		// A plan omp proposed that no idle event saw (B1), or one an omp resumed after a restart still holds (B3,
		// counted unsaid, as `discover` does); before the catch-up, so no review prompt goes into it.
		await this.step("plans", () => this.plans());
		await this.step("reopened plans", () => this.reopenedPlans());
		await this.step("catch-up reviews", () => this.catchUps());
		await this.step("approved plans", () => this.approvedPlans());
		await this.step("hand commits", () => c.lifecycle.handCommits(tick.byId));
		await this.step("spend", () => c.status.spendPassed());
		await this.step("sidebar and HUD", () => c.status.report());
	}

	/** The boss pane, ensured again when it is gone, its omp ended (0521) or it runs omp without this run's key (0484). */
	private async bossPane(read: HerdrSnapshot): Promise<HerdrSnapshot> {
		const { c } = this;
		const current = read.panes.find((p) => p.paneId === c.boss?.paneId);
		// An omp that ended in the boss pane starts again there, as at the start (0521).
		const bossEnded = current !== undefined && this.backoff.ompEnded(current);
		if (current && !bossEnded && !c.launched.restored(current) && !c.panes.unkeyedBoss(current)) return read;
		await c.panes.ensureBoss({ adopt: bossEnded });
		const again = await c.herdr.snapshot();
		c.launched.seen(again.panes);
		return again;
	}

	/** herdr's default workspace in a new session (0495): closed while untouched, once Jocasta's exists. */
	private async herdrDefaultClosed(before: HerdrSnapshot): Promise<HerdrSnapshot> {
		const outcome = await closeHerdrDefault(this.c.herdr, before);
		if (outcome !== "wait") this.herdrDefault = false;
		return outcome === "closed" ? await this.c.herdr.snapshot() : before;
	}

	/** Threads whose pane went in herdr are dropped and said closed (0405); the others follow their session's moves. */
	private async closedThreads({ byId, known }: Tick): Promise<void> {
		const { c } = this;
		for (const [paneId, thread] of known) {
			// Finished, relaunched or dropped meanwhile: not this list's to judge.
			if (c.threads.get(paneId) !== thread) continue;
			const pane = byId.get(paneId);
			if (!pane) {
				c.threads.drop(paneId);
				if (!c.finishing.has(thread.workspaceId)) await c.lifecycle.closed(thread);
			} else if (pane.sessionPath) thread.sessionPath = c.sessions.moved(pane.sessionPath);
		}
	}

	/**
	 * Threads herdr runs that the service doesn't know yet: a pane in a linked worktree of a watched project that
	 * Jocasta started omp in or that runs omp (CR-01) — never a hunk pane or a plain shell. The thread's other pane in
	 * its workspace that runs hunk is its review pane (CR-02), so a later task reuses it.
	 */
	async discover(snapshot: HerdrSnapshot): Promise<void> {
		const { c } = this;
		const { panes } = snapshot;
		const key = panes.map((p) => `${p.paneId}|${p.agent ?? ""}|${p.cwd ?? ""}`).join("\n");
		if (key === this.discoveredKey) return;
		const labels = new Map(snapshot.workspaces.map((w) => [w.id, w.label]));
		const taken = new Set(c.threads.all().map((t) => t.workspaceId));
		for (const pane of panes) {
			if (!pane.cwd || pane.paneId === c.boss?.paneId || c.threads.has(pane.paneId)) continue;
			if (taken.has(pane.workspaceId) || c.finishing.has(pane.workspaceId)) continue;
			if (pane.agent !== "omp" && !c.launched.has(pane.paneId)) continue;
			const checkout = c.places.checkout(pane.cwd);
			if (!checkout || sameDir(checkout.top, checkout.main)) continue;
			const project = c.places.projectAt(pane.cwd);
			if (!project || !sameDir(project.mainPath, checkout.main)) continue;
			taken.add(pane.workspaceId);
			// A pane found idle with its plan in omp's Plan Review still waits on him. Read in steps, as at the start.
			const atRest = isIdle(pane.agentState);
			const sessionPath = pane.sessionPath ? c.sessions.moved(pane.sessionPath) : undefined;
			if (atRest && sessionPath) await c.sessions.warm([sessionPath], () => c.stopping);
			const stats = atRest && sessionPath ? c.sessions.read(sessionPath) : undefined;
			const plan = stats?.planAwaiting ?? null;
			const reviewPane = panes.find(
				(p) =>
					p.workspaceId === pane.workspaceId &&
					p.paneId !== pane.paneId &&
					(p.label === "review" || /hunk/i.test(p.label ?? "")),
			)?.paneId;
			const task = findTask(project.id, checkout.top);
			const head = await gitOut(checkout.top, ["rev-parse", "HEAD"]);
			// Found meanwhile (a start, the next tick): that one stays.
			if (c.threads.has(pane.paneId)) continue;
			// Done and reviewed before a restart (its task file says so): its last review still awaits him (0432, 0549).
			const last = task?.state === "done and reviewed" ? task.reviews.at(-1) : undefined;
			const result: ReviewResult | undefined = last && {
				...(last.notes !== undefined ? { notes: last.notes } : {}),
				...(last.blocking !== undefined ? { blocking: last.blocking } : {}),
				tests: last.tests ?? "unknown",
				...(last.proof ? { proof: last.proof } : {}),
				checks: last.checks,
				at: last.at,
			};
			c.threads.add({
				paneId: pane.paneId,
				workspaceId: pane.workspaceId,
				project,
				title: labels.get(pane.workspaceId) ?? checkout.branch ?? basename(checkout.top),
				branch: checkout.branch ?? "",
				path: checkout.top,
				state: pane.agentState,
				shown:
					pane.agentState === "working"
						? "working"
						: pane.agentState === "blocked" || plan
							? "blocked"
							: result
								? "done"
								: "idle",
				changedAt: Date.now(),
				head,
				phase: "task",
				catchUp: true,
				plan: {
					reopened: true,
					...(plan ? { awaiting: plan } : {}),
					...(stats ? { sentBack: stats.planSentBack } : {}),
				},
				...(reviewPane || result
					? { review: { ...(reviewPane ? { pane: reviewPane } : {}), ...(result ? { result } : {}) } }
					: {}),
				...(sessionPath ? { sessionPath } : {}),
				...(task ? { task } : {}),
			});
		}
		// Only once every pane was looked at: a throw part-way looks again on the next tick (SVC-11).
		this.discoveredKey = key;
	}

	/**
	 * A pane Jocasta started before panes had keys (0578), idle with omp in it: started again with its key, resuming its
	 * session, so its thread tools answer it. Never mid-turn, mid-step or with a plan or question waiting.
	 */
	private unkeyed(pane: PaneInfo, thread?: Thread): boolean {
		const { c } = this;
		if (!c.launched.has(pane.paneId) || c.launched.keyOf(pane.paneId)) return false;
		if (pane.agent !== "omp" || !isIdle(pane.agentState) || c.lifecycle.busy(pane.paneId)) return false;
		return !thread || (thread.phase === "task" && !thread.plan?.awaiting && isIdle(thread.state));
	}

	/**
	 * A thread pane herdr restored as a shell, one whose omp ended while it stayed open (0521), or an idle one started
	 * before panes had keys (0578): a few at once (PERF-5), each on its own (SVC-3).
	 */
	private async threadPanes({ byId }: Tick): Promise<void> {
		const { c } = this;
		const due = c.threads.all().filter((thread) => {
			const pane = byId.get(thread.paneId);
			if (!pane || !this.backoff.due(thread.path)) return false;
			return c.launched.restored(pane) || this.backoff.ompEnded(pane) || this.unkeyed(pane, thread);
		});
		for (let at = 0; at < due.length; at += RELAUNCH_POOL) {
			await Promise.all(
				due
					.slice(at, at + RELAUNCH_POOL)
					.map((thread) => this.backoff.respawning(thread.path, () => c.lifecycle.relaunch(thread))),
			);
		}
	}

	/** Each thread pane's omp, read once per terminal (0490): a new pane, or one herdr started a process in again. */
	private async threadOmps({ byId }: Tick): Promise<void> {
		const { c } = this;
		await Promise.all(
			c.threads.all().map((thread) => {
				const pane = byId.get(thread.paneId);
				if (!pane || (thread.omp && thread.omp.terminal === pane.terminalId)) return undefined;
				return c.omps.remember(thread, pane.terminalId);
			}),
		);
		c.omps.complete(byId);
	}

	/**
	 * A project pane (the project's own omp in its main checkout) herdr restored as a shell, one whose omp ended while
	 * it stayed open (0521), or an idle one started before panes had keys (0578); never a finishing thread's.
	 */
	private async projectPanes({ snapshot }: Tick): Promise<void> {
		const { c } = this;
		const labels = new Map(snapshot.workspaces.map((w) => [w.id, w.label]));
		for (const pane of snapshot.panes) {
			const cwd = pane.cwd;
			if (pane.paneId === c.boss?.paneId || c.threads.has(pane.paneId) || !cwd) continue;
			if (c.finishing.has(pane.workspaceId) || !this.backoff.due(cwd)) continue;
			const unkeyed = this.unkeyed(pane);
			if (!c.launched.restored(pane) && !c.launched.ended(pane) && !unkeyed) continue;
			const project = c.places.projectAt(cwd);
			if (!project || !sameDir(project.mainPath, cwd)) continue;
			if (labels.get(pane.workspaceId) !== project.name) continue;
			if (c.launched.restored(pane) || unkeyed || this.backoff.ompEnded(pane)) {
				await this.backoff.respawning(cwd, () => c.panes.respawn(pane.paneId, { cwd, projectId: project.id }));
			}
		}
	}

	/** Plans awaiting him in idle threads that no idle event saw (B1, B3). */
	private async plans(): Promise<void> {
		const { c } = this;
		for (const thread of c.threads.all()) {
			if (thread.plan?.awaiting || !thread.sessionPath || !isIdle(thread.state)) continue;
			const plan = c.sessions.read(thread.sessionPath).planAwaiting;
			if (plan) await c.lifecycle.planFound(thread, plan, { announce: !thread.catchUp });
		}
	}

	/**
	 * A plan awaiting him in a reopened omp gets omp's Plan Review back (0494); a reopened omp idle with no plan
	 * awaiting in its session, read as the file is now, has none to show.
	 */
	private async reopenedPlans(): Promise<void> {
		const { c } = this;
		for (const thread of c.threads.all()) {
			if (!thread.plan?.reopened) continue;
			if (thread.plan.awaiting) await c.lifecycle.reopenPlanReview(thread);
			else if (isIdle(thread.state) && thread.sessionPath) {
				if (!c.sessions.read(thread.sessionPath, true).planAwaiting) thread.plan.reopened = false;
			}
		}
	}

	/** Found threads, idle now, get the review their unreviewed commits never had (0394). */
	private async catchUps(): Promise<void> {
		for (const thread of this.c.threads.all()) {
			if (!thread.catchUp || thread.phase !== "task" || thread.plan?.awaiting || !isIdle(thread.state)) continue;
			thread.catchUp = false;
			await this.c.review.catchUp(thread);
		}
	}

	/** A plan approved since the last state change (a new session's turn may bring none): the task file says so. */
	private approvedPlans(): void {
		const { c } = this;
		for (const thread of c.threads.values()) {
			if (!thread.plan?.proposed || thread.plan.awaiting || !thread.sessionPath) continue;
			const before = thread.task?.plan;
			c.lifecycle.noteApproval(thread, c.sessions.read(thread.sessionPath));
			if (thread.task?.plan !== before) c.status.writeTask(thread);
		}
	}
}
