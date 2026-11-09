import { expect, test } from "bun:test";
import { mkdirSync, readFileSync, realpathSync, utimesSync, writeFileSync } from "node:fs";
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

test("the scan registers git top-levels under a root, honoring depth and ignore, and skips plain folders (0105)", async () => {
	const code = join(env.root, "code");
	makeRepo(join(code, "demo"), { origin: DEMO });
	makeRepo(join(code, "group", "api"));
	makeRepo(join(code, "group", "deeper", "too-deep"));
	makeRepo(join(code, "node_modules", "pkg"));
	mkdirSync(join(code, "notes"));
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	await syncCatalog(db);
	expect(listProjects(db).map((p) => p.name)).toEqual(["api", "demo"]);
	db.close();
});

test("clones and worktrees of one repo are one project with main, clone and worktree paths (0104)", async () => {
	const code = join(env.root, "code");
	const main = makeRepo(join(code, "a-demo"), { origin: DEMO });
	const clone = makeRepo(join(code, "b-demo"), { origin: "git@github.com:example-org/demo.git" });
	const worktree = join(code, "c-demo-wt");
	sh(main, "worktree", "add", "-q", "-b", "feature/x", worktree);
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	await syncCatalog(db);
	const projects = listProjects(db);
	expect(projects).toHaveLength(1);
	const [project] = projects;
	expect(project?.id).toBe(projectIdFor("github.com/example-org/demo"));
	expect(project?.mainPath).toBe(main);
	expect(project?.paths.map((p) => p.role).sort()).toEqual(["clone", "main", "worktree"]);
	expect(project?.paths.find((p) => p.role === "clone")?.path).toBe(clone);
	db.close();
});

test("an unchanged .git/config is not re-resolved; a changed one is (0108)", async () => {
	const code = join(env.root, "code");
	const repo = makeRepo(join(code, "demo"));
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	await syncCatalog(db);
	const before = listProjects(db)[0];
	expect(before?.identity).toMatch(/^root:/);
	// Same mtime: a cached identity stays even though git would now answer differently.
	db.query("UPDATE projects SET identity = 'root:cached'").run();
	await syncCatalog(db);
	expect(listProjects(db)[0]?.identity).toBe("root:cached");
	// Adding an origin rewrites .git/config (its mtime moves), so the path is resolved again into the origin's project.
	sh(repo, "remote", "add", "origin", DEMO);
	const later = new Date(Date.now() + 10_000);
	utimesSync(join(repo, ".git", "config"), later, later);
	await syncCatalog(db);
	expect(listProjects(db).map((p) => p.identity)).toEqual(["github.com/example-org/demo"]);
	db.close();
});

test("project add persists to add: keeping comments, registers non-git folders by path, and remove excludes (0109, 0111)", async () => {
	const file = writeProjectsYaml(env.config, "version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n");
	const notes = join(env.root, "notes");
	mkdirSync(notes);
	const db = openCatalog();
	const added = await addProject(db, notes);
	expect(added.identity.startsWith("path:")).toBe(true);
	expect(added.git).toBe(false);
	const text = readFileSync(file, "utf8");
	expect(text).toContain("version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n  - ");
	expect(readProjectsFile(file).add).toHaveLength(2);

	const removed = await removeProject(db, added.name);
	expect(removed.id).toBe(added.id);
	expect(listProjects(db)).toEqual([]);
	const after = readProjectsFile(file);
	expect(after.add).toEqual(["/nowhere/else"]);
	expect(after.exclude).toHaveLength(1);
	expect(readFileSync(file, "utf8")).toContain("# adds below");
	db.close();
});

test("removing a scanned repo excludes it so the next scan skips it; adding it again lifts the exclusion", async () => {
	const code = join(env.root, "code");
	const repo = makeRepo(join(code, "demo"), { origin: DEMO });
	writeProjectsYaml(env.config, rootsYaml(code));
	const db = openCatalog();
	await syncCatalog(db);
	await removeProject(db, projectIdFor("github.com/example-org/demo"));
	await syncCatalog(db);
	expect(listProjects(db)).toEqual([]);
	await addProject(db, repo);
	expect(listProjects(db).map((p) => p.name)).toEqual(["demo"]);
	expect(readProjectsFile().exclude).toEqual([]);
	db.close();
});

test("project add on a subfolder registers the repo's top level", async () => {
	const repo = makeRepo(join(env.root, "demo"), { origin: DEMO });
	mkdirSync(join(repo, "src"));
	const db = openCatalog();
	expect((await addProject(db, join(repo, "src"))).mainPath).toBe(repo);
	db.close();
});

test("home and drive roots are never projects (0106)", async () => {
	const db = openCatalog();
	await expect(addProject(db, homedir())).rejects.toThrow(UserError);
	await expect(addProject(db, parse(env.root).root)).rejects.toThrow(UserError);
	db.close();
});

test("remove refuses an unknown name and an ambiguous one", async () => {
	writeProjectsYaml(env.config, "version: 1\n");
	const one = join(env.root, "x", "api");
	const two = join(env.root, "y", "api");
	mkdirSync(one, { recursive: true });
	mkdirSync(two, { recursive: true });
	const db = openCatalog();
	await addProject(db, one);
	await addProject(db, two);
	await expect(removeProject(db, "nope")).rejects.toThrow(UserError);
	await expect(removeProject(db, "api")).rejects.toThrow(/2 projects are named api/);
	db.close();
});

test("names: renames a project by id (0112)", async () => {
	const code = join(env.root, "code");
	makeRepo(join(code, "demo"), { origin: DEMO });
	const id = projectIdFor("github.com/example-org/demo");
	writeProjectsYaml(env.config, rootsYaml(code, `names:\n  ${id}: demo-web\n`));
	const db = openCatalog();
	await syncCatalog(db);
	expect(listProjects(db).map((p) => p.name)).toEqual(["demo-web"]);
	db.close();
});

test("list edits rewrite a flow sequence as a block and keep CRLF line endings", async () => {
	const file = writeProjectsYaml(env.config, "version: 1\r\nexclude: [] # none yet\r\nroots: []\r\n");
	expect(addToList("exclude", "D:/old-fork", file)).toBe(true);
	expect(addToList("exclude", "D:/old-fork", file)).toBe(false);
	expect(readFileSync(file, "utf8")).toBe("version: 1\r\nexclude: # none yet\r\n  - D:/old-fork\r\nroots: []\r\n");
	expect(removeFromList("exclude", "D:/old-fork", file)).toBe(true);
	expect(readProjectsFile(file).exclude).toEqual([]);
});

test("a malformed projects.yaml stops with one line naming the file and line, exit 1 (0582)", async () => {
	const file = writeProjectsYaml(env.config, "version: 1\nroots:\n  - path: D:/code\n    depth: 0\n");
	const db = openCatalog();
	env.defer(() => db.close());
	const depth = await syncCatalog(db).catch((error: unknown) => error);
	expect(depth).toBeInstanceOf(UserError);
	expect((depth as UserError).exitCode).toBe(1);
	expect((depth as UserError).message).toBe(`${file}:4: roots[0].depth must be >= 1`);
	writeProjectsYaml(env.config, "version: 1\nadd: [unclosed\n");
	expect(() => readProjectsFile(file)).toThrow(new UserError(`${file}:2: Unexpected token`, 1));
});

test("project add on a linked worktree adds its main checkout and writes add: only for that (0582)", async () => {
	const file = writeProjectsYaml(env.config, "version: 1\n");
	const main = makeRepo(join(env.root, "acme-web"), { origin: DEMO });
	const worktree = join(env.root, "acme-web.wt", "feature");
	sh(main, "worktree", "add", "-q", "-b", "feature", worktree);
	const db = openCatalog();
	env.defer(() => db.close());
	const added = await addProject(db, worktree);
	expect(added.mainPath).toBe(main);
	expect(added.mainCheckoutOf).toBe(realpathSync.native(worktree));
	expect(readProjectsFile(file).add).toEqual([main.replaceAll("\\", "/")]);
	expect(listProjects(db).map((p) => p.id)).toEqual([projectIdFor("github.com/example-org/demo")]);
});

test("a repo first seen before its first commit becomes git once it has one (SHR-2)", async () => {
	const fresh = join(env.root, "fresh");
	mkdirSync(fresh);
	sh(fresh, "init", "-q", "-b", "main");
	writeProjectsYaml(env.config, "version: 1\n");
	const db = openCatalog();
	env.defer(() => db.close());
	const before = await addProject(db, fresh);
	expect(before.git).toBe(false);
	expect(before.identity.startsWith("path:")).toBe(true);
	writeFileSync(join(fresh, "README.md"), "fresh\n");
	sh(fresh, "add", ".");
	sh(fresh, "commit", "-q", "-m", "first");
	await syncCatalog(db);
	const [after] = listProjects(db);
	expect(after?.git).toBe(true);
	expect(after?.identity).toBe(`root:${sh(fresh, "rev-list", "--max-parents=0", "HEAD")}`);
	expect(after?.mainPath).toBe(realpathSync.native(fresh));
});
