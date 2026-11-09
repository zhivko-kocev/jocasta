/**
 * A thread's states as herdr and omp report them (0405, 0407, 0547, CR-09): each pane's changes handled in order,
 * a run's end turned into a task end, a review or handoff step, a plan awaiting him (0494) or a turn; a pane closed,
 * relaunched (0521, 0549) or committed to by hand (0471).
 */

import type { ServiceContext } from "./context.ts";
import type { AgentState, PaneInfo } from "./herdr.ts";
import type { SessionStats } from "./sessions.ts";
import { isIdle, type Phase, planOf, setPhase, settled, show, subject, type Thread } from "./thread.ts";
import { LESSON_OFFER, PLAN_REVIEW_COMMAND, quoted, REPORT_STEER } from "./wording.ts";
import { gitOut } from "./worktrees.ts";

/**
 * How long a plan-mode turn's end waits for the session to show a proposed plan: omp writes the `xd://propose`
 * result through a buffered writer, so herdr's idle event can come before it is in the file.
 */
export const PLAN_SETTLE_MS = 2_000;
/** How often a plan-mode turn's end reads the session again meanwhile. */
const PLAN_POLL_MS = 100;
/** After a line goes into the boss pane, its idle pane state is herdr's lag, not the end of the turn it started. */
const BOSS_PROMPT_GRACE_MS = 10_000;

/** omp's Plan Review on a pane's screen (omp 18.12's picker: "Approve and keep context", "Refine plan"; 0494). */
const PLAN_REVIEW_SCREEN = /Approve and keep context|Refine plan/;

/**
 * Any other omp picker on a pane's screen (0494): a question's options end with "Other (type your own)", and every
 * picker's footer reads "… navigate … select … cancel".
 */
const PICKER_SCREEN = /Other \(type your own\)|\bnavigate\b.*\bcancel\b/;

/**
 * Where a state change came from: herdr's own event, omp's host registry (both pushed: the refresh tick's reconcile
 * never overrides them), or a read of herdr's pane list.
 */
export type StateSource = "herdr" | "host" | "reconcile";

/** What a run's end does in the phase it ended in. */
type TurnEnd = (thread: Thread, stats: SessionStats | undefined) => Promise<void>;

export class ThreadLifecycle {
	/** Each pane's state changes, handled one after another in the order herdr sent them (CR-09). */
	private readonly stateChains = new Map<string, Promise<void>>();

	/**
	 * A run's end (working → idle) by the phase it ended in (0394, 0400, 0426): the review turn's posts "review ready"
	 * and asks for the handoff; the handoff turn's closes the task; the finish turn's checks the PR it opened; a task's
	 * is a task end, a plan or a turn (`taskRunEnded`).
	 */
	private readonly onTurnEnd: Record<Phase, TurnEnd> = {
		review: (thread) => this.c.review.ready(thread),
		finish: (thread) => this.c.finisher.prOpened(thread),
		handoff: async (thread, stats) => {
			const head = await gitOut(thread.path, ["rev-parse", "HEAD"]);
			setPhase(thread, "task", "its handoff turn ended");
			// `handoff()` noted HEAD after the review's commits. When it moved since, this run was the next task's (the
			// handoff wrote no entry, CO-4): its end is a task end, not absorbed.
			if (head !== thread.head) await this.taskRunEnded(thread, stats, head);
		},
		task: async (thread, stats) => this.taskRunEnded(thread, stats, await gitOut(thread.path, ["rev-parse", "HEAD"])),
	};

	/**
	 * A step its omp was cut in by a restart (0582, SVC-6), once the new omp is idle: a cut review runs again on the
	 * same commits, a cut `/handoff` runs again, and a cut PR turn goes back to its task (it may have pushed half-way;
	 * he decides).
	 */
	private readonly onRelaunch: Record<Exclude<Phase, "task">, (thread: Thread) => Promise<void>> = {
		review: async (thread) => {
			setPhase(thread, "task", "its review turn was cut by a restart");
			await this.c.review.taskEnded(thread, thread.review?.running?.commits ?? "new commits");
		},
		handoff: (thread) => this.c.review.handoff(thread),
		finish: async (thread) => {
			setPhase(thread, "task", "its PR turn was cut by a restart");
			await this.c.events.thread(
				thread,
				"no PR: its omp started again mid-turn; everything stays, finish it again",
				"normal",
			);
		},
	};

	constructor(private readonly c: ServiceContext) {}

	/** Whether a state change of the pane is still being handled. */
	busy(paneId: string): boolean {
		return this.stateChains.has(paneId);
	}

	/** Waits for every state change still being handled (`stop`). */
	async settled(): Promise<void> {
		await Promise.allSettled([...this.stateChains.values()]);
	}

	/** A pane's state changed: handled after the pane's earlier changes (CR-09). */
	stateChanged(paneId: string, state: AgentState, source: StateSource): void {
		if (source !== "reconcile") {
			const now = Date.now();
			if (this.c.boss?.paneId === paneId) this.c.boss.eventAt = now;
			const thread = this.c.threads.get(paneId);
			if (thread) thread.eventAt = now;
		}
		const chain = (this.stateChains.get(paneId) ?? Promise.resolve())
			.then(() => this.onState(paneId, state))
			.catch((e) => this.c.warn(e));
		this.stateChains.set(paneId, chain);
		void chain.then(() => {
			if (this.stateChains.get(paneId) === chain) this.stateChains.delete(paneId);
		});
	}

	/**
	 * herdr pushed a pane's agent state: a thread whose omp host answers keeps omp's own state (0547, 0561); herdr
	 * saying no agent runs there any more counts (omp's entry goes with its process), and so does every boss change.
	 */
	herdrState(paneId: string, state: AgentState): void {
		const thread = this.c.threads.get(paneId);
		if (thread?.host !== undefined) {
			if (state !== "unknown") return;
			thread.host = undefined;
		}
		this.stateChanged(paneId, state, "herdr");
	}

	/**
	 * herdr's pane list read at `readAt` against what its events said (CR-09): a pane whose state differs, with no event
	 * since that read, changes as if herdr had pushed it. The boss pane just given a line is left alone a moment:
	 * herdr shows it idle until omp starts on the line. A handoff that ended without herdr or omp's host ever reporting
	 * it as a turn (F-01) ends here, for a thread whose host answers too: its session file has the handoff's entry.
	 */
	async reconcile(byId: Map<string, PaneInfo>, readAt: number): Promise<void> {
		const boss = this.c.boss;
		const bossPane = boss && byId.get(boss.paneId);
		if (
			boss &&
			bossPane &&
			settled(bossPane.agentState) !== settled(boss.state) &&
			(boss.eventAt ?? 0) < readAt &&
			(boss.promptedAt ?? 0) < readAt - BOSS_PROMPT_GRACE_MS
		) {
			this.stateChanged(boss.paneId, bossPane.agentState, "reconcile");
		}
		for (const thread of this.c.threads.all()) {
			const pane = byId.get(thread.paneId);
			if (!pane || (thread.eventAt ?? 0) >= readAt) continue;
			// omp's own state, while its host answers, isn't overridden by herdr's (0547).
			if (thread.host === undefined && settled(pane.agentState) !== settled(thread.state)) {
				this.stateChanged(thread.paneId, pane.agentState, "reconcile");
			} else if (thread.phase === "handoff" && settled(thread.state) === "idle" && this.handedOff(thread)) {
				await this.nextTask(thread);
			}
		}
	}

	/** Whether the `/handoff` asked for at `thread.handoffAt` is in the thread's session (F-01). */
	handedOff(thread: Thread): boolean {
		const at = thread.sessionPath ? this.c.sessions.read(thread.sessionPath).handoffAt : null;
		return at !== null && at >= (thread.handoffAt ?? 0);
	}

	/** The handoff is done (F-01): what runs from here is the next task, whose commits count from HEAD now. */
	private async nextTask(thread: Thread): Promise<void> {
		setPhase(thread, "task", "its handoff is written");
		thread.head = await gitOut(thread.path, ["rev-parse", "HEAD"]);
	}

	/** A watched pane's agent state changed (in order per pane, `stateChanged`). */
	private async onState(paneId: string, state: AgentState): Promise<void> {
		const { c } = this;
		if (c.stopping) return;
		if (paneId === c.boss?.paneId) {
			await c.events.bossChanged(state);
			return;
		}
		const thread = c.threads.get(paneId);
		if (!thread || thread.state === state) return;
		const was = thread.state;
		// The session file omp's integration reported (the question it waits on, its last line), known before the new
		// state is: a thread counted at work has its session, which Stop everything saves for `--restore` (0549).
		const sessionPath = (await c.herdr.panes()).find((p) => p.paneId === paneId)?.sessionPath;
		if (sessionPath) thread.sessionPath = c.sessions.moved(sessionPath);
		thread.state = state;
		// Before the start's read of today's sessions is done, this one is read in steps too, not in one long block.
		if (!c.warmed && thread.sessionPath) await c.sessions.warm([thread.sessionPath], () => c.stopping);
		// Read as the file is now, not as a read under a second ago saw it: a plan proposed since must count (B1).
		const stats = thread.sessionPath ? c.sessions.read(thread.sessionPath, true) : undefined;
		// herdr never reports `/handoff` as a turn (F-01): once its entry is in the session, this run is the next task,
		// whose commits count from here.
		if (state === "working" && thread.phase === "handoff" && this.handedOff(thread)) await this.nextTask(thread);
		if (stats) {
			// He sent its plan back from Plan Review (0471): the boss offers once to keep what he corrected.
			if (stats.planSentBack > (thread.plan?.sentBack ?? stats.planSentBack)) {
				await c.events.thread(thread, `he sent its plan back${LESSON_OFFER}`, "normal");
			}
			planOf(thread).sentBack = stats.planSentBack;
		}
		if (was === "blocked" || (thread.plan?.awaiting && !isIdle(state))) {
			c.day.threadUnblocked(paneId);
			if (thread.plan) thread.plan.awaiting = undefined;
		}
		this.noteApproval(thread, stats);
		if (state === "working") await this.working(thread);
		else if (state === "blocked") await this.blocked(thread, stats);
		else if (isIdle(state) && thread.cut) {
			// Its omp was started again mid-step and is idle now (0582).
			const cut = thread.cut;
			thread.cut = undefined;
			await this.onRelaunch[cut](thread);
		} else if (isIdle(state) && (was === "working" || was === "blocked")) {
			// A question he dismissed ends the turn too: the thread no longer awaits him (CO-3).
			await this.onTurnEnd[thread.phase](thread, stats);
		}
		c.status.writeTask(thread);
		await c.status.report();
	}

	/** A thread at work. */
	private async working(thread: Thread): Promise<void> {
		// Reviewed and not finished, and at work again on a task: he sent the reviewed work back (0471). When the
		// boss's own `thread_send` started it, it carried his word, which may not be a rejection: the boss knows.
		const result = thread.review?.result;
		if (thread.phase === "task" && result?.at !== undefined) {
			const relayed = (thread.sentAt ?? 0) >= result.at;
			result.at = undefined;
			await this.c.events.thread(
				thread,
				relayed
					? `at work again on its reviewed work, after your thread_send${LESSON_OFFER} if he rejected that work`
					: `he sent its reviewed work back${LESSON_OFFER}`,
				"normal",
			);
		}
		// The review and handoff turns keep the finished task's place in `today` (0394, 0400).
		if (thread.phase === "task") show(thread, "working");
		else if (thread.shown === "blocked") show(thread, "done");
	}

	/** A thread waits on his answer to its question. */
	private async blocked(thread: Thread, stats: SessionStats | undefined): Promise<void> {
		const { paneId } = thread;
		show(thread, "blocked");
		const question = stats?.question;
		this.c.day.threadBlocked(paneId, `${subject(thread)} — ${question ?? "awaiting you"}`, thread.project.id);
		await this.c.events.thread(
			thread,
			`awaiting you${question ? `: ${quoted(question)}` : ""}`,
			"urgent",
			() => this.c.threads.get(paneId) === thread && thread.state === "blocked",
		);
		this.c.events.send({
			type: "toast",
			reason: "blocked",
			level: "warning",
			text: `${subject(thread)} — awaiting you${question ? `: ${question}` : ""}`,
			paneId,
		});
	}

	/**
	 * A task's run ended at `head` (0400, 0413): a task end when its branch has new commits, a plan waiting for his
	 * approval in its pane, else a turn.
	 */
	private async taskRunEnded(thread: Thread, stats: SessionStats | undefined, head: string | undefined): Promise<void> {
		const { c } = this;
		if (head && head !== thread.head) {
			// A task's end without its report gets one steer asking for it; the next end goes to the review either way
			// (0539). HEAD stays noted where the task began, so that end still counts the task's commits. With the layer
			// off there is no report to ask for (0576).
			if (c.layer && c.options.askReport !== false && !thread.reportAsked && !c.review.reportIn(thread)) {
				thread.reportAsked = true;
				try {
					await c.herdr.prompt(thread.paneId, REPORT_STEER);
					return;
				} catch (error) {
					c.warn(error);
				}
			}
			thread.reportAsked = false;
			const from = thread.head;
			thread.head = head;
			show(thread, "done");
			const { said, subject } = await c.review.commitsSaid(thread, from, head);
			await c.review.taskEnded(thread, said, subject);
			return;
		}
		const plan = stats?.planMode && !stats.planAwaiting ? await this.settledPlan(thread) : stats?.planAwaiting;
		if (plan) {
			await this.planReady(thread, plan, { announce: true });
			return;
		}
		const lastLine = (thread.sessionPath ? c.sessions.read(thread.sessionPath).lastLine : stats?.lastLine) ?? null;
		show(thread, "idle");
		await c.events.thread(thread, `finished its turn${lastLine ? `: ${quoted(lastLine)}` : ""}`, "normal");
		// The check on idle runs beside the pane's state chain: a long check never holds its next state change.
		c.review.startIdleCheck(thread);
	}

	/**
	 * A plan-mode turn ended with no proposal in the session yet: the file is read again for a while, as omp's buffered
	 * writer may not have put the `xd://propose` result in it when herdr said idle (B1). The pane's next state change
	 * waits meanwhile (its chain). The plan's title, or undefined when none came or the service stops.
	 */
	private async settledPlan(thread: Thread): Promise<string | undefined> {
		const end = Date.now() + (this.c.options.planSettleMs ?? PLAN_SETTLE_MS);
		while (Date.now() < end && !this.c.stopping && thread.sessionPath) {
			await Bun.sleep(PLAN_POLL_MS);
			const plan = this.c.sessions.read(thread.sessionPath, true).planAwaiting;
			if (plan) return plan;
		}
		return undefined;
	}

	/**
	 * omp's Plan Review is open: herdr reports idle, but the thread waits on him like a blocked one (0405, 0407). Said
	 * as an urgent line, a toast and a sentence when `announce`; otherwise only counted (HUD, `today`, close question).
	 */
	private async planReady(thread: Thread, plan: string, { announce }: { announce: boolean }): Promise<void> {
		const own = planOf(thread);
		own.awaiting = plan;
		own.proposed = {
			title: plan,
			file: thread.sessionPath ? this.c.sessions.read(thread.sessionPath).planFile : null,
		};
		show(thread, "blocked");
		this.c.day.threadBlocked(thread.paneId, `${subject(thread)} — plan ready`, thread.project.id);
		if (!announce) return;
		// Dropped from the boss's queue once the plan no longer waits (beta day P8).
		await this.c.events.thread(thread, "plan ready, awaiting you", "urgent", () => this.planWaits(thread));
		this.c.events.send({
			type: "toast",
			reason: "blocked",
			level: "warning",
			text: `${subject(thread)} — plan ready, awaiting you`,
			paneId: thread.paneId,
		});
	}

	/**
	 * The task file's approved plan (0540): the last plan proposed, once he acted on it in omp's Plan Review and the
	 * thread's session (the new one, after "Approve and execute") is out of plan mode with no plan waiting; refining
	 * keeps it in plan mode until the next proposal.
	 */
	noteApproval(thread: Thread, stats: SessionStats | undefined): void {
		const own = thread.plan;
		const proposed = own?.proposed;
		if (!own || !proposed || own.awaiting || !thread.task || !stats || stats.planMode || stats.planAwaiting) return;
		thread.task.plan = { approvedAt: Date.now(), title: proposed.title, file: proposed.file ?? stats.planFile };
		own.proposed = undefined;
	}

	/** Whether the thread's plan still waits on him: it is open and its session shows the proposal unanswered. */
	private planWaits(thread: Thread): boolean {
		if (!this.c.threads.current(thread) || !thread.plan?.awaiting) return false;
		return !thread.sessionPath || this.c.sessions.read(thread.sessionPath, true).planAwaiting !== null;
	}

	/**
	 * Shows omp's Plan Review again in a reopened thread whose plan awaits him (0494): omp resumes the plan from its
	 * session but not the picker, so `/plan-review` is typed into its pane once its omp is idle. Typing while a picker
	 * is open would pick its default, "Approve and execute" (B1), so the pane's screen is read first: the Plan Review
	 * already open ends the wait; a question waits for a later look.
	 */
	async reopenPlanReview(thread: Thread): Promise<void> {
		if (!isIdle(thread.state)) return;
		if (this.busy(thread.paneId) || !thread.sessionPath || !this.planWaits(thread)) return;
		const paneId = thread.paneId;
		const screen = await this.c.herdr.screen(paneId);
		if (this.c.threads.get(paneId) !== thread || !thread.plan?.reopened) return;
		if (PLAN_REVIEW_SCREEN.test(screen)) {
			thread.plan.reopened = false;
			return;
		}
		if (PICKER_SCREEN.test(screen)) return;
		thread.plan.reopened = false;
		await this.c.herdr.prompt(paneId, PLAN_REVIEW_COMMAND);
	}

	/**
	 * Whether the thread waits on him now, before anything is typed into its pane (B1): a question (herdr's blocked) or
	 * a plan in omp's Plan Review, read from herdr's pane and the session as the file is now. A plan found here that the
	 * service missed counts from now on (`planFound`).
	 */
	async awaitingHim(thread: Thread): Promise<"question" | "plan" | undefined> {
		const pane = (await this.c.herdr.panes()).find((p) => p.paneId === thread.paneId);
		if (pane?.sessionPath) thread.sessionPath = this.c.sessions.moved(pane.sessionPath);
		if (thread.state === "blocked" || pane?.agentState === "blocked") return "question";
		if (thread.plan?.awaiting) return "plan";
		const plan = thread.sessionPath ? this.c.sessions.read(thread.sessionPath, true).planAwaiting : null;
		if (!plan) return undefined;
		await this.planFound(thread, plan, { announce: true });
		return "plan";
	}

	/**
	 * An idle thread's plan waiting in omp's Plan Review that no state change reported (B1, B3): its idle event came
	 * before omp wrote the proposal, or omp came back with `--continue` after a restart. It counts as awaiting him from
	 * now on, and the boss hears it when `announce`. Left to the pane's own state change while one is being handled.
	 */
	async planFound(thread: Thread, plan: string, how: { announce: boolean }): Promise<void> {
		if (thread.plan?.awaiting || thread.phase !== "task" || this.busy(thread.paneId)) return;
		if (!isIdle(thread.state)) return;
		await this.planReady(thread, plan, how);
	}

	/**
	 * A thread's pane or workspace was closed in herdr (0405): its worktree and branch stay on disk, a leftover (0456).
	 * What its omp left running ends (0490), found by the omp Jocasta remembered for the pane, never by folder. In quiet
	 * hours its line is held for the boss like any normal one (0298).
	 */
	async closed(thread: Thread): Promise<void> {
		if (thread.omp?.processes) void this.c.omps.end(thread.omp.processes);
		if (thread.shown === "blocked") this.c.day.threadUnblocked(thread.paneId);
		this.c.status.forget(thread);
		void this.c.day.refreshLeftovers();
		await this.c.events.thread(thread, "closed; its worktree and branch stay", "normal");
	}

	/**
	 * Commits that appeared on an idle thread's branch with no run of its own (0471): he committed by hand, so the boss
	 * offers once to save a lesson. Threads busy, in a review or handoff, waiting on a plan or with a state change still
	 * being handled are left alone; their HEAD is then taken as the thread's. Every HEAD is read at once, off the event
	 * loop (PERF-1).
	 */
	async handCommits(byId: Map<string, PaneInfo>): Promise<void> {
		const quiet = (thread: Thread) => {
			if (thread.phase !== "task" || thread.plan?.awaiting || thread.catchUp || !thread.head || !isIdle(thread.state)) {
				return false;
			}
			const pane = byId.get(thread.paneId);
			return pane !== undefined && isIdle(pane.agentState) && !this.busy(thread.paneId);
		};
		const candidates = this.c.threads
			.all()
			.filter(quiet)
			.map((thread) => ({ thread, was: thread.head }));
		const heads = await Promise.all(candidates.map(({ thread }) => gitOut(thread.path, ["rev-parse", "HEAD"])));
		for (const [i, { thread, was }] of candidates.entries()) {
			const head = heads[i];
			// Read again after the wait: a state change meanwhile takes the HEAD as its own.
			if (!head || head === was || thread.head !== was || !quiet(thread)) continue;
			const count = Number((await gitOut(thread.path, ["rev-list", "--count", `${was}..${head}`])) ?? 0);
			if (thread.head !== was) continue;
			thread.head = head;
			if (count === 0) continue; // reset or rebased away, not a commit
			await this.c.events.thread(
				thread,
				`he committed by hand on its branch (${count} commit${count === 1 ? "" : "s"})${LESSON_OFFER}`,
				"normal",
			);
		}
	}

	/**
	 * A thread whose pane herdr restored as a shell, or whose omp ended in it while it stayed open (0521): omp again, in
	 * the same tab (its hunk pane goes with the old one); one that was mid-turn when Jocasta stopped goes on with that
	 * turn (`--restore`, 0549). What the old omp left, the old omp too while it still runs, ends first (0490), so the
	 * new omp is the session's only writer. A review, `/handoff` or PR turn it was cut in is noted (`cut`, 0582): the
	 * new omp, once idle, runs it again or goes back to its task.
	 */
	async relaunch(thread: Thread): Promise<void> {
		const { c } = this;
		const old = thread.omp?.processes;
		if (old) await c.omps.end(old);
		const env = await c.panes.threadPaneEnv(thread.project, thread.path);
		const restore = c.launched.paused(thread.paneId);
		if (!restore && thread.phase !== "task") thread.cut = thread.phase;
		const paneId = await c.panes.respawn(thread.paneId, {
			cwd: thread.path,
			projectId: thread.project.id,
			env,
			...(restore ? { restore } : {}),
			thread,
		});
		c.threads.drop(thread.paneId);
		const wasBlocked = thread.shown === "blocked";
		if (wasBlocked) c.day.threadUnblocked(thread.paneId);
		thread.paneId = paneId;
		thread.state = "unknown";
		thread.host = undefined;
		// An interrupted run that doesn't resume with `--continue` (R3) is no longer at work until herdr says it works
		// again, and a question it asked is gone with it; a restored one (0549) says so itself. A plan in omp's Plan
		// Review still awaits him (0494 shows it again); its row moves to the new pane. Its "unknown" → "idle" is no turn
		// end, so nothing else would undo these.
		if (thread.shown === "working" || (wasBlocked && !thread.plan?.awaiting)) show(thread, "idle");
		else if (wasBlocked) c.day.threadBlocked(paneId, `${subject(thread)} — plan ready`, thread.project.id);
		if (thread.review) thread.review.pane = undefined;
		thread.omp = undefined;
		planOf(thread).reopened = true;
		c.threads.add(thread);
		await c.omps.remember(thread);
	}
}
