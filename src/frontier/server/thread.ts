/**
 * A thread as the service knows it (0412): a herdr worktree workspace grouped under its project, with its step in the
 * task cycle, its last review and its plan; and the registry of the open ones, by pane.
 */
import { UserError } from "../../shared/errors.ts";
import { log } from "../../shared/log.ts";
import type { OmpProcesses, ProcessId } from "../../shared/proctree.ts";
import type { Project } from "../../shared/projects.ts";
import type { CheckSpec } from "./checks.ts";
import type { DayThread } from "./day.ts";
import type { AgentState } from "./herdr.ts";
import type { ReviewProof } from "./hunk.ts";
import type { TestsResult } from "./sessions.ts";
import type { CheckExit, TaskRecord } from "./task-file.ts";

/**
 * After a task end (0394, 0400): the review turn runs, then the `/handoff` turn; neither run's end is a task end.
 * `finish`: the thread pushes and opens its PR (0426).
 */
export type Phase = "task" | "review" | "handoff" | "finish";

/**
 * The moves between phases (SVC-15): a task's review, its handoff and Finish's PR turn, each back to the task. A task
 * with no review to run goes straight to its handoff; Finish takes a thread whose handoff is written (or held past
 * its time) to a new review (main moved) or its PR turn.
 */
const MOVES: Record<Phase, readonly Phase[]> = {
	task: ["review", "handoff", "finish"],
	review: ["handoff", "task"],
	handoff: ["task", "review", "finish"],
	finish: ["task"],
};

/** The last review's outcome (0425, 0453). */
export interface ReviewResult {
	/** How many notes it left in hunk; undefined when hunk couldn't count them. */
	notes?: number;
	/** How many of them the reviewer marked P0 or P1 (0453). */
	blocking?: number;
	/** Its checked test result (0425, 0453). */
	tests: TestsResult;
	/** Its diff size and test edits (0453). */
	proof?: ReviewProof;
	/** Its checks with their exit codes: the project's check, then each `done_when` command (0540). */
	checks: CheckExit[];
	/** When it became ready; cleared once the thread works again (0456 "Awaiting you"). */
	at?: number;
}

/** A thread's review step (0406): the hunk pane beside it, the review running now and the last one's outcome. */
export interface ThreadReview {
	/** The hunk pane beside it (0406), once a task ended. */
	pane?: string;
	/**
	 * The review running now: HEAD, the time it was asked for, what it covers and its range, the checks the runner ran
	 * and why the project's own couldn't run (0559).
	 */
	running?: {
		head: string | undefined;
		at: number;
		commits: string;
		range: string;
		checks?: CheckSpec[];
		skipped?: string;
	};
	/** The last review's outcome, once it is ready; none while a new one runs. */
	result?: ReviewResult;
}

/** A thread's plan in omp's Plan Review (0405, 0471, 0494, 0540). */
export interface ThreadPlan {
	/** The title of a plan waiting for his approval in omp's Plan Review (herdr reports the pane idle meanwhile). */
	awaiting?: string;
	/**
	 * The last plan omp proposed, with its file, until the session shows it approved (he acted on it in omp's Plan
	 * Review, and the session is out of plan mode): then the task file records it (0540). "Approve and execute" goes on
	 * in a new session, which has no plan of its own.
	 */
	proposed?: { title: string; file: string | null };
	/**
	 * Its omp was reopened (found in herdr, or started again in a pane herdr restored): omp shows no Plan Review for a
	 * plan resumed from its session, so `/plan-review` is typed once to show it again (0494). Cleared once typed, or once
	 * the picker is seen already open.
	 */
	reopened?: boolean;
	/** How often he had sent its plan back when last looked at (0471). */
	sentBack?: number;
}

/** A thread the service knows: a herdr worktree workspace grouped under its project (0412). */
export interface Thread {
	paneId: string;
	workspaceId: string;
	project: Project;
	title: string;
	branch: string;
	/** The worktree. */
	path: string;
	/**
	 * herdr's state of the pane, or omp's own while its host answers (`host`); what `today` shows is `shown`.
	 */
	state: AgentState;
	/**
	 * omp's own state from its host registry entry (0547, 0561): working (a turn runs), blocked (a dialog of omp's
	 * waits) or idle; undefined while its omp has no entry that answers (unknown, never idle: herdr's events and the
	 * session file decide then, as before).
	 */
	host?: AgentState;
	/** What `today` shows: working, waiting on him, done with a task, or idle. */
	shown: DayThread["state"];
	changedAt: number;
	/** HEAD at the last task end (or start): new commits since mean the task ended (0400). */
	head: string | undefined;
	/** Its step in the task cycle; changed only through `setPhase`. */
	phase: Phase;
	/**
	 * The step its omp was cut in by a restart (0582, SVC-6): once the new omp is idle, a cut review or `/handoff` runs
	 * again, and a cut PR turn goes back to `task`.
	 */
	cut?: Exclude<Phase, "task">;
	/** Whether the boss heard its spend pass $50 (0541, PRM-11); undefined until its ledger is first read. */
	spendSaid?: boolean;
	/** Its review step (0406, 0453). */
	review?: ThreadReview;
	/** When the boss last typed into its pane with `thread_send`: a run after review it started is his relayed word. */
	sentAt?: number;
	/** `thread_finish` merged main into the branch: the review now running decides the merge (0454). */
	mergeAfterReview?: boolean;
	/** Found in herdr rather than started here: once omp is idle, unreviewed commits get their review (0394). */
	catchUp?: boolean;
	/** Its plan in omp's Plan Review (0405, 0494, 0540). */
	plan?: ThreadPlan;
	sessionPath?: string;
	/** When `/handoff` was sent (0400): a handoff entry in the session at or after it means the handoff is done. */
	handoffAt?: number;
	/** When herdr last pushed this pane's state (the refresh tick's reconcile never overrides a newer one). */
	eventAt?: number;
	/**
	 * Its pane's omp as read for the pane's terminal `terminal` (0490), in a pane Jocasta started: `shim` the process
	 * herdr started for the pane, `processes` it and what ran under it. Read again when the terminal changes, and on
	 * each tick until the shim has a child (the real omp). What is left of it ends when the pane goes in herdr or herdr
	 * restores it as a shell, and when Jocasta stops everything.
	 */
	omp?: { terminal: string | undefined; shim: ProcessId | undefined; processes: OmpProcesses | undefined };
	/** Its task file's record (0540): none for a thread found in herdr that Jocasta has no record of. */
	task?: TaskRecord;
	/** This task's end already steered it once to hand in its report (0539). */
	reportAsked?: boolean;
	/** The tree its last check on idle ran on (0513, 0542): an unchanged tree isn't checked again. */
	idleTree?: string;
}

/** An agent state at rest: herdr's `done` is idle, not yet looked at. */
export function isIdle(state: AgentState): boolean {
	return state === "idle" || state === "done";
}

/** A state with herdr's `done` read as idle, for comparing two readings. */
export function settled(state: AgentState): AgentState {
	return state === "done" ? "idle" : state;
}

/** A thread as events, toasts and replies name it (UX-21): `<project>/<title>`. */
export function subject(thread: Thread): string {
	return `${thread.project.name}/${thread.title}`;
}

/** A thread's state as `thread_status` says it. */
export function threadState(t: Thread): string {
	// herdr's `done` is "idle, not yet looked at", not a finished task (as in `bossState`); a finished task is `shown`.
	if (t.shown === "done") return "done";
	if (t.plan?.awaiting) return "plan ready";
	if (t.state === "blocked") return "awaiting you";
	return t.state === "done" ? "idle" : t.state;
}

/** The thread's plan record, made when it has none yet. */
export function planOf(thread: Thread): ThreadPlan {
	if (!thread.plan) thread.plan = {};
	return thread.plan;
}

/**
 * Moves the thread to phase `to` (SVC-15): a move `MOVES` holds is made and logged with `why`; any other is refused
 * and logged as a warning. Staying in the phase it is in is no move. Whether the thread is in `to` now.
 */
export function setPhase(thread: Thread, to: Phase, why: string): boolean {
	const from = thread.phase;
	if (from === to) return true;
	if (!MOVES[from].includes(to)) {
		log("warn", "phase", `${subject(thread)}: ${from} → ${to} refused (${why})`);
		return false;
	}
	thread.phase = to;
	log("info", "phase", `${subject(thread)}: ${from} → ${to} (${why})`);
	return true;
}

/** Sets what `today` shows for the thread, and when it changed. */
export function show(thread: Thread, shown: DayThread["state"]): void {
	thread.shown = shown;
	thread.changedAt = Date.now();
}

/** The open threads by pane id. */
export class ThreadRegistry {
	private readonly byPane = new Map<string, Thread>();

	get(paneId: string): Thread | undefined {
		return this.byPane.get(paneId);
	}

	has(paneId: string): boolean {
		return this.byPane.has(paneId);
	}

	/** Registers `thread` under its pane. */
	add(thread: Thread): void {
		this.byPane.set(thread.paneId, thread);
	}

	drop(paneId: string): void {
		this.byPane.delete(paneId);
	}

	/** Whether `thread` is the one open in its pane now: one closed, finished or relaunched meanwhile isn't. */
	current(thread: Thread): boolean {
		return this.byPane.get(thread.paneId) === thread;
	}

	/** Whether `thread` is still open, in whatever pane it runs now. */
	includes(thread: Thread): boolean {
		return this.all().includes(thread);
	}

	/** The open threads, live: a thread added or dropped while iterating shows as a Map's iteration does. */
	values(): IterableIterator<Thread> {
		return this.byPane.values();
	}

	/** The open threads as they are now, a copy. */
	all(): Thread[] {
		return [...this.byPane.values()];
	}

	/** The open threads with their pane ids, a copy. */
	entries(): [string, Thread][] {
		return [...this.byPane];
	}

	paneIds(): IterableIterator<string> {
		return this.byPane.keys();
	}

	/** The open threads of a project. */
	ofProject(projectId: string): Thread[] {
		return this.all().filter((t) => t.project.id === projectId);
	}

	/** A thread by pane id, title or branch; unknown ones are a refusal. */
	find(wanted: string): Thread {
		const key = wanted.trim().toLowerCase();
		const threads = this.all();
		const found =
			this.byPane.get(wanted) ??
			threads.find((t) => t.title.toLowerCase() === key) ??
			threads.find((t) => t.branch.toLowerCase() === key);
		if (!found) throw new UserError(`no thread ${wanted}.`);
		return found;
	}
}
