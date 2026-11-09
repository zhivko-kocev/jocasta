/**
 * A project's lessons (0459): `<data>/projects/<pid>/lessons`, outside client repos, which every pane of the project
 * sees as the omp skill `lessons` through its overlay's `skills.customDirectories`. `SKILL.md` is the index; each
 * lesson is its own file beside it, written by the boss's `lesson` tool only on his word.
 *
 * Beside it, `<data>/projects/<pid>/.omp/agents/drift.md` is the weekly big-picture review's `drift` agent (0583):
 * omp finds project agents in the nearest `.omp/agents` above its working folder, and the weekly review runs in
 * `<data>/projects/<pid>/weekly`.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { writeAtomically } from "../../shared/fsx.ts";
import { shippedConfigDir } from "../../shared/install.ts";
import { lessonsDir, projectDir } from "../../shared/paths.ts";

const LESSONS_HEADING = /^##\s+Lessons\s*$/;

/**
 * The index with the shipped header (everything up to its `## Lessons` heading) and the lesson lines `current` holds
 * below its own heading, so a shipped edit of the header reaches every project (0583).
 */
function refreshedIndex(shipped: string, current: string): string {
	const shippedLines = shipped.replace(/\s+$/, "").split(/\r?\n/);
	const heading = shippedLines.findIndex((line) => LESSONS_HEADING.test(line));
	const header = heading >= 0 ? shippedLines.slice(0, heading + 1) : [...shippedLines, "", "## Lessons"];
	const currentLines = current.replace(/\s+$/, "").split(/\r?\n/);
	const at = currentLines.findIndex((line) => LESSONS_HEADING.test(line));
	const lessons = at >= 0 ? currentLines.slice(at + 1) : currentLines.filter((line) => /^- \[.*\]\(.*\)/.test(line));
	return `${[...header, ...lessons].join("\n")}\n`;
}

/** Writes `file` with `text` unless it already holds exactly that. */
function writeWhenChanged(file: string, text: string): void {
	if (existsSync(file) && readFileSync(file, "utf8") === text) return;
	writeAtomically(file, text);
}

/**
 * The project's lessons folder: its `SKILL.md` index with the shipped header from `config/lessons/SKILL.md` and the
 * project's lesson lines, and the shipped `drift` agent from `config/agents/drift.md` beside it.
 */
export function ensureLessonsSkill(projectId: string, configDir = shippedConfigDir()): string {
	const dir = lessonsDir(projectId);
	const index = join(dir, "SKILL.md");
	const shipped = readFileSync(join(configDir, "lessons", "SKILL.md"), "utf8");
	writeWhenChanged(index, existsSync(index) ? refreshedIndex(shipped, readFileSync(index, "utf8")) : shipped);
	writeWhenChanged(
		join(projectDir(projectId), ".omp", "agents", "drift.md"),
		readFileSync(join(configDir, "agents", "drift.md"), "utf8"),
	);
	return dir;
}
