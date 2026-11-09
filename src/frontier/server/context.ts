/**
 * What the service's parts share (SVC-15): the catalog, the options, herdr, the day, the session index and each part,
 * built once by `JocastaService`. A part reaches another only through this, so the parts import each other's types
 * alone and the wiring stays in one place (`service.ts`).
 */
import type { Database } from "bun:sqlite";
import type { AllowGate } from "./allow.ts";
import type { LaunchedPanes } from "./boss.ts";
import type { CheckRunner } from "./checks.ts";
import type { Day } from "./day.ts";
import type { BossPane, Events } from "./events.ts";
import type { Finisher } from "./finish.ts";
import type { Herdr } from "./herdr.ts";
import type { OmpTracker } from "./omp-processes.ts";
import type { PaneLauncher } from "./pane-launch.ts";
import type { Places } from "./places.ts";
import type { ReviewFlow } from "./review-flow.ts";
import type { ServiceOptions } from "./service.ts";
import type { SessionIndex } from "./sessions.ts";
import type { StatusViews } from "./status.ts";
import type { ThreadRegistry } from "./thread.ts";
import type { ThreadLifecycle } from "./thread-lifecycle.ts";
import type { ThreadStarts } from "./thread-start.ts";
import type { WeeklyReviews } from "./weekly-review.ts";
import type { ReviewedCommits, ThreadNumbers } from "./worktrees.ts";

export interface ServiceContext {
	readonly catalog: Database;
	readonly options: ServiceOptions;
	readonly herdr: Herdr;
	readonly sessions: SessionIndex;
	readonly checks: CheckRunner;
	/** The panes Jocasta started omp in (`boss.ts` `LaunchedPanes`). */
	readonly launched: LaunchedPanes;
	readonly threads: ThreadRegistry;
	/** The layer's test switch (0576): off, thread panes get no layer parts, no report steer, no check on idle. */
	readonly layer: boolean;
	/** H2's `herdr` condition (0576): off, omp's host registry is never read; herdr's events and the tick decide. */
	readonly hosts: boolean;
	/** Tool, CLI and key calls under way (SVC-13), and background git reads: `stop` waits for them. */
	readonly calls: Set<Promise<unknown>>;
	/** Thread workspaces `thread_finish` removes or removed: neither found again nor reported closed (CR-11). */
	readonly finishing: Map<string, "removing" | "removed">;
	/** The service is stopping: nothing new starts, nothing more is said. */
	stopping: boolean;
	/** Today's sessions were read once (`SessionIndex.warm`): until then nothing reads a whole session on a tick. */
	warmed: boolean;
	/** The boss pane, once ensured. */
	boss: BossPane | undefined;
	day: Day;
	places: Places;
	events: Events;
	allow: AllowGate;
	panes: PaneLauncher;
	omps: OmpTracker;
	numbers: ThreadNumbers;
	reviewed: ReviewedCommits;
	review: ReviewFlow;
	finisher: Finisher;
	lifecycle: ThreadLifecycle;
	status: StatusViews;
	starts: ThreadStarts;
	weekly: WeeklyReviews;
	/** Into Jocasta's log (0431), which a source run also prints; nothing once the service is stopping. */
	warn(error: unknown): void;
}
