/** The per-project omp overlay (0399, 0400) every project and thread pane loads after the guard. */
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { lessonsDir, projectDir } from "../../shared/paths.ts";
import { ensureLessonsSkill } from "./lessons.ts";
import { RELAY_PORT, relayOverlay } from "./relay.ts";

/** `<data>/projects/<pid>/omp.yml` (0399, 0400): the per-project omp overlay every project and thread pane loads. */
export function overlayFile(projectId: string): string {
	return join(projectDir(projectId), "omp.yml");
}

/**
 * The overlay's text: the project's own Mnemopi bank in its own file, shared by its worktrees (0400, 0447); new
 * sessions in plan mode; compaction at 300k tokens, a handoff first (0399, 0448, 0592); private notes from
 * `AGENTS.local.md` (0449); the project's lessons as the skill `lessons` (0459: omp lists `<dir>/<name>/SKILL.md`, so
 * the project folder is the skill directory and `lessons/` its one skill); no reviewer model, so omp's bundled reviewer
 * runs on his `slow` role (0488); the privacy settings, whatever his global config says (0478): no Auto QA reports, no
 * Claude Code plugins, web search on DuckDuckGo (an overlay's `modelRoles.web` leaves his other roles alone; its
 * `disabledProviders` replaces his list); every session hosted in Jocasta's relay on `relayPort` and `/share` kept on
 * this machine (0536, 0561). Paths are JSON strings, valid YAML.
 */
export function overlayText(projectId: string, relayPort = RELAY_PORT): string {
	const path = (value: string) => JSON.stringify(value);
	return [
		"memory:",
		"  backend: mnemopi",
		"mnemopi:",
		"  scoping: global",
		`  bank: ${projectId}`,
		"  llmMode: smol",
		`  dbPath: ${path(join(projectDir(projectId), "mnemopi.db"))}`,
		"plan:",
		"  defaultOnStartup: true",
		"compaction:",
		"  methodOrder: [handoff, remote, soft, shake, snapcompact]",
		"  thresholdTokens: 300000",
		"  idleEnabled: true",
		"  idleThresholdTokens: 100000",
		"contextFiles:",
		"  extra: [AGENTS.local.md]",
		"skills:",
		`  customDirectories: [${path(dirname(lessonsDir(projectId)))}]`,
		"dev:",
		"  autoqa: false",
		"disabledProviders: [claude-plugins]",
		"modelRoles:",
		"  web: web/duckduckgo",
		...relayOverlay(relayPort),
		"",
	].join("\n");
}

/** Errors Windows gives a rename over a file another process has open (omp reading the overlay as it starts, 0534). */
const RENAME_BUSY: Record<string, true> = { EPERM: true, EBUSY: true, EACCES: true };

/** How often, and how far apart, a refused overlay rename is tried again (0534): 10 times, 100 ms apart. */
const RENAME_TRIES = 10;
const RENAME_RETRY_MS = 100;

/** Each project's overlay write under way (0534), shared by every start that needs it meanwhile. */
const overlayWrites = new Map<string, Promise<string>>();

/**
 * The project's overlay, written when it differs, and its lessons skill (0459); resolves the overlay's path. One write
 * per project at a time: a start that comes while one runs waits for that one (0534). An overlay that already holds
 * the text isn't written again, so parallel starts in one project never replace the file their omps are reading; a
 * rename Windows refuses (EPERM, EBUSY, EACCES: an omp has it open) is tried again for about a second. The waits are
 * async, not `writeAtomically`'s: the service's event loop goes on while an omp holds the file.
 */
export function ensureOverlay(projectId: string, relayPort = RELAY_PORT): Promise<string> {
	const running = overlayWrites.get(projectId);
	if (running) return running;
	const write = writeOverlay(projectId, relayPort).finally(() => overlayWrites.delete(projectId));
	overlayWrites.set(projectId, write);
	return write;
}

async function writeOverlay(projectId: string, relayPort: number): Promise<string> {
	const overlay = overlayFile(projectId);
	ensureLessonsSkill(projectId);
	const text = overlayText(projectId, relayPort);
	try {
		if (readFileSync(overlay, "utf8") === text) return overlay;
	} catch {
		// none yet
	}
	mkdirSync(dirname(overlay), { recursive: true });
	const temp = `${overlay}.${process.pid}.tmp`;
	try {
		writeFileSync(temp, text);
		for (let attempt = 1; ; attempt++) {
			try {
				renameSync(temp, overlay);
				break;
			} catch (error) {
				if (attempt >= RENAME_TRIES || !RENAME_BUSY[(error as NodeJS.ErrnoException).code ?? ""]) throw error;
				await Bun.sleep(RENAME_RETRY_MS);
			}
		}
	} catch (error) {
		rmSync(temp, { force: true });
		throw error;
	}
	return overlay;
}
