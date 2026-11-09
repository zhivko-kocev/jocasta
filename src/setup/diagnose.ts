/**
 * `jocasta diagnose` (0472): a read-only omp pane in workspace "Jocasta" that explains what went wrong from Jocasta's own
 * evidence and changes nothing. Its folder `<data>/diagnose/` holds `state.md` (the service's state now), `recent.log`
 * (the newest lines of Jocasta's log, 0431) and `omp.yml`, the pane's settings: plan mode from the start and the
 * shipped `config/skills/` (the diagnose skill). omp runs with read, grep and glob only, the two files attached to
 * its first message, so the log and the state (project names, pane folders, repo paths) go to his model provider with
 * it (0484); the pane's first line says so.
 */
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Herdr, HerdrSnapshot, PaneInfo } from "../frontier/server/herdr.ts";
import type { ServiceState } from "../frontier/server/launch.ts";
import { UserError } from "../shared/errors.ts";
import { LOG_DAYS, LOG_FILE } from "../shared/log.ts";
import type { ServiceFile } from "../shared/service-file.ts";

/** The boss's workspace (0392, 0404), where the diagnose pane opens. */
const WORKSPACE = "Jocasta";

/** Lines of Jocasta's log `recent.log` holds. */
export const RECENT_LOG_LINES = 400;

/**
 * omp's tools in the pane: reading and searching only (plan mode adds its own guarded `write` for the plan). No `find`:
 * it's off (0451, `find.enabled`), and omp refuses to start when asked for a tool it doesn't have.
 */
const READ_ONLY_TOOLS = "read,grep,glob";

export interface DiagnoseDeps {
	dataDir: string;
	/** The shipped `config/`, whose `skills/` holds the diagnose skill. */
	assetsDir: string;
	/** How omp starts (`["omp"]`). */
	omp: string[];
	/** `jocasta --version`'s line. */
	versions: string;
	session: string;
	service(): Promise<{ state: ServiceState; file: ServiceFile | undefined }>;
	/** Jocasta's herdr session, started when it isn't running. */
	connect(): Promise<Herdr>;
	/** Makes workspace "Jocasta" with its boss pane when it is missing. */
	ensureBoss(herdr: Herdr): Promise<PaneInfo>;
	/** Setup's start check (`setupCheck`): what waits, or why it couldn't check. */
	setupCheck(): Promise<{ waiting?: string; failed?: string }>;
	now?: Date;
}

/** The newest `lines` lines across Jocasta's log files, oldest first, and the files they came from. */
export function recentLog(logsDir: string, lines = RECENT_LOG_LINES): { text: string; files: string[] } {
	let names: string[];
	try {
		names = readdirSync(logsDir).filter((n) => LOG_FILE.test(n));
	} catch {
		names = [];
	}
	const kept: string[][] = [];
	const files: string[] = [];
	let count = 0;
	for (const name of names.sort().reverse()) {
		if (count >= lines) break;
		const fileLines = readFileSync(join(logsDir, name), "utf8").replace(/\n$/, "").split("\n");
		const take = fileLines.slice(Math.max(0, fileLines.length - (lines - count)));
		kept.unshift(take);
		files.unshift(name);
		count += take.length;
	}
	return { text: kept.length > 0 ? `${kept.flat().join("\n")}\n` : "", files };
}

/** The pane's omp settings: plan mode from the start, and the shipped skills (0472). */
export function diagnoseOverlay(skillsDir: string): string {
	return [
		"# `jocasta diagnose` (0472): a read-only pane. Written by Jocasta each time; changes are lost.",
		"plan:",
		"  defaultOnStartup: true",
		"skills:",
		"  customDirectories:",
		`    - ${JSON.stringify(skillsDir)}`,
		"",
	].join("\n");
}

function serviceLine(state: ServiceState, file: ServiceFile | undefined): string {
	if (!file) return `${state} (no jocasta-service.json: it isn't running)`;
	return `${state} (jocasta-service.json names pid ${file.pid}, port ${file.port})`;
}

function panesSection(snapshot: HerdrSnapshot): string[] {
	const lines: string[] = [];
	for (const workspace of snapshot.workspaces) {
		lines.push(`- ${workspace.label} (${workspace.id})${workspace.cwd ? ` in ${workspace.cwd}` : ""}`);
		for (const pane of snapshot.panes.filter((p) => p.workspaceId === workspace.id)) {
			const agent = pane.agent ? `${pane.agent} ${pane.agentState}` : `no agent (${pane.agentState})`;
			lines.push(
				`  - pane ${pane.paneId}: ${agent}${pane.cwd ? `, ${pane.cwd}` : ""}${pane.label ? `, "${pane.label}"` : ""}`,
			);
		}
	}
	return lines.length > 0 ? lines : ["- none"];
}

/** Writes the pane's folder and opens the pane; resolves the exit code. */
export async function runDiagnose(deps: DiagnoseDeps, print: (line: string) => void): Promise<number> {
	const now = deps.now ?? new Date();
	const dir = join(deps.dataDir, "diagnose");
	const logsDir = join(deps.dataDir, "logs");
	const { state, file } = await deps.service();
	let herdr: Herdr;
	try {
		herdr = await deps.connect();
	} catch (error) {
		throw new UserError(
			`herdr session \`${deps.session}\` doesn't answer and couldn't start (${error instanceof Error ? error.message : String(error)}); Jocasta's log is in ${logsDir}`,
			1,
		);
	}
	try {
		const version = await herdr.ping();
		const snapshot = await herdr.snapshot();
		// Setup's check runs before the log is read, and a failed one is said as such (SET-14).
		const setup = await deps.setupCheck();
		const recent = recentLog(logsDir);
		const statePath = join(dir, "state.md");
		const logPath = join(dir, "recent.log");
		const overlay = join(dir, "omp.yml");
		mkdirSync(dir, { recursive: true });
		writeFileSync(
			statePath,
			[
				`# Jocasta's state for \`jocasta diagnose\` (${now.toISOString()})`,
				"",
				`- Versions: ${deps.versions}`,
				`- Service: ${serviceLine(state, file)}`,
				`- herdr session \`${deps.session}\`: herdr ${version ?? "?"} answers; ${snapshot.workspaces.length} workspaces, ${snapshot.panes.length} panes`,
				`- Setup: ${setup.failed !== undefined ? `couldn't check (${setup.failed})` : (setup.waiting ?? "nothing waiting")}`,
				`- Logs: ${logsDir}, one file per day, ${LOG_DAYS} days kept; recent.log has the newest ${RECENT_LOG_LINES} lines${recent.files.length > 0 ? ` (from ${recent.files.join(", ")})` : " (there are none)"}`,
				"",
				"## herdr's workspaces and panes",
				...panesSection(snapshot),
				"",
			].join("\n"),
		);
		writeFileSync(logPath, recent.text);
		writeFileSync(overlay, diagnoseOverlay(join(deps.assetsDir, "skills")));
		let workspace = snapshot.workspaces.find((w) => w.label === WORKSPACE)?.id;
		if (!workspace) {
			workspace = (await deps.ensureBoss(herdr)).workspaceId;
		}
		const command = [
			...deps.omp,
			"--config",
			overlay,
			"--tools",
			READ_ONLY_TOOLS,
			`@${statePath}`,
			`@${logPath}`,
			"Jocasta's state and recent log, attached, go to the model provider with this message. Diagnose Jocasta with the diagnose skill from them. Change nothing.",
		];
		const tab = await herdr.createTab(workspace, { title: "diagnose", cwd: dir, command });
		await herdr.focus(tab.paneId);
		print(
			`Opened a read-only diagnose pane in workspace ${WORKSPACE} (herdr session \`${deps.session}\`): run \`jocasta\` to see it.`,
		);
		return 0;
	} finally {
		herdr.close();
	}
}
