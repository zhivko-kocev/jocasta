/**
 * The weekly big-picture review of a project (0470, 0583): everything merged into trunk in the past 7 days, read by
 * Jocasta's drift agent in a headless omp, one at a time per project (SVC-12).
 */
import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { shippedConfigDir } from "../../shared/install.ts";
import { projectDir } from "../../shared/paths.ts";
import { runProgram } from "../../shared/process.ts";
import { endProcesses, processesIn, processIdOf } from "../../shared/proctree.ts";
import type { Project } from "../../shared/projects.ts";
import type { ServiceContext } from "./context.ts";
import { ensureOverlay } from "./overlay.ts";
import { gitOut } from "./worktrees.ts";

/** How long the weekly big-picture review may run before it is stopped (0470). */
export const WEEKLY_REVIEW_MS = 30 * 60_000;
/** git's empty tree: the start of a repo younger than the review's week. */
const EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/** What a big-picture review covers: trunk a week ago → trunk now, with the count merged. */
interface Week {
	trunk: string;
	now: string;
	from: string;
	range: string;
	merged: number;
}

export class WeeklyReviews {
	/** Each project's big-picture review under way (SVC-12, 0577): one at a time per project. */
	private readonly runs = new Map<string, Promise<string | null>>();

	constructor(private readonly c: ServiceContext) {}

	/** Whether the project's big-picture review is running now. */
	running(projectId: string): boolean {
		return this.runs.has(projectId);
	}

	/**
	 * What a big-picture review would cover (0470): trunk a week ago → trunk now, with the count merged; null when trunk
	 * had fewer than two commits in the past 7 days (one change can't drift across changes, and the thread's own review
	 * saw it).
	 */
	async range(project: Project): Promise<Week | null> {
		const trunk = project.git ? await this.c.places.trunk(project) : undefined;
		if (!trunk) return null;
		const main = project.mainPath;
		const [now, count] = await Promise.all([
			gitOut(main, ["rev-parse", "--short", trunk]),
			gitOut(main, ["rev-list", "--count", "--since=7.days.ago", trunk]),
		]);
		const merged = Number(count ?? 0);
		if (!now || merged < 2) return null;
		// Trunk a week ago; a younger repo is reviewed from its start (git's empty tree).
		const from =
			(await gitOut(main, ["rev-list", "-1", "--abbrev-commit", "--before=7.days.ago", trunk])) || EMPTY_TREE;
		return { trunk, now, from, range: `${from}..${now}`, merged };
	}

	/**
	 * One read-only big-picture review of a project (0470), one at a time per project (SVC-12): a second caller gets the
	 * running one's result, so neither ends the other's processes. Resolves the result line, or null when there is no
	 * week to review (`range`).
	 */
	review(project: Project): Promise<string | null> {
		const running = this.runs.get(project.id);
		if (running) return running;
		const run = this.run(project).finally(() => this.runs.delete(project.id));
		this.runs.set(project.id, run);
		return run;
	}

	/**
	 * The big-picture review itself: everything merged into trunk in the past 7 days, by Jocasta's drift agent (0583,
	 * written beside the project's lessons skill, found from the run's folder) against the project's rules and lessons,
	 * looking for design drift across the changes. omp runs headless (`-p`) in `<data>/projects/<pid>/weekly` with the
	 * project's overlay, read-only tools and the main checkout added, so no session lands where the project pane
	 * resumes; `config/headless.yml` turns omp's async jobs off, so the subagent's result comes back before print mode
	 * exits.
	 */
	private async run(project: Project): Promise<string | null> {
		const week = await this.range(project);
		if (!week) return null;
		const { trunk, now, from, range, merged } = week;
		const panes = this.c.panes;
		const main = project.mainPath;
		const dir = join(projectDir(project.id), "weekly");
		mkdirSync(dir, { recursive: true });
		const overlay = await ensureOverlay(project.id, panes.relayPort);
		const rules = [join(main, "AGENTS.md"), join(main, "AGENTS.local.md")].filter((f) => existsSync(f));
		const slash = (path: string) => path.replace(/\\/g, "/");
		const prompt = [
			`Big-picture review of ${project.name}: everything merged into ${trunk} in the past 7 days,`,
			`\`git -C "${slash(main)}" diff ${range}\` (its commits: \`git -C "${slash(main)}" log --oneline ${from === EMPTY_TREE ? now : range}\`).`,
			"This review is read-only: change no file, commit nothing. Run the drift subagent on that range. In its task",
			`give it ${rules.length > 0 ? `the project's rules (${rules.map(slash).join(", ")}) and ` : ""}its lessons in skill://lessons,`,
			"and ask it for design drift across these changes: each may be reasonable alone while together they pull the design",
			"apart (a second way of doing what already has one, patterns that disagree, broken layering, a rule or lesson",
			"contradicted). Not line-level bugs. Tell it: report only material findings; prefer one strong finding over",
			"several weak ones. End your reply with one line on its own: `DRIFT: <one sentence: no drift found, or the",
			"strongest finding and where>`.",
		].join(" ");
		const argv = [
			...panes.paneOmp.filter((arg) => arg !== "--continue"),
			"--config",
			overlay,
			"--config",
			join(shippedConfigDir(), "headless.yml"),
			"-p",
			"--tools",
			// No `find`: its fuzzy search is off (0451, `find.enabled`), and omp refuses to start when asked for it.
			"read,grep,glob,task",
			"--add-dir",
			main,
			prompt,
		];
		const run = await runProgram(argv, {
			cwd: dir,
			// The service's own env with the panes' (omp needs its `PI_*`), then mise's.
			env: await panes.withMise(project.id, main, { ...process.env, ...panes.paneEnv } as Record<string, string>),
			timeoutMs: this.c.options.weeklyReviewMs ?? WEEKLY_REVIEW_MS,
		});
		const { stdout: out, stderr: err, code } = run;
		// Workers omp started (a memory embedding server, the tiny model) can outlive print mode and keep this folder
		// busy, as panes' did (0486); whatever still works in this run's own folder is this run's.
		await endProcesses(processesIn(dir).flatMap((found) => processIdOf(found.pid) ?? []));
		const drift = [...out.matchAll(/^[\s*_`>-]*DRIFT:\s*(.+?)[`*_\s]*$/gim)].at(-1)?.[1]?.trim();
		if (!drift) {
			const why =
				run.timedOut || run.signal
					? "it ran out of time"
					: `omp exited ${code}: ${(err || out).trim().split("\n").at(-1) ?? ""}`;
			throw new Error(`the big-picture review of ${range} gave no result (${why})`);
		}
		return `${range} (${merged} commit${merged === 1 ? "" : "s"}): ${drift}`;
	}
}
