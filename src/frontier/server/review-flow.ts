/**
 * The review step after a task (0394, 0406, 0425, 0453, 0540, 0542): hunk beside the thread on its diff, the task's
 * checks through the runner, the review turn, its outcome said once, then `/handoff` (0400); the check on idle (0513)
 * and the `check` tool.
 */
import { existsSync } from "node:fs";
import { join } from "node:path";
import { UserError } from "../../shared/errors.ts";
import { log } from "../../shared/log.ts";
import { type CheckRun, type CheckSpec, gateReply, shortTree } from "./checks.ts";
import type { ServiceContext } from "./context.ts";
import { hunkDiffArgs, listComments, proofSaid, type ReviewProof, reviewPrompt, reviewProof } from "./hunk.ts";
import { miseCheck, miseEnv, skippedSaid, trustCopies } from "./mise.ts";
import { reportLine } from "./report.ts";
import { checkExits, checkedTests, type SessionStats, type TestsResult } from "./sessions.ts";
import type { CheckExit } from "./task-file.ts";
import { isIdle, setPhase, show, subject, type Thread, type ThreadReview } from "./thread.ts";
import { HANDOFF_PROMPT, idleCheckText, quoted, reviewSaid, TESTS_SAID } from "./wording.ts";
import { gitOut } from "./worktrees.ts";

/** What a finished review turn found, before it is said (`ReviewFlow.ready`). */
interface ReviewFound {
	notes: number | undefined;
	blocking: number | undefined;
	tests: TestsResult;
	proof: ReviewProof | undefined;
	checks: CheckExit[];
	doneWhen: string[];
}

export class ReviewFlow {
	/** Checks on idle under way: the stop waits for them once their checks are ended. */
	private readonly idleChecks = new Set<Promise<void>>();

	constructor(private readonly c: ServiceContext) {}

	/**
	 * A thread found in herdr, now idle (0394): commits ahead of trunk that no review covered (the service was off
	 * when the task ended) get their review now, said as the commits since its last reviewed one (beta day P3).
	 */
	async catchUp(thread: Thread): Promise<void> {
		const [trunk, head] = await Promise.all([
			this.c.places.trunk(thread.project),
			gitOut(thread.path, ["rev-parse", "HEAD"]),
		]);
		if (!trunk || !head) return;
		const ahead = Number((await gitOut(thread.path, ["rev-list", "--count", `${trunk}..HEAD`])) ?? 0);
		const reviewed = this.c.reviewed.of(thread.path);
		if (ahead === 0 || reviewed === head) return;
		const from =
			reviewed && (await gitOut(thread.path, ["merge-base", "--is-ancestor", reviewed, head])) !== undefined
				? reviewed
				: undefined;
		thread.head = head;
		show(thread, "done");
		const { said, subject } = await this.commitsSaid(thread, from, head);
		await this.taskEnded(thread, said, subject);
	}

	/**
	 * What a task's commits from `from` (exclusive; undefined: since trunk) to `to` say in its review line (N-2, beta
	 * day P3): the thread's own commits, a merge and trunk's commits it brought in left out (`2 new commits`), with the
	 * subject of its last one; `<trunk> merged in` when only trunk came in.
	 */
	async commitsSaid(thread: Thread, from: string | undefined, to: string): Promise<{ said: string; subject?: string }> {
		const trunk = await this.c.places.trunk(thread.project);
		const range = [from ? `${from}..${to}` : to, ...(trunk ? [`^${trunk}`] : [])];
		const count = Number((await gitOut(thread.path, ["rev-list", "--count", "--no-merges", ...range])) ?? 0);
		if (count > 0) {
			const subject = await gitOut(thread.path, ["log", "-1", "--no-merges", "--format=%s", ...range]);
			return { said: `${count} new commit${count === 1 ? "" : "s"}`, ...(subject ? { subject } : {}) };
		}
		return { said: trunk && from ? `${trunk} merged in` : "new commits" };
	}

	/**
	 * A thread finished a task (0394, 0406, 0425) with `commits`, the last of them `subject`: hunk opens to the right of
	 * its pane at half width on `<trunk>...<branch>` (`--watch`, so a later task's commits show in the same pane), and
	 * the thread is asked to run the project's tests (its trusted mise `verify`/`test` task when it has one, 0473) and
	 * its `verify` skill, then omp's reviewer with the approved plan, the project's rules and lessons (0453, 0459,
	 * 0460), posting its findings into hunk and ending on its test command; the task is announced once, when its review
	 * is ready (UX-21). Without hunk, a trunk or a branch the review is skipped (and its commit counts as reviewed): the
	 * task is announced done and the handoff follows at once (0400).
	 */
	async taskEnded(thread: Thread, commits: string, subject?: string): Promise<void> {
		const { herdr, panes, sessions } = this.c;
		const hunk = panes.hunk();
		const [trunk, head] = await Promise.all([
			this.c.places.trunk(thread.project),
			gitOut(thread.path, ["rev-parse", "HEAD"]),
		]);
		if (!hunk || !trunk || !thread.branch) {
			this.c.warn(
				new Error(
					`no review for ${thread.title}: ${!hunk ? "hunk isn't installed" : !trunk ? "the project has no trunk branch" : "the thread has no branch"}`,
				),
			);
			this.c.reviewed.mark(thread.path, head);
			await this.announceDone(thread, commits);
			await this.handoff(thread);
			return;
		}
		const range = `${trunk}...${thread.branch}`;
		setPhase(thread, "review", "its task ended");
		// What this review covers, so a second task's line differs from the first's (N-2): `1 new commit «subject»`. The
		// last review's outcome goes with it.
		const running: NonNullable<ThreadReview["running"]> = {
			head,
			at: Date.now(),
			commits: `${commits}${subject ? ` ${quoted(subject)}` : ""}`,
			range,
		};
		const review: ThreadReview = { pane: thread.review?.pane, running };
		thread.review = review;
		try {
			const open = review.pane && (await herdr.panes()).some((p) => p.paneId === review.pane);
			if (!open) {
				// Started from the home folder first, the session daemon never holds the worktree open (F-02).
				await panes.ensureHunkDaemon(hunk);
				review.pane = await herdr.splitBeside(thread.paneId, {
					command: hunkDiffArgs(hunk, range),
					cwd: thread.path,
					env: panes.paneEnv,
					label: "review",
					direction: "right",
					ratio: 0.5,
				});
			}
			// After "Approve and execute" the session is a new one without the plan: the plan recorded at approval (0453, 0540).
			const plan =
				(thread.sessionPath ? sessions.read(thread.sessionPath).planFile : null) ?? thread.task?.plan?.file ?? null;
			const rules = [join(thread.path, "AGENTS.md"), join(thread.project.mainPath, "AGENTS.local.md")].filter((f) =>
				existsSync(f),
			);
			// Jocasta's runner checks the task's tree first (0542, 0546): the turn gets the failing gates, nothing on a pass.
			const { specs, skipped } = await this.checksFor(thread);
			const run = await this.runChecks(thread, specs);
			if (run) running.checks = specs;
			if (skipped) running.skipped = skipped;
			const record = thread.task;
			await herdr.prompt(
				thread.paneId,
				reviewPrompt(hunk, range, {
					...(plan ? { plan } : {}),
					rules,
					...(run ? { gates: gateReply(run), checked: specs.some((spec) => spec.mise === true) } : {}),
					...(record?.brief.trim() ? { brief: record.brief } : {}),
					report: record?.report && this.reportIn(thread) ? reportLine(record.report.value) : null,
				}),
			);
		} catch (error) {
			this.c.warn(error);
			setPhase(thread, "task", "its review couldn't start");
			if (thread.mergeAfterReview) {
				thread.mergeAfterReview = false;
				await this.c.events.thread(
					thread,
					`not merged: its new review couldn't start (${(error as Error).message})`,
					"normal",
				);
			}
			await this.announceDone(thread, commits);
			await this.handoff(thread);
		}
	}

	/** A task done with no review to follow: one line, one toast, one spoken sentence (0405, 0407). */
	private async announceDone(thread: Thread, commits: string): Promise<void> {
		await this.c.events.thread(thread, `done: ${commits}`, "normal");
		this.c.events.send({
			type: "toast",
			reason: "done",
			level: "info",
			text: `${subject(thread)} — done: ${commits}`,
			paneId: thread.paneId,
		});
	}

	/**
	 * The review turn ended: what it found is collected (`found`), the task is announced done and reviewed in one line,
	 * toast and sentence (0394, 0405, 0425, UX-21), the commit counts as reviewed, then `/handoff`. A review
	 * `thread_finish` asked for after main moved (0454) merges when it passed, else says why not.
	 */
	async ready(thread: Thread): Promise<void> {
		const found = await this.found(thread);
		const { notes, blocking, tests, proof, checks, doneWhen } = found;
		const reported = this.reportIn(thread);
		const running = thread.review?.running;
		const at = Date.now();
		thread.review = { pane: thread.review?.pane, result: { notes, blocking, tests, proof, checks, at } };
		show(thread, "done");
		this.c.reviewed.mark(thread.path, running?.head);
		const covered = running?.commits;
		// The project's check that couldn't run (0559): said beside the review's verdict, never a silent pass.
		const skipped = running?.skipped ? skippedSaid(running.skipped) : undefined;
		const doneWhenSaid =
			doneWhen.length > 0
				? `done-when ${checks
						.slice(checks.length - doneWhen.length)
						.map((c) => `\`${c.command}\` ${c.exit === null ? "not run" : `exit ${c.exit}`}`)
						.join(", ")}`
				: undefined;
		const said = [
			covered,
			proof ? proofSaid(proof) : undefined,
			reviewSaid(notes, tests, blocking),
			doneWhenSaid,
			skipped,
			reported ? undefined : "no report",
		]
			.filter(Boolean)
			.join(", ");
		const counted = `done and reviewed: ${said}`;
		if (thread.task) {
			thread.task.reviews.push({
				at,
				passed: this.failed(thread) === undefined,
				checks,
				said: [reviewSaid(notes, tests, blocking), skipped, reported ? undefined : "no report"]
					.filter(Boolean)
					.join(", "),
				// What a restart gives the thread back as its last review (`Refresher.discover`).
				...(notes !== undefined ? { notes } : {}),
				...(blocking !== undefined ? { blocking } : {}),
				tests,
				...(proof ? { proof } : {}),
			});
			this.c.status.writeTask(thread);
		}
		// A review `thread_finish` asked for after main moved (0454) is said once, with the merge's outcome (beta day P3).
		const merging = thread.mergeAfterReview === true;
		thread.mergeAfterReview = false;
		const failed = merging ? this.failed(thread) : undefined;
		if (!merging) await this.c.events.thread(thread, counted, "normal");
		else if (failed) {
			await this.c.events.thread(
				thread,
				`${counted}; not merged: its new review after main moved didn't pass (${failed})`,
				"normal",
			);
		}
		this.c.events.send({
			type: "toast",
			reason: "done",
			level: "info",
			text: `${subject(thread)} — ${counted.replace(/[«»]/g, "")}`,
			paneId: thread.paneId,
		});
		await this.c.status.report();
		if (merging && !failed) {
			await this.c.finisher.afterReview(thread, said);
			return;
		}
		await this.handoff(thread);
	}

	/**
	 * What the review turn found: hunk's notes since the review was asked for, counted with the blocking ones (a count
	 * hunk can't give is said, not hidden, F-03); the tests checked against the turn's bash results (0453); the diff's
	 * size and test edits from git; and the checks: the runner's on the reviewed tree (its stored result when the turn
	 * changed nothing), the project's check first, then each done_when (0453, 0540, 0542); without the project's check,
	 * the test command the turn named and its `TESTS:` line, as the session's bash runs show them.
	 */
	private async found(thread: Thread): Promise<ReviewFound> {
		const hunk = this.c.panes.hunk();
		const running = thread.review?.running;
		const since = running?.at ?? 0;
		let notes: number | undefined;
		let blocking: number | undefined;
		try {
			if (!hunk) throw new Error("hunk isn't installed");
			({ notes, blocking } = await listComments(hunk, thread.path, since, this.c.panes.hunkPort));
		} catch (error) {
			this.c.warn(new Error(`${thread.title}'s review notes weren't counted: ${(error as Error).message}`));
		}
		const stats = thread.sessionPath ? this.c.sessions.read(thread.sessionPath) : undefined;
		const range = running?.range;
		const proof = range ? await reviewProof(thread.path, range) : undefined;
		const doneWhen = thread.task?.doneWhen ?? [];
		const specs = running?.checks;
		const run = specs ? await this.runChecks(thread, specs) : undefined;
		const project = run?.results.find((result) => result.spec.mise === true);
		let tests: TestsResult;
		if (project) tests = project.gates.every((gate) => gate.pass) ? "pass" : "fail";
		else tests = stats ? checkedTests(stats, since) : "unknown";
		const named = stats?.tests && stats.tests.at >= since ? (stats.tests.command ?? undefined) : undefined;
		let checks: CheckExit[];
		if (run) {
			checks = [
				...(project ? [] : named ? checkExits(stats as SessionStats, [named], since) : []),
				...run.results.map((result) => ({ command: result.spec.command, exit: result.exit })),
			];
		} else if (stats) checks = checkExits(stats, [...(named ? [named] : []), ...doneWhen], since);
		else checks = [...(named ? [named] : []), ...doneWhen].map((command) => ({ command, exit: null }));
		return { notes, blocking, tests, proof, checks, doneWhen };
	}

	/**
	 * Why the thread's last review wasn't a pass (0453): its tests didn't pass by their run, or the reviewer marked
	 * notes blocking (P0/P1); undefined when it passed or no review result is known here.
	 */
	failed(thread: Thread): string | undefined {
		const result = thread.review?.result;
		if (result === undefined) return undefined;
		// A done_when command that didn't exit 0 in the review turn (0540): the task's own definition of done.
		const doneWhen = new Set(thread.task?.doneWhen ?? []);
		const unmet = result.checks.filter((c) => doneWhen.has(c.command) && c.exit !== 0);
		const why = [
			...(result.tests === "pass" || result.tests === "none" ? [] : [TESTS_SAID[result.tests]]),
			...(result.notes === undefined ? ["notes not counted"] : []),
			...(result.blocking ? [`${result.blocking} blocking note${result.blocking === 1 ? "" : "s"}`] : []),
			...unmet.map((c) => `done-when \`${c.command}\` ${c.exit === null ? "not run" : `exit ${c.exit}`}`),
		];
		return why.length > 0 ? why.join(", ") : undefined;
	}

	/** Whether the thread handed in its report for the task now ending: one after its last review (0539). */
	reportIn(thread: Thread): boolean {
		const report = thread.task?.report;
		return report !== null && report !== undefined && report.at > (thread.task?.reviews.at(-1)?.at ?? 0);
	}

	/**
	 * The task's checks (0453, 0473, 0540, 0542): the project's own (its trusted mise `verify`/`test` task, run as
	 * `mise run -c <task>` so a failing task doesn't stop the others, 0559) and the task's `done_when` commands, with why
	 * the project's own can't run when mise may not load its config (0559). The one place checks are decided: the
	 * review, the check on idle and the `check` tool all run these through the runner. The worktree's unchanged copies
	 * of trusted mise configs are trusted first (0596), so a copy a merge brought in from trunk is trusted again here.
	 */
	private async checksFor(thread: Thread): Promise<{ specs: CheckSpec[]; skipped?: string }> {
		const mise = this.c.options.mise;
		const env = this.c.panes.checkEnv();
		if (mise !== null) await trustCopies(thread.project.id, thread.project.mainPath, thread.path, env, mise);
		const project = mise === null ? undefined : await miseCheck(thread.project.id, thread.path, env, mise);
		const argv = (mise ?? ["mise"]).map((word) => (/^[\w./:=-]+$/.test(word) ? word : `"${word}"`));
		const doneWhen = thread.task?.doneWhen ?? [];
		const task = project && "task" in project ? project.task : undefined;
		return {
			specs: [
				...(task ? [{ name: task, command: `${argv.join(" ")} run -c ${task}`, mise: true }] : []),
				...doneWhen.map((command, i) => ({ name: i === 0 ? "done_when" : `done_when-${i + 1}`, command })),
			],
			...(project && "skipped" in project ? { skipped: project.skipped } : {}),
		};
	}

	/**
	 * Runs `specs` on the thread's worktree through the runner (0542, 0559), with the env its panes get (mise's
	 * included, Jocasta's own keys left out), on `tree` when the caller already keyed it (SRV-B-17); undefined without
	 * specs, outside git, or when the runner fails (logged).
	 */
	private async runChecks(thread: Thread, specs: CheckSpec[], tree?: string): Promise<CheckRun | undefined> {
		if (specs.length === 0) return undefined;
		const env = this.c.panes.checkEnv();
		const mise = this.c.options.mise;
		try {
			const own = mise === null ? {} : await miseEnv(thread.project.id, thread.path, env, mise);
			return await this.c.checks.run(thread.path, specs, { ...env, ...own }, tree);
		} catch (error) {
			this.c.warn(new Error(`${thread.title}'s checks didn't run: ${(error as Error).message}`));
			return undefined;
		}
	}

	/**
	 * Starts the check on idle beside the pane's state chain: a long check never holds its next state change. None with
	 * the layer off (0576).
	 */
	startIdleCheck(thread: Thread): void {
		if (!this.c.layer) return;
		const checking: Promise<void> = this.idleCheck(thread)
			.catch((error: unknown) => this.c.warn(error))
			.finally(() => this.idleChecks.delete(checking));
		this.idleChecks.add(checking);
	}

	/** Waits for the checks on idle still under way (`stop`, once the runner ended their checks). */
	async settled(): Promise<void> {
		await Promise.allSettled([...this.idleChecks]);
	}

	/**
	 * The check on idle (0513, v1 by 0542): a thread whose turn ended with edits not yet checked has its checks run on
	 * its tree; failing gates go into its pane under 0583's frame, a pass says nothing. A tree already checked, or one
	 * with no edits since the last commit, costs the tree key only: mise is asked for the checks after (SVC-10).
	 */
	private async idleCheck(thread: Thread): Promise<void> {
		if (thread.phase !== "task" || thread.plan?.awaiting) return;
		const tree = await this.c.checks.treeKey(thread.path);
		if (!tree || tree === thread.idleTree) return;
		// No edits since its last commit: nothing new to check.
		if (tree === (await gitOut(thread.path, ["rev-parse", "HEAD^{tree}"]))) return;
		const { specs } = await this.checksFor(thread);
		if (specs.length === 0) return;
		thread.idleTree = tree;
		const run = await this.runChecks(thread, specs, tree);
		if (!run || run.passed || this.c.stopping || !this.c.threads.current(thread)) return;
		// Only into a thread still idle on that tree: a turn since, or a plan or question waiting, leaves it alone.
		if (thread.phase !== "task" || thread.plan?.awaiting || !isIdle(thread.state)) return;
		if (this.c.lifecycle.busy(thread.paneId)) return;
		log("info", "checks", `${subject(thread)}: checks failed on tree ${shortTree(run.tree)}`);
		await this.c.herdr.prompt(thread.paneId, idleCheckText(gateReply(run)));
	}

	/**
	 * The `check` thread tool (0542, 0546): the task's checks on the thread's tree, as gates; one line on a pass. A
	 * project check that couldn't run is said (0559).
	 */
	async checkTool(paneId: string | undefined, unkeyed: string): Promise<string> {
		const thread = paneId ? this.c.threads.get(paneId) : undefined;
		if (!thread)
			throw new UserError(`check: only a thread's pane runs its checks, and this pane isn't a thread's${unkeyed}.`);
		const { specs, skipped } = await this.checksFor(thread);
		if (specs.length === 0) {
			return skipped
				? `No checks: ${skippedSaid(skipped)}, and the task has no done_when.`
				: "No checks: the project has no trusted mise verify or test task, and the task has no done_when.";
		}
		const run = await this.runChecks(thread, specs);
		if (!run) throw new UserError("check: the checks couldn't run here (see Jocasta's log).");
		const said = gateReply(run) || `checks: PASS (tree ${shortTree(run.tree)})`;
		return skipped ? `${said}\nNote: ${skippedSaid(skipped)}.` : said;
	}

	/**
	 * Asks the thread for `/handoff` (0400), so the task leaves one summary for the watcher. HEAD is noted first:
	 * whatever the review committed is not the next task's work, and a later move means a new task ran (CO-4).
	 */
	async handoff(thread: Thread): Promise<void> {
		setPhase(thread, "handoff", "it is asked for /handoff");
		thread.handoffAt = Date.now();
		thread.head = await gitOut(thread.path, ["rev-parse", "HEAD"]);
		try {
			await this.c.herdr.prompt(thread.paneId, HANDOFF_PROMPT);
		} catch (error) {
			setPhase(thread, "task", "it didn't take /handoff");
			this.c.warn(error);
		}
	}
}
