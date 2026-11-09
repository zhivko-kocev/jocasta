import { expect, test } from "bun:test";
import { mkdirSync, readFileSync, utimesSync } from "node:fs";
import { homedir } from "node:os";
import { join, parse } from "node:path";
import { UserError } from "../../src/shared/errors.ts";
import { projectIdFor } from "../../src/shared/identity.ts";
import { addProject, listProjects, removeProject, syncCatalog } from "../../src/shared/projects.ts";
import { addToList, readProjectsFile, removeFromList } from "../../src/shared/projects-file.ts";
import { openCatalog } from "../../src/shared/schema.ts";
import { makeRepo, sh, useTempJocasta, writeProjectsYaml } from "./fixture.ts";

const env = useTempJocasta();
const DEMO = "https://github.com/example-org/demo.git";

const rootsYaml = (root: string, extra = "") =>
	`version: 1\n# my roots\nroots:\n  - path: ${root.replaceAll("\\", "/")}\n    depth: 2\n    ignore: ["*.wt", "node_modules"]\n${extra}`;

test("the scan registers git top-levels under a root, honoring depth and ignore, and skips plain folders (0105)", () => {
	const code = join(env.root, "code");
	makeRepo(join(code, "demo"), { origin: DEMO });
	makeRepo(join(code, "group", "api"));
	makeRepo(join(code, "group", "deeper", "too-deep"));
	makeRepo(join(code, "node_modules", "pkg"));
	mkdirSync(join(code, "notes"));
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	syncCatalog(db);
	expect(listProjects(db).map((p) => p.name)).toEqual(["api", "demo"]);
	db.close();
});

test("clones and worktrees of one repo are one project with main, clone and worktree paths (0104)", () => {
	const code = join(env.root, "code");
	const main = makeRepo(join(code, "a-demo"), { origin: DEMO });
	const clone = makeRepo(join(code, "b-demo"), { origin: "git@github.com:example-org/demo.git" });
	const worktree = join(code, "c-demo-wt");
	sh(main, "worktree", "add", "-q", "-b", "feature/x", worktree);
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	syncCatalog(db);
	const projects = listProjects(db);
	expect(projects).toHaveLength(1);
	const [project] = projects;
	expect(project?.id).toBe(projectIdFor("github.com/example-org/demo"));
	expect(project?.mainPath).toBe(main);
	expect(project?.paths.map((p) => p.role).sort()).toEqual(["clone", "main", "worktree"]);
	expect(project?.paths.find((p) => p.role === "clone")?.path).toBe(clone);
	db.close();
});

test("an unchanged .git/config is not re-resolved; a changed one is (0108)", () => {
	const code = join(env.root, "code");
	const repo = makeRepo(join(code, "demo"));
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	syncCatalog(db);
	const before = listProjects(db)[0];
	expect(before?.identity).toMatch(/^root:/);
	// Same mtime: a cached identity stays even though git would now answer differently.
	db.query("UPDATE projects SET identity = 'root:cached'").run();
	syncCatalog(db);
	expect(listProjects(db)[0]?.identity).toBe("root:cached");
	// Adding an origin rewrites .git/config (its mtime moves), so the path is resolved again into the origin's project.
	sh(repo, "remote", "add", "origin", DEMO);
	const later = new Date(Date.now() + 10_000);
	utimesSync(join(repo, ".git", "config"), later, later);
	syncCatalog(db);
	expect(listProjects(db).map((p) => p.identity)).toEqual(["github.com/example-org/demo"]);
	db.close();
});

test("project add persists to add: keeping comments, registers non-git folders by path, and remove excludes (0109, 0111)", () => {
	const file = writeProjectsYaml(env.config, "version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n");
	const notes = join(env.root, "notes");
	mkdirSync(notes);
	const db = openCatalog();
	const added = addProject(db, notes);
	expect(added.identity.startsWith("path:")).toBe(true);
	expect(added.git).toBe(false);
	const text = readFileSync(file, "utf8");
	expect(text).toContain("version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n  - ");
	expect(readProjectsFile(file).add).toHaveLength(2);

	const removed = removeProject(db, added.name);
	expect(removed.id).toBe(added.id);
	expect(listProjects(db)).toEqual([]);
	const after = readProjectsFile(file);
	expect(after.add).toEqual(["/nowhere/else"]);
	expect(after.exclude).toHaveLength(1);
	expect(readFileSync(file, "utf8")).toContain("# adds below");
	db.close();
});

test("removing a scanned repo excludes it so the next scan skips it; adding it again lifts the exclusion", () => {
	const code = join(env.root, "code");
	const repo = makeRepo(join(code, "demo"), { origin: DEMO });
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	syncCatalog(db);
	removeProject(db, projectIdFor("github.com/example-org/demo"));
	syncCatalog(db);
	expect(listProjects(db)).toEqual([]);
	addProject(db, repo);
	expect(listProjects(db).map((p) => p.name)).toEqual(["demo"]);
	expect(readProjectsFile().exclude).toEqual([]);
	db.close();
});

test("project add on a subfolder registers the repo's top level", () => {
	const repo = makeRepo(join(env.root, "demo"), { origin: DEMO });
	mkdirSync(join(repo, "src"));
	const db = openCatalog();
	expect(addProject(db, join(repo, "src")).mainPath).toBe(repo);
	db.close();
});

test("home and drive roots are never projects (0106)", () => {
	const db = openCatalog();
	expect(() => addProject(db, homedir())).toThrow(UserError);
	expect(() => addProject(db, parse(env.root).root)).toThrow(UserError);
	db.close();
});

test("remove refuses an unknown name and an ambiguous one", () => {
	writeProjectsYaml(env.config, "version: 1\n");
	const one = join(env.root, "x", "api");
	const two = join(env.root, "y", "api");
	mkdirSync(one, { recursive: true });
	mkdirSync(two, { recursive: true });
	const db = openCatalog();
	addProject(db, one);
	addProject(db, two);
	expect(() => removeProject(db, "nope")).toThrow(UserError);
	expect(() => removeProject(db, "api")).toThrow(/2 projects are named api/);
	db.close();
});

test("names: renames a project by id (0112)", () => {
	const code = join(env.root, "code");
	makeRepo(join(code, "demo"), { origin: DEMO });
	const id = projectIdFor("github.com/example-org/demo");
	writeProjectsYaml(env.config, rootsYaml(code, `names:\n  ${id}: demo-web\n`));
	const db = openCatalog();
	syncCatalog(db);
	expect(listProjects(db).map((p) => p.name)).toEqual(["demo-web"]);
	db.close();
});

test("list edits rewrite a flow sequence as a block and keep CRLF line endings", () => {
	const file = writeProjectsYaml(env.config, "version: 1\r\nexclude: [] # none yet\r\nroots: []\r\n");
	expect(addToList("exclude", "D:/old-fork", file)).toBe(true);
	expect(addToList("exclude", "D:/old-fork", file)).toBe(false);
	expect(readFileSync(file, "utf8")).toBe("version: 1\r\nexclude: # none yet\r\n  - D:/old-fork\r\nroots: []\r\n");
	expect(removeFromList("exclude", "D:/old-fork", file)).toBe(true);
	expect(readProjectsFile(file).exclude).toEqual([]);
});
