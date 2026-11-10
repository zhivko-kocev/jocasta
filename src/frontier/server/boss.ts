/**
 * The boss pane (0392, 0404, 0424): workspace "Jocasta", first in herdr's list (Alt+1), with one omp pane in
 * `<data>/home`. Its manual is `<data>/home/.omp/APPEND_SYSTEM.md`, which omp loads for every session in that folder
 * when no `--append-system-prompt` is given (omp 18.18.6 `pi-coding-agent/src/main.ts:1401-1425`, `:1470`), so a pane
 * herdr restores or he starts by hand still has it; the procedures it rarely needs are skills in
 * `<data>/home/.omp/skills/` (0465). Both the service and bare `jocasta` ensure it.
 */
import { randomBytes, timingSafeEqual } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { asObject, readJsonObject, writeAtomically } from "../../shared/fsx.ts";
import { shippedConfigDir } from "../../shared/install.ts";
import { sameDir, samePath } from "../../shared/pathkey.ts";
import { dataDir } from "../../shared/paths.ts";
import type { Herdr, HerdrSnapshot, PaneInfo } from "./herdr.ts";

/** The boss's workspace label (0392, 0404). */
export const BOSS_WORKSPACE = "Jocasta";

/** The `today` pane's label: the plugin pane's title in `config/herdr-plugin/herdr-plugin.toml` (0553). */
export const TODAY_LABEL = "today";

/**
 * The herdr plugin's `today` pane (0553): plugin-owned, no agent in it, never the boss or a thread, and never adopted
 * as either.
 */
export function isTodayPane(pane: Pick<PaneInfo, "label" | "agent">): boolean {
	return pane.label === TODAY_LABEL && !pane.agent;
}

/** The boss pane's folder, `<data>/home`. */
export function bossHome(): string {
	return join(dataDir(), "home");
}

/** The boss pane's command: omp resuming its session line with the boss's own settings (0424) and the guard (0450). */
export function bossCommand(omp: string[], configDir = shippedConfigDir()): string[] {
	return [...omp, "--continue", "--config", join(configDir, "boss.yml"), "--config", join(configDir, "guard.yml")];
}

/**
 * What to tell him when the boss's omp would start without its tools: `<home>/.omp/mcp.json` (setup's migration 2,
 * 0392 project-level MCP config) has no enabled `jocasta` server, so the boss would improvise without them (beta day
 * P9). Undefined when the server is there.
 */
export function bossToolsMissing(home = bossHome()): string | undefined {
	const file = join(home, ".omp", "mcp.json");
	const server = asObject(asObject(readJsonObject(file)?.mcpServers)?.jocasta);
	if (server && server.enabled !== false) return undefined;
	return `The boss has no tools: ${file} has no "jocasta" MCP server. Run \`jocasta setup\`, then close Jocasta and run \`jocasta\` again.`;
}

/** Writes `text` to `file` when it differs. */
function installText(file: string, text: string): void {
	if (existsSync(file) && readFileSync(file, "utf8") === text) return;
	writeAtomically(file, text);
}

/**
 * The shipped manual as `<home>/.omp/APPEND_SYSTEM.md`, and each shipped boss skill `config/boss-skills/<name>/` as
 * `<home>/.omp/skills/<name>/` (0465); the shipped TDD lesson goes with `save-lesson`, which hands it to a project (0461).
 */
function installBossFiles(home = bossHome(), configDir = shippedConfigDir()): void {
	const omp = join(home, ".omp");
	installText(join(omp, "APPEND_SYSTEM.md"), readFileSync(join(configDir, "boss-manual.md"), "utf8"));
	const skills = join(configDir, "boss-skills");
	for (const skill of readdirSync(skills, { withFileTypes: true })) {
		if (!skill.isDirectory()) continue;
		for (const file of readdirSync(join(skills, skill.name))) {
			installText(join(omp, "skills", skill.name, file), readFileSync(join(skills, skill.name, file), "utf8"));
		}
	}
	installText(join(omp, "skills", "save-lesson", "tdd.md"), readFileSync(join(configDir, "lessons", "tdd.md"), "utf8"));
}

/**
 * The variable that carries a launched pane's own key into its environment (0578, SEC-NET-6): `jocasta mcp` sends it,
 * and the service knows the pane by it, never by a pane id the caller states.
 */
export const PANE_KEY_ENV = "JOCASTA_PANE_KEY";

/** A new pane key: 32 random bytes, hex (0578). */
export function newPaneKey(): string {
	return randomBytes(32).toString("hex");
}

/**
 * `<data>/panes.json`: the panes Jocasta started omp in (the boss, project and thread panes), each with its folder, its
 * pane key (0578: kept, so a service restart knows the panes still working) and herdr's terminal
 * behind it once seen. herdr keeps pane ids across a restart but gives each pane a new terminal, and
 * with `[session] resume_agents_on_restore = false` (`config/herdr.toml`) it restores them as shells: such a pane, in
 * the same folder and without omp, runs omp again with its own flags (its `--config`), which herdr's own resume
 * (`omp --resume=<session>`) would drop. A pane that kept its terminal runs omp again only once omp was seen running
 * in that terminal and has ended there (0521, `ended`); no pane Jocasta didn't start is ever touched. A thread pane
 * that was mid-turn when Jocasta stopped everything keeps its session file as `restore` (0549): its omp comes back with
 * `--restore` and goes on with that turn.
 */
export class LaunchedPanes {
	private entries: Record<string, { cwd: string; terminal?: string; restore?: string; key?: string }>;
	/**
	 * The terminal each recorded pane's omp was seen running in (0521). Not kept in `panes.json`: an omp that ended
	 * while no service watched (it stopped) isn't one this run saw end.
	 */
	private readonly ran = new Map<string, string>();

	constructor(private readonly file = join(dataDir(), "panes.json")) {
		this.entries = {};
		const parsed = readJsonObject(file);
		// Only well-formed entries: a hand-edited or torn file must not throw on every refresh (CO-19).
		for (const [paneId, entry] of Object.entries(parsed ?? {})) {
			const fields = asObject(entry);
			if (typeof fields?.cwd !== "string") continue;
			if (fields.terminal !== undefined && typeof fields.terminal !== "string") continue;
			this.entries[paneId] = {
				cwd: fields.cwd,
				...(typeof fields.terminal === "string" ? { terminal: fields.terminal } : {}),
				...(typeof fields.restore === "string" ? { restore: fields.restore } : {}),
				...(typeof fields.key === "string" ? { key: fields.key } : {}),
			};
		}
	}

	has(paneId: string): boolean {
		return Object.hasOwn(this.entries, paneId);
	}

	/** The folder Jocasta started omp in for `paneId`, or undefined for a pane it didn't start. */
	cwdOf(paneId: string): string | undefined {
		return this.entries[paneId]?.cwd;
	}

	/** The panes Jocasta started omp in. */
	panes(): string[] {
		return Object.keys(this.entries);
	}

	/** Jocasta started omp in `paneId`, in `cwd`, with pane key `key`: an omp not yet seen running (0521, 0578). */
	add(paneId: string, cwd: string, key?: string): void {
		this.entries[paneId] = { cwd, ...(key ? { key } : {}) };
		this.ran.delete(paneId);
		this.save();
	}

	/** The key `paneId` was started with (0578); undefined for a pane started before panes had keys, or not Jocasta's. */
	keyOf(paneId: string): string | undefined {
		return this.entries[paneId]?.key;
	}

	/** The pane started with `key` (0578), compared in constant time; undefined for any other key. */
	paneOfKey(key: string | undefined): string | undefined {
		if (!key) return undefined;
		const given = Buffer.from(key);
		for (const [paneId, entry] of Object.entries(this.entries)) {
			const known = Buffer.from(entry.key ?? "");
			if (known.length === given.length && timingSafeEqual(known, given)) return paneId;
		}
		return undefined;
	}

	/** `paneId`'s omp was mid-turn in session file `session` as Jocasta stopped (0549): its next start restores it. */
	pause(paneId: string, session: string): void {
		const entry = this.entries[paneId];
		if (!entry) return;
		entry.restore = session;
		this.save();
	}

	/** The session `paneId`'s omp was mid-turn in when Jocasta last stopped, or undefined. */
	paused(paneId: string): string | undefined {
		return this.entries[paneId]?.restore;
	}

	/**
	 * Notes each recorded pane's terminal the first time it is seen, and the terminal its omp is seen running in
	 * (0521); forgets panes herdr no longer has.
	 */
	seen(panes: PaneInfo[]): void {
		// herdr answering with no panes at all is a server still restoring, not every pane closed.
		if (panes.length === 0) return;
		const live = new Map(panes.map((p) => [p.paneId, p]));
		let changed = false;
		for (const [paneId, entry] of Object.entries(this.entries)) {
			const pane = live.get(paneId);
			if (!pane) {
				delete this.entries[paneId];
				this.ran.delete(paneId);
				changed = true;
				continue;
			}
			if (entry.terminal === undefined && pane.terminalId) {
				entry.terminal = pane.terminalId;
				changed = true;
			}
			if (pane.agent && pane.terminalId && entry.terminal === pane.terminalId) this.ran.set(paneId, pane.terminalId);
		}
		if (changed) this.save();
	}

	/** A pane Jocasta started that herdr restored as a shell: same folder, a new terminal, no omp. */
	restored(pane: PaneInfo): boolean {
		const entry = this.entries[pane.paneId];
		if (!entry?.terminal || !pane.terminalId || entry.terminal === pane.terminalId) return false;
		return this.shellIn(pane, entry.cwd);
	}

	/**
	 * A pane Jocasta started whose omp ended while the pane stayed open (0521): omp was seen running in its terminal,
	 * which it keeps, and it is a shell in the same folder now. Never a pane whose omp hasn't come up yet.
	 */
	ended(pane: PaneInfo): boolean {
		const entry = this.entries[pane.paneId];
		if (!entry?.terminal || !pane.terminalId || entry.terminal !== pane.terminalId) return false;
		if (this.ran.get(pane.paneId) !== pane.terminalId) return false;
		return this.shellIn(pane, entry.cwd);
	}

	/** `pane` runs no agent, in folder `cwd`. */
	private shellIn(pane: PaneInfo, cwd: string): boolean {
		if (pane.agent || pane.agentState !== "unknown" || !pane.cwd) return false;
		return sameDir(pane.cwd, cwd);
	}

	private save(): void {
		writeAtomically(this.file, `${JSON.stringify(this.entries, null, 2)}\n`);
	}
}

interface EnsureBossOptions {
	/** The boss pane's argv. */
	command: string[];
	/** The boss pane's own variables (this run's boss key, 0484), whenever its omp is started. */
	env?: Record<string, string>;
	/**
	 * Runs before the boss's omp starts with `command` in `cwd` and `env` (the relay check, 0561); a throw leaves the
	 * pane unstarted.
	 */
	check?(command: string[], cwd: string, env: Record<string, string>): Promise<void>;
	/** The panes Jocasta started: the boss pane joins them; a restored one runs omp again. */
	launched: LaunchedPanes;
	/**
	 * The service's start, or a boss pane whose omp ended in it (0521): a boss pane with no agent in it runs omp even
	 * when `launched` has no record of it (herdr restored workspace "Jocasta" from its own saved session, or a pane from
	 * an earlier build). Not on every refresh: a just-started omp reports nothing for a moment.
	 */
	adopt?: boolean;
	/** A found boss pane whose omp must start again (it lacks this run's `env`, 0484). */
	stale?(pane: PaneInfo): boolean;
	/** Called with the boss pane's id each time its omp is started here, with `env`. */
	started?(paneId: string): void;
	home?: string;
	configDir?: string;
}

/**
 * Workspace "Jocasta" with its boss pane, made when missing and moved first when it isn't (0404); a boss pane herdr
 * restored as a shell runs omp again. Bare `jocasta` and the service may both make it at once (herdr's list and
 * create aren't one step): after making it, the oldest "Jocasta" stays and any other is closed (CR-10). The boss pane
 * is one Jocasta started or one running omp; another pane is adopted only when it is the workspace's only one, so his
 * own shell tab there is never taken over (CO-21).
 */
export async function ensureBossPane(herdr: Herdr, options: EnsureBossOptions): Promise<PaneInfo> {
	const home = options.home ?? bossHome();
	installBossFiles(home, options.configDir);
	const bossWorkspaces = async () => (await herdr.workspaces()).filter((w) => w.label === BOSS_WORKSPACE);
	let workspaceId = (await bossWorkspaces())[0]?.id;
	const inBoss = async () => {
		// `today` beside the boss is never taken for it (0553).
		const panes = (await herdr.panes()).filter((p) => p.workspaceId === workspaceId && !isTodayPane(p));
		return (
			panes.find((p) => options.launched.has(p.paneId)) ??
			panes.find((p) => p.agent === "omp") ??
			(panes.length === 1 ? panes[0] : undefined)
		);
	};
	let pane = workspaceId ? await inBoss() : undefined;
	const { command } = options;
	// Each start of the boss's omp gets its own pane key (0578), kept with the pane.
	const key = newPaneKey();
	const env = { ...options.env, [PANE_KEY_ENV]: key };
	if (!pane) {
		await options.check?.(command, home, env);
		if (workspaceId) {
			const tab = await herdr.createTab(workspaceId, {
				title: BOSS_WORKSPACE,
				cwd: home,
				command,
				env,
			});
			pane = (await herdr.panes()).find((p) => p.paneId === tab.paneId);
		} else {
			const made = await herdr.ensureWorkspace(BOSS_WORKSPACE, home, command, env);
			const all = (await bossWorkspaces()).map((w) => w.id);
			workspaceId = [...all].sort(byCreation)[0] ?? made;
			for (const other of all) if (other !== workspaceId) await herdr.closeWorkspace(other);
			pane = await inBoss();
		}
		if (!pane) throw new Error(`herdr made workspace ${BOSS_WORKSPACE} without a pane`);
		options.launched.add(pane.paneId, home, key);
		options.started?.(pane.paneId);
	} else if (
		options.launched.restored(pane) ||
		(options.adopt && !pane.agent && pane.agentState === "unknown") ||
		options.stale?.(pane)
	) {
		await options.check?.(command, home, env);
		const respawned = await herdr.respawn(pane.paneId, { cwd: home, command, env });
		pane = (await herdr.panes()).find((p) => p.paneId === respawned.paneId);
		if (!pane) throw new Error(`herdr's respawned boss pane ${respawned.paneId} is gone`);
		options.launched.add(pane.paneId, home, key);
		options.started?.(pane.paneId);
	}
	if ((await herdr.workspaces())[0]?.id !== workspaceId) await herdr.moveWorkspace(workspaceId as string, 0);
	return pane;
}

/**
 * herdr's workspace ids in the order herdr made them: its ids are `w<number>` (a number that only grows), so both
 * racers of CR-10 keep the same one.
 */
function byCreation(a: string, b: string): number {
	return a.localeCompare(b, "en", { numeric: true });
}

/**
 * Whether a session whose server Jocasta just started is new (0495): it has no "Jocasta" workspace and at most one
 * other, herdr's default, made when his client attached to the empty session first. A session herdr restored from its
 * saved state lists the "Jocasta" workspace it had.
 */
export function freshSession(workspaces: { label: string }[]): boolean {
	return workspaces.length <= 1 && !workspaces.some((w) => w.label === BOSS_WORKSPACE);
}

/**
 * herdr's default workspace in a session Jocasta created (0495): herdr makes it only while a session has no workspace,
 * so it is the one made before "Jocasta". It closes while untouched: its one pane a shell (no agent) in his home
 * folder whose screen (`pane.read`) is one prompt with nothing typed after it. `wait` while there is nothing to judge
 * yet ("Jocasta" not made yet, or a shell that hasn't drawn its prompt); `none` when there is no default to close
 * ("Jocasta" came first) or it was touched.
 */
export async function closeHerdrDefault(
	herdr: Herdr,
	snapshot: HerdrSnapshot,
	home = homedir(),
): Promise<"wait" | "none" | "closed"> {
	const boss = snapshot.workspaces.find((w) => w.label === BOSS_WORKSPACE);
	const first = [...snapshot.workspaces].sort((a, b) => byCreation(a.id, b.id))[0];
	if (!boss || !first) return "wait";
	if (first.id === boss.id) return "none";
	const panes = snapshot.panes.filter((p) => p.workspaceId === first.id);
	const pane = panes[0];
	if (panes.length !== 1 || !pane?.cwd || pane.agent || pane.agentState !== "unknown") return "none";
	if (!samePath(pane.cwd, home)) return "none";
	const lines = (await herdr.screen(pane.paneId))
		.split("\n")
		.map((line) => line.trimEnd())
		.filter((line) => line !== "");
	if (lines.length === 0) return "wait";
	if (lines.length !== 1 || !/[>$#%]$/.test(lines[0] ?? "")) return "none";
	await herdr.closeWorkspace(first.id);
	return "closed";
}
