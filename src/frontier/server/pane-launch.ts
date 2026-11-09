/**
 * How the service starts omp in a pane (0399, 0404, 0457, 0521, 0540, 0578): the boss pane with this run's boss key,
 * a project or thread pane again with its overlay, env and pane key, each checked to host on loopback only (0561);
 * the panes' env with mise's (0473, 0477) and hunk's (F-02, F-03); and Jocasta's hunk session daemon.
 */
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { BOSS_KEY_ENV } from "../../shared/boss-key.ts";
import { writeAtomically } from "../../shared/fsx.ts";
import { shippedConfigDir } from "../../shared/install.ts";
import { LAYER_ENV } from "../../shared/layer.ts";
import { log } from "../../shared/log.ts";
import { pathKey, realPath } from "../../shared/pathkey.ts";
import { dataDir } from "../../shared/paths.ts";
import { endProcesses, type ProcessId, processStart } from "../../shared/proctree.ts";
import type { Project } from "../../shared/projects.ts";
import { withoutLeadingComment } from "../../shared/prompt.ts";
import { bossCommand, bossToolsMissing, ensureBossPane, newPaneKey, PANE_KEY_ENV } from "./boss.ts";
import type { ServiceContext } from "./context.ts";
import type { PaneInfo } from "./herdr.ts";
import { ensureHunkDaemon, findHunk, HUNK_PORT, hunkEnv } from "./hunk.ts";
import { miseEnv } from "./mise.ts";
import { ensureOverlay } from "./overlay.ts";
import { configFiles, isLoopbackRelay, notLoopback, ompRelayUrl, RELAY_PORT } from "./relay.ts";
import { launchPrompt, type TaskRecord } from "./task-file.ts";
import type { Thread } from "./thread.ts";
import { threadEnv } from "./worktrees.ts";

/** What every thread's omp starts with besides its pane's flags (0541, K10): a cost warning in its pane at $50. */
export const THREAD_FLAGS = ["--warn-cost", "50"];

/** The first wait before an omp that ended again in a Jocasta pane starts again; doubled with each ending (0521). */
const OMP_AGAIN_MS = 10_000;
/** The longest wait before an omp that keeps ending in a Jocasta pane starts again (0521). */
const OMP_AGAIN_MAX_MS = 5 * 60_000;
/** An omp that ran this long since Jocasta last started it again ends afresh: no wait (0521). */
const OMP_AGAIN_RESET_MS = 10 * 60_000;

/** mise's env without the keys it must not override in a pane (0484): herdr's, Jocasta's and omp's (`PI_*`). */
export function withoutJocastaKeys(env: Record<string, string>): Record<string, string> {
	return Object.fromEntries(Object.entries(env).filter(([key]) => !/^(HERDR|JOCASTA|PI)_/i.test(key)));
}

/**
 * The pane prompt `file` as omp should append it (0577, PRM-6): without its leading `<!-- … -->` comment (Ponytail's
 * license and file notes), as a copy in `<data>` written when it differs; `file` itself when it has no such comment or
 * can't be read.
 */
function strippedPrompt(file: string): string {
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch {
		return file;
	}
	const stripped = withoutLeadingComment(text).trimStart();
	if (stripped === text) return file;
	const copy = join(dataDir(), basename(file));
	try {
		if (!existsSync(copy) || readFileSync(copy, "utf8") !== stripped) writeAtomically(copy, stripped);
	} catch (error) {
		log("warn", "service", `${copy} wasn't written: ${(error as Error).message}`);
		return file;
	}
	return copy;
}

/** The wait before a pane's omp starts again after `count` endings or failures in a row (0521): 10 s doubling to 5 min. */
function againWait(count: number): number {
	return Math.min(OMP_AGAIN_MS * 2 ** (count - 1), OMP_AGAIN_MAX_MS);
}

export class PaneLauncher {
	readonly omp: string[];
	/** How every project and thread pane starts, before its overlay: omp resuming, with the appended prompt. */
	readonly paneOmp: string[];
	/** The panes' shipped prompt file (Ponytail), or null; a thread's own launch prompt starts with it (0540). */
	private readonly panePrompt: string | null;
	/** Jocasta's hunk port and no update notice, for every hunk the service starts and every thread pane (F-02, F-03). */
	readonly paneEnv: Record<string, string>;
	readonly hunkPort: number;
	/** The relay port the panes' overlays name (0561). */
	readonly relayPort: number;
	/** This run's boss key (0484): only the boss pane's environment gets it, and boss tools require it. */
	readonly bossKey: string;
	/** The boss pane started with this run's key; another boss pane runs omp started before this run. */
	bossKeyed: string | undefined;
	/** The boss's omp started without its `jocasta` tools (beta day P9), said once as a warning notification. */
	bossNotice: string | undefined;
	/** The hunk session daemon this service started (B4): Jocasta's close ends it. */
	private hunkDaemon: ProcessId | undefined;

	constructor(private readonly c: ServiceContext) {
		const options = c.options;
		this.omp = options.omp ?? ["omp"];
		this.bossKey = options.bossKey ?? randomBytes(32).toString("hex");
		const ponytail = join(shippedConfigDir(), "ponytail.md");
		const prompt = options.panePrompt !== undefined ? options.panePrompt : existsSync(ponytail) ? ponytail : null;
		this.panePrompt = prompt;
		// The guard (0450) rides on every pane, before the project's overlay; the appended prompt without its leading
		// comment (0577, PRM-6).
		this.paneOmp = [
			...this.omp,
			"--continue",
			...(prompt ? ["--append-system-prompt", strippedPrompt(prompt)] : []),
			"--config",
			join(shippedConfigDir(), "guard.yml"),
		];
		this.hunkPort = options.hunkPort ?? HUNK_PORT;
		this.relayPort = options.relayPort ?? RELAY_PORT;
		this.paneEnv = hunkEnv(this.hunkPort);
	}

	/**
	 * Workspace "Jocasta", first, with its boss pane in `<data>/home`; made again when it was closed (0404). At start,
	 * or when its omp ended in it (0521), (`adopt`) a boss pane with no omp in it runs omp, even one Jocasta has no
	 * record of starting. The boss pane's omp starts with this run's boss key in its environment (0484); one already
	 * running without it (started before this run, or by bare `jocasta`) starts again with it, resuming its session,
	 * once it isn't working or asking him.
	 */
	async ensureBoss({ adopt }: { adopt: boolean }): Promise<void> {
		const pane = await ensureBossPane(this.c.herdr, {
			command: this.c.options.bossCommand ?? bossCommand(this.omp),
			check: (command, cwd, env) => this.relayChecked(command, cwd, env),
			env: { [BOSS_KEY_ENV]: this.bossKey },
			launched: this.c.launched,
			adopt,
			stale: (p) => this.unkeyedBoss(p),
			started: (paneId) => {
				this.bossKeyed = paneId;
				const missing = bossToolsMissing();
				if (missing) {
					log("warn", "service", missing);
					this.bossNotice = missing;
				}
			},
		});
		const same = this.c.boss?.paneId === pane.paneId ? this.c.boss : undefined;
		this.c.boss = {
			...same,
			paneId: pane.paneId,
			workspaceId: pane.workspaceId,
			state: same?.state ?? pane.agentState,
			...(pane.sessionPath ? { sessionPath: pane.sessionPath } : {}),
		};
	}

	/** A boss pane whose omp started without this run's key and is idle now, so it can start again with it (0484). */
	unkeyedBoss(pane: PaneInfo): boolean {
		return (
			pane.paneId !== this.bossKeyed &&
			pane.agent === "omp" &&
			(pane.agentState === "idle" || pane.agentState === "done")
		);
	}

	/**
	 * A project or thread pane herdr restored as a shell, or whose omp ended in it (0521), runs omp again in its folder
	 * with the project's overlay (0399, 0400), resuming its session, and its env: a thread's own variables (0457) or,
	 * for a project pane, the hunk variables, both with mise's (0473, 0477); returns the new pane. With `restore` (a
	 * session file), omp goes on with the turn that session's last process left unfinished (0549). A thread's omp
	 * appends its task's brief and done-when again (0540). Each start gets its own pane key (0578), kept with the pane.
	 */
	async respawn(
		paneId: string,
		how: { cwd: string; projectId: string; env?: Record<string, string>; restore?: string; thread?: Thread },
	): Promise<string> {
		const { cwd, projectId, env, restore, thread } = how;
		const overlay = await ensureOverlay(projectId, this.relayPort);
		const pane = thread ? this.taskPrompted([...this.paneOmp, ...THREAD_FLAGS], thread.task) : this.paneOmp;
		const omp = restore ? [...pane.filter((arg) => arg !== "--continue"), "--restore", restore] : pane;
		const command = [...omp, "--config", overlay];
		const key = newPaneKey();
		const paneEnv = { ...(env ?? (await this.withMise(projectId, cwd, this.paneEnv))), [PANE_KEY_ENV]: key };
		await this.relayChecked(command, cwd, paneEnv);
		const respawned = await this.c.herdr.respawn(paneId, { cwd, command, env: paneEnv });
		this.c.launched.add(respawned.paneId, cwd, key);
		return respawned.paneId;
	}

	/**
	 * A thread pane's omp argv with its task's launch prompt (0540): `prompt.md` (the panes' shipped prompt, then the
	 * task file's Brief, Done-when and When you finish sections; the Brief alone with the layer off, 0576) as the one
	 * `--append-system-prompt` omp takes, in the shipped prompt's place (before the guard's `--config`). Unchanged
	 * without a recorded brief.
	 */
	taskPrompted(command: string[], task: TaskRecord | undefined): string[] {
		if (!task?.brief.trim()) return command;
		const file = launchPrompt(task, this.panePrompt, this.c.layer);
		const at = command.indexOf("--append-system-prompt");
		if (at >= 0) return command.map((arg, i) => (i === at + 1 ? file : arg));
		const guard = command.indexOf("--config");
		return [...command.slice(0, guard), "--append-system-prompt", file, ...command.slice(guard)];
	}

	/**
	 * Refuses to start the omp of `command` in `cwd` with `env` unless its `collab.relayUrl` resolves to loopback
	 * (0536, 0561): a pane that would host its session on a public relay doesn't start.
	 */
	async relayChecked(command: string[], cwd: string, env: Record<string, string>): Promise<void> {
		const relayUrlOf = this.c.options.relayUrlOf;
		const value = await (relayUrlOf ? relayUrlOf(command, cwd, env) : ompRelayUrl(this.omp, command, cwd, env));
		if (!isLoopbackRelay(value)) {
			const refused = notLoopback(cwd, value, configFiles(command));
			log("error", "service", refused.message);
			throw refused;
		}
	}

	/** `env` with mise's for a pane of `projectId` in `cwd` (0473, 0477); `env` alone without mise. */
	async withMise(projectId: string, cwd: string, env: Record<string, string>): Promise<Record<string, string>> {
		if (this.c.options.mise === null) return env;
		const mise = await miseEnv(projectId, cwd, { ...process.env, ...env }, this.c.options.mise);
		return { ...env, ...withoutJocastaKeys(mise) };
	}

	/**
	 * The env the task's checks run with (0542, 0559, SVC-16): the service's own without Jocasta's, herdr's and omp's
	 * keys, and the hunk variables its panes get.
	 */
	checkEnv(): Record<string, string> {
		const base: Record<string, string> = {};
		for (const [key, value] of Object.entries(process.env)) if (value !== undefined) base[key] = value;
		return { ...withoutJocastaKeys(base), ...this.paneEnv };
	}

	/**
	 * A thread pane's env (0457, 0473): the hunk variables, the thread's own, then mise's; with the layer off, the
	 * layer's switch too, so its `jocasta mcp` lists no thread tools (0576).
	 */
	async threadPaneEnv(project: Project, path: string): Promise<Record<string, string>> {
		const open = this.c.threads.ofProject(project.id).map((t) => t.path);
		const own = threadEnv(basename(project.mainPath), basename(path), this.c.numbers.of(path, open));
		const layer: Record<string, string> = this.c.layer ? {} : { [LAYER_ENV]: "off" };
		return this.withMise(project.id, path, { ...this.paneEnv, ...own, ...layer });
	}

	/** hunk's argv: the test seam's, else the shipped or npm binary; undefined when there is none. */
	hunk(): string[] | undefined {
		if (this.c.options.hunk) return this.c.options.hunk;
		const found = findHunk();
		return found ? [found] : undefined;
	}

	/** Jocasta's hunk session daemon, started when it isn't up; one started here is remembered by pid and start. */
	async ensureHunkDaemon(hunk: string[]): Promise<void> {
		await ensureHunkDaemon(hunk, this.hunkPort, (pid) => {
			const start = processStart(pid);
			if (start !== undefined) this.hunkDaemon = { pid, start };
		}).catch((error) => this.c.warn(error));
	}

	/**
	 * Ends the hunk session daemon this service started, the same process by pid and start time (0434: Jocasta's
	 * close stops everything it runs; 0486). Run once herdr's panes are gone, so no hunk pane starts another.
	 */
	async endHunkDaemon(): Promise<void> {
		const daemon = this.hunkDaemon;
		this.hunkDaemon = undefined;
		if (daemon && processStart(daemon.pid) === daemon.start) await endProcesses([daemon]);
	}
}

/**
 * When a Jocasta pane's omp starts again (0521, SVC-3): an omp that ended while its pane stayed open, at once the
 * first time and after a doubling wait for each ending in a row; a respawn that failed, after the same wait for each
 * failure in a row, its failure said once per pane and reason.
 */
export class OmpBackoff {
	/**
	 * omps Jocasta started again after they ended in an open pane (0521), by folder (a pane started again may get a new
	 * id): how many in a row, the last start, and whether the wait for the next was said.
	 */
	private readonly ompAgain = new Map<string, { count: number; at: number; said?: boolean }>();
	/** Panes whose respawn failed (SVC-3), by folder: how many in a row and the last try. */
	private readonly failures = new Map<string, { count: number; at: number }>();
	/** The last failure said per folder, so each is said once per reason. */
	private readonly said = new Map<string, string>();

	constructor(private readonly c: ServiceContext) {}

	/**
	 * A pane Jocasta started whose omp ended while the pane stayed open (0521) and that starts omp again now: at once
	 * the first time, then after a wait that doubles with each ending in a row (10 s first, at most 5 minutes),
	 * so an omp that keeps dying doesn't loop; each wait is said once into Jocasta's log. An omp that ran 10
	 * minutes since its last start ends afresh. Jocasta's own endings never get here: a finished or closed thread's
	 * pane is gone or no longer a thread's, Stop everything stops herdr with its panes, and a stopping service doesn't
	 * refresh.
	 */
	ompEnded(pane: PaneInfo): boolean {
		if (!pane.cwd || !this.c.launched.ended(pane)) return false;
		const key = pathKey(realPath(pane.cwd));
		const now = Date.now();
		const last = this.ompAgain.get(key);
		const count = last && now - last.at < OMP_AGAIN_RESET_MS ? last.count : 0;
		const wait = count === 0 ? 0 : againWait(count);
		if (last && now - last.at < wait) {
			if (!last.said) {
				last.said = true;
				const seconds = Math.ceil((wait - (now - last.at)) / 1000);
				this.c.warn(
					new Error(
						`omp ended in pane ${pane.paneId} (${pane.cwd}) ${count + 1} times in a row; it starts again in ${seconds} s.`,
					),
				);
			}
			return false;
		}
		this.ompAgain.set(key, { count: count + 1, at: now });
		return true;
	}

	/** Whether a pane in `cwd` whose respawn failed has waited out 0521's doubling wait (SVC-3); true for any other. */
	due(cwd: string | undefined): boolean {
		const failed = cwd ? this.failures.get(pathKey(realPath(cwd))) : undefined;
		if (!failed) return true;
		return Date.now() - failed.at >= againWait(failed.count);
	}

	/** Runs one pane's respawn: a failure is said once per pane and reason and waits before the next try (SVC-3). */
	async respawning(cwd: string, run: () => Promise<unknown>): Promise<void> {
		const key = pathKey(realPath(cwd));
		try {
			await run();
			this.failures.delete(key);
			this.said.delete(key);
		} catch (error) {
			const last = this.failures.get(key);
			this.failures.set(key, { count: (last?.count ?? 0) + 1, at: Date.now() });
			const message = `omp didn't start again in ${cwd}: ${(error as Error).message}`;
			if (this.said.get(key) !== message) {
				this.said.set(key, message);
				this.c.warn(new Error(message));
			}
		}
	}
}
