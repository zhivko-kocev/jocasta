/**
 * Starting a thread (0388, 0399, 0400, 0412, 0414, 0445, 0454, 0455, 0457, 0459, 0540, 0544): from the boss's
 * `thread_start`, his New thread form and `jocasta thread new`, one project's starts one after another.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { UserError } from "../../shared/errors.ts";
import { removeTreeAsync } from "../../shared/fsx.ts";
import { gitAsync } from "../../shared/git.ts";
import { type GuardRule, parseGuard, verdict } from "../../shared/guard.ts";
import { shippedConfigDir } from "../../shared/install.ts";
import { sameDir } from "../../shared/pathkey.ts";
import { waitFor } from "../../shared/poll.ts";
import type { Project } from "../../shared/projects.ts";
import { newPaneKey, PANE_KEY_ENV } from "./boss.ts";
import type { ServiceContext } from "./context.ts";
import type { PaneInfo } from "./herdr.ts";
import { ensureOverlay } from "./overlay.ts";
import { afterOverlay, THREAD_FLAGS } from "./pane-launch.ts";
import { newTask, taskDir } from "./task-file.ts";
import { subject, type Thread } from "./thread.ts";
import { branchSlug, COMMIT_LINE, type Door, LESSONS_LINE, START_PREFIX } from "./wording.ts";
import { copyWorktreeInclude, GIT_MS, gitOut, removeCheckout, threadPlace } from "./worktrees.ts";

/** How long `thread_start` waits for omp to come up in the new pane before sending the brief. */
export const AGENT_WAIT_MS = 20_000;
/** How long `thread_start` waits for the pane to start working on the brief before sending one Enter. */
export const SUBMIT_WAIT_MS = 5_000;
/** How often a start looks at the new pane meanwhile. */
const POLL_MS = 250;
/** How long taking back a failed start waits for its checkout's processes to let go (SVC-7). */
const UNDO_REMOVE_MS = 5_000;

/** `done_when` as given (0540): absent, or a list of command strings; anything else is a refusal named by the door. */
export function doneWhenOf(value: unknown, door: Door): string[] {
	if (value === undefined) return [];
	if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
		throw new UserError(`${START_PREFIX[door]}: done_when is a list of commands.`);
	}
	return value;
}

/** `heavy` as given (0590): absent, true or false; anything else is a refusal named by the door. */
export function heavyOf(value: unknown, door: Door): boolean {
	if (value === undefined) return false;
	if (typeof value !== "boolean") throw new UserError(`${START_PREFIX[door]}: heavy is true or false.`);
	return value;
}

/** The shipped guard's rules (0450, 0580), read once: every `done_when` is checked against them before it's stored. */
let guardRules: GuardRule[] | undefined;
function shippedGuard(): GuardRule[] {
	const file = join(shippedConfigDir(), "guard.yml");
	guardRules ??= parseGuard(readFileSync(file, "utf8"), file);
	return guardRules;
}

export class ThreadStarts {
	/** Each project's `thread_start` under way, so parallel starts make one project workspace (CO-5). */
	private readonly starts = new Map<string, Promise<unknown>>();
	/**
	 * The project of the last pane he focused in herdr that belongs to a project (0484, 0492): a focused pane in no
	 * project (the boss's, herdr's home shell) leaves it as it was; undefined before any project pane in this run.
	 */
	focusedProject: Project | undefined;
	/** The project the last thread start went into (0581, UX-15): the New thread form's third choice. */
	lastStartedIn: Project | undefined;
	private readonly agentWaitMs: number;
	private readonly submitWaitMs: number;

	constructor(private readonly c: ServiceContext) {
		this.agentWaitMs = c.options.agentWaitMs ?? AGENT_WAIT_MS;
		this.submitWaitMs = c.options.submitWaitMs ?? SUBMIT_WAIT_MS;
	}

	/**
	 * Remembers the project of the pane herdr has focused when it belongs to one (0484, 0492); the boss's pane and any
	 * other pane in no project leave it as it was. `thread_start` compares it.
	 */
	noteFocus(panes: PaneInfo[]): void {
		const pane = panes.find((p) => p.focused);
		if (!pane || pane.paneId === this.c.boss?.paneId) return;
		const project =
			this.c.threads.get(pane.paneId)?.project ?? (pane.cwd ? this.c.places.projectAt(pane.cwd) : undefined);
		if (project) this.focusedProject = project;
	}

	/** The project of a pane Jocasta started, from its recorded worktree or folder (0484); undefined for any other pane. */
	paneProject(paneId: string): Project | undefined {
		const thread = this.c.threads.get(paneId);
		if (thread) return thread.project;
		if (paneId === this.c.boss?.paneId) return undefined;
		const cwd = this.c.launched.cwdOf(paneId);
		return cwd ? this.c.places.projectAt(cwd) : undefined;
	}

	/** The project's workspace (label = its name, 0392), or undefined when herdr has none. */
	private async projectWorkspace(project: Project): Promise<string | undefined> {
		const workspaces = await this.c.herdr.workspaces();
		this.c.status.workspaces = workspaces;
		return workspaces.find((w) => w.label === project.name)?.id;
	}

	/**
	 * The project's workspace with its project pane, the project's omp overlay, a herdr worktree workspace on branch
	 * `feature/<title slug>` from the project's trunk in `<project>.wt/<title slug>` (`-2`, `-3`… when the branch or
	 * folder is taken; `thread-<YYYYMMDD>-<n>` for a title with no slug, 0581), its `.worktreeinclude` files copied in,
	 * running a fresh omp session with that overlay (so it opens in plan mode), the thread's env and pane key (0578) and
	 * its task file's prompt appended to its system prompt, then the brief, told to read the project's lessons, once omp
	 * is up. Each `done_when` goes through the shipped guard first (0580): deny refuses, prompt needs his Allow at the
	 * boss's door and is refused at the CLI's, which asks nothing (0544). A `heavy` thread (0590) is recorded so on its
	 * task and its omp loads `config/heavy.yml` after the overlay, at this launch and every later one. A start that fails
	 * once herdr made its worktree takes back what it made (SVC-7). Its texts name the door (0581); `done` is false for a
	 * start that is only half done (the brief not sent).
	 */
	async start(
		projectName: string,
		title: string,
		brief: string,
		door: Door = "boss",
		{ doneWhen = [], heavy = false }: { doneWhen?: string[]; heavy?: boolean } = {},
	): Promise<{ text: string; done: boolean }> {
		const { c } = this;
		const { herdr, panes } = c;
		const prefix = START_PREFIX[door];
		await c.places.sync();
		const project = c.places.find(projectName);
		if (!project.git)
			throw new UserError(`${prefix}: ${project.name} isn't a git repository; threads need a worktree.`);
		const checks = doneWhen.map((command) => command.trim());
		if (checks.some((command) => !command || /[\r\n]/.test(command))) {
			throw new UserError(`${prefix}: each done_when is one command on one line.`);
		}
		// Jocasta runs each done_when itself, outside omp's guard: the guard's own rules decide first (0580). omp matches
		// a command with its quotes removed, Jocasta's copy the raw text, so both forms are matched and the stricter
		// verdict counts: `git push --for"ce"` is still `--force`.
		const prompted: string[] = [];
		for (const command of checks) {
			const forms = [verdict(command, shippedGuard()), verdict(command.replace(/["']/g, ""), shippedGuard())];
			const { verdict: said, rule } =
				forms.find((form) => form.verdict === "deny") ??
				forms.find((form) => form.verdict === "prompt") ??
				(forms[0] as (typeof forms)[number]);
			if (said === "deny") {
				throw new UserError(
					`${prefix}: done_when \`${command}\` is denied by Jocasta's guard (\`${rule}\`); no thread was started.`,
				);
			}
			if (said === "prompt") prompted.push(command);
		}
		if (prompted.length > 0 && door === "cli") {
			throw new UserError(
				`${prefix}: done_when \`${prompted[0]}\` needs your Allow under Jocasta's guard, and jocasta thread new asks nothing; have the boss start it. No thread was started.`,
			);
		}
		// A boss's start in a project other than the one of the last project pane he focused, or with a done_when the
		// guard asks about, waits for his Allow (0484, 0492, 0580); one he started with his own key and form, or with
		// `jocasta thread new`, needs none: the thread waits at its plan for him (0544).
		this.noteFocus(await herdr.panes());
		const focused = this.focusedProject;
		const elsewhere = focused?.id !== project.id;
		if (door === "boss" && (elsewhere || prompted.length > 0)) {
			await c.allow.ask(
				{
					message: `Start the thread "${title.trim()}" in ${project.name}?`,
					detail: [
						...(elsewhere
							? [
									focused
										? `The last project pane you focused is in ${focused.name}, not ${project.name}.`
										: "You haven't focused a project's pane yet in this run.",
								]
							: []),
						...checks.map((command) => `It runs when the work is reviewed:  \`${command}\``),
						"",
						"The boss's brief:",
						brief.trim(),
					].join("\n"),
				},
				"no thread was started",
				door,
			);
		}
		const overlay = await ensureOverlay(project.id, panes.relayPort);
		const command = [...panes.paneOmp, "--config", overlay];
		const trunk = await c.places.trunk(project);
		// Its task file (0540), whose brief and done-when its omp appends to the system prompt at every launch.
		const task = newTask({
			projectId: project.id,
			project: project.name,
			title: title.trim(),
			brief: brief.trim(),
			doneWhen: checks,
			...(heavy ? { heavy: true as const } : {}),
			base: trunk ? ((await gitOut(project.mainPath, ["rev-parse", "--short", trunk])) ?? null) : null,
		});
		// A new thread never resumes a session saved for a reused folder (0455); a reopened one still does (0399). It
		// warns at $50 (0541); a heavy one loads `heavy.yml` after the overlay (0590), as `PaneLauncher.respawn` does.
		const fresh = [...command.filter((arg) => arg !== "--continue"), ...afterOverlay(task), ...THREAD_FLAGS];
		// The project pane and the thread pane load the same overlays; the worktree is trunk's checkout of the project,
		// so its own `.omp/config.yml` is the main checkout's (0561). Asked before anything is made.
		const mainEnv = await panes.withMise(project.id, project.mainPath, panes.paneEnv);
		await panes.relayChecked(command, project.mainPath, mainEnv);
		const key = newPaneKey();
		let place: { branch: string; path: string } | undefined;
		// One project's starts go one after another (CO-5): omp runs a turn's tool calls at once, and two that both
		// find no project workspace would make two.
		const step = (this.starts.get(project.id) ?? Promise.resolve())
			.catch(() => {})
			.then(async () => {
				let projectWs = await this.projectWorkspace(project);
				if (!projectWs) {
					const projectKey = newPaneKey();
					const env = { ...mainEnv, [PANE_KEY_ENV]: projectKey };
					const made = await herdr.ensureWorkspace(project.name, project.mainPath, command, env);
					const pane = (await herdr.panes()).find((p) => p.workspaceId === made);
					if (pane) c.launched.add(pane.paneId, project.mainPath, projectKey);
					projectWs = made;
				}
				// The branch and folder are picked here, after the project's earlier starts made theirs.
				const picked = threadPlace(project.mainPath, branchSlug(title));
				place = picked;
				// Its HEAD is read before omp starts in the pane, so no state omp reports comes before the thread is known.
				let head: string | undefined;
				const created = await herdr.createWorktree(projectWs, {
					repo: project.mainPath,
					branch: picked.branch,
					...(trunk ? { base: trunk } : {}),
					path: picked.path,
					title: title.trim(),
					// Written only now, once the relay check passed and nothing refused the start.
					command: panes.taskPrompted(fresh, task),
					prepare: async (path) => {
						await copyWorktreeInclude(project.mainPath, path);
						head = await gitOut(path, ["rev-parse", "HEAD"]);
						return { ...(await panes.threadPaneEnv(project, path)), [PANE_KEY_ENV]: key };
					},
				});
				return { ...created, branch: picked.branch, head };
			});
		this.starts.set(project.id, step);
		let created: Awaited<typeof step>;
		try {
			created = await step.finally(() => {
				if (this.starts.get(project.id) === step) this.starts.delete(project.id);
			});
		} catch (error) {
			// No thread: its launch prompt goes with it, and what herdr made for it so far.
			await removeTreeAsync(taskDir(task));
			if (place) await this.undo(project, place);
			throw error;
		}
		const { branch } = created;
		const thread: Thread = {
			paneId: created.paneId,
			workspaceId: created.workspaceId,
			project,
			title: title.trim(),
			branch,
			path: created.path,
			state: "unknown",
			shown: "working",
			changedAt: Date.now(),
			head: created.head,
			phase: "task",
			plan: { sentBack: 0 },
			spendSaid: false,
			task,
		};
		task.path = created.path;
		task.branch = branch;
		c.threads.add(thread);
		c.launched.add(thread.paneId, thread.path, key);
		this.lastStartedIn = project;
		c.status.writeTask(thread);
		await c.omps.remember(thread);
		const started =
			door === "boss" ? `Started ${subject(thread)} on branch ${branch}` : `Started ${subject(thread)} on ${branch}`;
		// The commit step is in `prompt.md`'s When you finish (0577); with the layer off there is none, so it ends the brief.
		const said = `${brief.trimEnd()}\n\n${LESSONS_LINE}${c.layer ? "" : `\n${COMMIT_LINE}`}`;
		if (!(await this.agentUp(thread.paneId))) {
			const wait = Math.round(this.agentWaitMs / 1000);
			return {
				text: `${started}, but omp didn't come up in its pane within ${wait} s; the brief wasn't sent.`,
				done: false,
			};
		}
		try {
			await herdr.prompt(thread.paneId, said);
		} catch (error) {
			return { text: `${started}, but the brief couldn't be sent: ${(error as Error).message}`, done: false };
		}
		// The paste can land while omp's input drops the Enter that follows it: if the pane doesn't start working on the
		// brief, one Enter submits it (as herdr's own `agent prompt` retries a stalled prompt). Never a second one.
		if (!(await this.startedWorking(thread.paneId))) {
			try {
				await herdr.submit(thread.paneId);
			} catch (error) {
				return {
					text: `${started}, but the brief may still sit unsent in its pane: ${(error as Error).message}`,
					done: false,
				};
			}
		}
		// A heavy thread says so (0590): it runs on his slow role, his strongest model.
		const slow = task.heavy ? ` Heavy: it runs on ${door === "boss" ? "his" : "your"} slow role.` : "";
		return {
			text:
				door === "boss"
					? `${started}. It opens in plan mode and waits for his approval of its plan in its pane.${slow}`
					: `${started}: it plans first; approve its plan in its pane.${slow}`,
			done: true,
		};
	}

	/**
	 * Takes back what a failed start made (SVC-7): the workspace herdr opened in its worktree, the worktree and the
	 * branch, which `threadPlace` picked as one no branch held. Each failure is logged; nothing of his is touched.
	 */
	private async undo(project: Project, place: { branch: string; path: string }): Promise<void> {
		const { herdr } = this.c;
		try {
			const panes = await herdr.panes();
			const workspaces = new Set(panes.filter((p) => p.cwd && sameDir(p.cwd, place.path)).map((p) => p.workspaceId));
			for (const workspaceId of workspaces) await herdr.closeWorkspace(workspaceId);
			const failed = existsSync(place.path)
				? await removeCheckout(project.mainPath, place.path, UNDO_REMOVE_MS)
				: undefined;
			if (failed) throw new Error(`${place.path} couldn't be removed (${failed})`);
			if (await gitOut(project.mainPath, ["rev-parse", "--verify", "-q", `refs/heads/${place.branch}`])) {
				await gitAsync(project.mainPath, ["branch", "-D", place.branch], GIT_MS);
			}
			this.c.numbers.forget(place.path);
		} catch (error) {
			this.c.warn(new Error(`a failed start in ${project.name} left ${place.path}: ${(error as Error).message}`));
		}
	}

	/** Waits up to `submitWaitMs` for the pane to leave idle (working, or already blocked on a question). */
	private startedWorking(paneId: string): Promise<boolean> {
		return waitFor(
			async () => {
				const thread = this.c.threads.get(paneId);
				const pane = (await this.c.herdr.panes()).find((p) => p.paneId === paneId);
				const state = pane?.agentState ?? thread?.state;
				return state === "working" || state === "blocked" || thread?.state === "working";
			},
			{ timeoutMs: this.submitWaitMs, intervalMs: POLL_MS },
		);
	}

	/**
	 * Waits until herdr sees an agent in the pane (its state isn't `unknown`), at most `agentWaitMs`. A thread takes its
	 * first state from herdr's pane list here: an omp that reported it as herdr made the pane did so before the thread
	 * was registered, so its event found no thread (0456).
	 */
	private agentUp(paneId: string): Promise<boolean> {
		return waitFor(
			async () => {
				const pane = (await this.c.herdr.panes()).find((p) => p.paneId === paneId);
				const thread = this.c.threads.get(paneId);
				if (pane?.sessionPath && thread) thread.sessionPath = pane.sessionPath;
				if (!pane || pane.agentState === "unknown") return false;
				if (thread?.state === "unknown" && !this.c.lifecycle.busy(paneId)) {
					this.c.lifecycle.stateChanged(paneId, pane.agentState, "reconcile");
				}
				return true;
			},
			{ timeoutMs: this.agentWaitMs, intervalMs: POLL_MS },
		);
	}
}
