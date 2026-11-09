/**
 * A project's lessons (0459): `<data>/projects/<pid>/lessons`, outside client repos, which every pane of the project
 * sees as the omp skill `lessons` through its overlay's `skills.customDirectories`. `SKILL.md` is the index; each
 * lesson is its own file beside it, written by the boss's `lesson` tool only on his word.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { writeAtomically } from "../../shared/fsx.ts";
import { shippedConfigDir } from "../../shared/install.ts";
import { lessonsDir } from "../../shared/paths.ts";

/** The project's lessons folder, its `SKILL.md` made from the shipped `config/lessons/SKILL.md` when missing. */
export function ensureLessonsSkill(projectId: string, configDir = shippedConfigDir()): string {
	const dir = lessonsDir(projectId);
	const index = join(dir, "SKILL.md");
	if (!existsSync(index)) writeAtomically(index, readFileSync(join(configDir, "lessons", "SKILL.md"), "utf8"));
	return dir;
}
