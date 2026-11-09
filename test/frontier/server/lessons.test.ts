import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ensureLessonsSkill } from "../../../src/frontier/server/lessons.ts";
import { removeTree } from "../../../src/shared/fsx.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
import { lessonsDir, projectDir } from "../../../src/shared/paths.ts";

let root: string;
let saved: string | undefined;
beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), "jocasta-lessons-"));
	saved = process.env.JOCASTA_DATA_DIR;
	process.env.JOCASTA_DATA_DIR = join(root, "data");
});
afterEach(() => {
	if (saved === undefined) delete process.env.JOCASTA_DATA_DIR;
	else process.env.JOCASTA_DATA_DIR = saved;
	removeTree(root);
});

/** A config folder with a lessons index header and a drift agent. */
function config(header: string): string {
	const dir = join(root, "config");
	mkdirSync(join(dir, "lessons"), { recursive: true });
	mkdirSync(join(dir, "agents"), { recursive: true });
	writeFileSync(join(dir, "lessons", "SKILL.md"), `---\nname: lessons\n---\n\n${header}\n\n## Lessons\n`);
	writeFileSync(join(dir, "agents", "drift.md"), "---\nname: drift\ndescription: d\n---\nbody\n");
	return dir;
}

test("a new project gets the shipped index and the drift agent where omp finds project agents (0459, 0583)", () => {
	const dir = config("Only on his word.");
	expect(ensureLessonsSkill("p-1", dir)).toBe(lessonsDir("p-1"));
	expect(readFileSync(join(lessonsDir("p-1"), "SKILL.md"), "utf8")).toBe(
		readFileSync(join(dir, "lessons", "SKILL.md"), "utf8"),
	);
	// The weekly review runs in <project>/weekly; omp walks up from there to the nearest `.omp/agents`.
	expect(readFileSync(join(projectDir("p-1"), ".omp", "agents", "drift.md"), "utf8")).toBe(
		readFileSync(join(dir, "agents", "drift.md"), "utf8"),
	);
});

test("a shipped header change reaches an existing index and its lessons stay (0583)", () => {
	ensureLessonsSkill("p-1", config("Only when he says so."));
	const index = join(lessonsDir("p-1"), "SKILL.md");
	const lines =
		"- [Retry with backoff](retry-with-backoff.md) — adding an HTTP call\n- [No sync git](no-sync-git.md) — timers\n";
	writeFileSync(index, `${readFileSync(index, "utf8")}${lines}`);
	ensureLessonsSkill("p-1", config("Only on his word."));
	expect(readFileSync(index, "utf8")).toBe(`---\nname: lessons\n---\n\nOnly on his word.\n\n## Lessons\n${lines}`);
});

test("the shipped drift agent is named drift and reviews for drift, not bugs (0583)", () => {
	const text = readFileSync(join(shippedConfigDir(), "agents", "drift.md"), "utf8");
	const front = Bun.YAML.parse(text.split("---")[1] ?? "") as { name: string; description: string };
	expect(front.name).toBe("drift");
	expect(front.description).toBe(
		"Review a week of merged changes for design drift: a second way of doing what already has one, patterns that disagree, broken layering, a rule or lesson contradicted. Not bugs.",
	);
});
