import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { DbError, openDb } from "../../src/shared/db.ts";
import { CATALOG_MIGRATIONS, openCatalog } from "../../src/shared/schema.ts";
import { useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();

const MIGRATIONS = ["CREATE TABLE a (x INTEGER) STRICT;", "CREATE TABLE b (y TEXT) STRICT;"];

test("runs migrations in order, records user_version, and applies 0099's connection settings", () => {
	const path = join(env.root, "x.db");
	const db = openDb(path, MIGRATIONS);
	expect(db.query("PRAGMA user_version").get()).toEqual({ user_version: 2 });
	expect(db.query("PRAGMA journal_mode").get()).toEqual({ journal_mode: "wal" });
	expect(db.query("PRAGMA foreign_keys").get()).toEqual({ foreign_keys: 1 });
	expect(db.query("PRAGMA synchronous").get()).toEqual({ synchronous: 1 }); // NORMAL
	expect(db.query("PRAGMA busy_timeout").get()).toEqual({ timeout: 5000 });
	db.close();
});

test("an older file only runs the migrations it lacks", () => {
	const path = join(env.root, "x.db");
	openDb(path, MIGRATIONS.slice(0, 1)).close();
	const db = openDb(path, MIGRATIONS);
	expect(db.query("PRAGMA user_version").get()).toEqual({ user_version: 2 });
	expect(db.query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all()).toEqual([
		{ name: "a" },
		{ name: "b" },
	]);
	db.close();
});

test("a failing migration rolls back and leaves the version where it was", () => {
	const path = join(env.root, "x.db");
	expect(() =>
		openDb(path, [MIGRATIONS[0] as string, "CREATE TABLE c (z TEXT) STRICT; CREATE TABLE a (x INT);"]),
	).toThrow();
	const raw = new Database(path);
	expect(raw.query("PRAGMA user_version").get()).toEqual({ user_version: 1 });
	expect(raw.query("SELECT name FROM sqlite_master WHERE name = 'c'").get()).toBeNull();
	raw.close();
});

test("a second process migrating the same old file at once makes the first skip that step, not fail (SHR-13)", async () => {
	const path = join(env.root, "x.db");
	openDb(path, []).close();
	// Another process takes the write lock and runs migration 0 while this one has already read user_version 0. It
	// commits after a real delay: openDb blocks this thread on the lock, so nothing here can tell it when to let go.
	const holder = Bun.spawn(
		[
			process.execPath,
			"-e",
			`import { Database } from "bun:sqlite";
const db = new Database(${JSON.stringify(path)});
db.exec("BEGIN IMMEDIATE");
db.exec(${JSON.stringify(MIGRATIONS[0])});
db.exec("PRAGMA user_version = 1");
console.log("holding");
await Bun.sleep(500);
db.exec("COMMIT");
db.close();`,
		],
		{ stdout: "pipe", stderr: "inherit", windowsHide: true },
	);
	const reader = holder.stdout.getReader();
	const { value } = await reader.read();
	expect(new TextDecoder().decode(value)).toContain("holding");
	const db = openDb(path, MIGRATIONS);
	expect(db.query("PRAGMA user_version").get()).toEqual({ user_version: 2 });
	expect(db.query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all()).toEqual([
		{ name: "a" },
		{ name: "b" },
	]);
	db.close();
	reader.releaseLock();
	expect(await holder.exited).toBe(0);
});

test("a newer file is refused with 0101's message and left untouched", () => {
	const path = join(env.root, "catalog.db");
	const raw = new Database(path);
	raw.exec("PRAGMA user_version = 7");
	raw.close();
	const before = readFileSync(path);
	expect(() => openDb(path, MIGRATIONS)).toThrow(
		new DbError(`${path} is schema 7; this Jocasta knows up to 2. Update Jocasta.`),
	);
	expect(readFileSync(path).equals(before)).toBe(true);
});

test("a file that isn't a database is reported and never rewritten", () => {
	const path = join(env.root, "project.db");
	writeFileSync(path, "not a database, just text that is long enough to have a header ".repeat(4));
	const before = readFileSync(path);
	expect(() => openDb(path, MIGRATIONS)).toThrow(
		/project\.db can't be opened: .*not a database.*\. It was left as is\./,
	);
	expect(readFileSync(path).equals(before)).toBe(true);
});

test("a write lock held past busy_timeout is reported as locked", () => {
	const path = join(env.root, "catalog.db");
	openDb(path, []).close();
	const holder = new Database(path);
	holder.exec("PRAGMA journal_mode = WAL");
	holder.exec("BEGIN EXCLUSIVE");
	try {
		const started = Date.now();
		expect(() => openDb(path, MIGRATIONS, { busyTimeoutMs: 200 })).toThrow(
			new DbError(`${path} is locked by another Jocasta process (waited 0.2 s).`),
		);
		expect(Date.now() - started).toBeGreaterThanOrEqual(150);
	} finally {
		holder.exec("ROLLBACK");
		holder.close();
	}
});

test("catalog.db opens at its migration ledger version with STRICT tables", () => {
	const catalog = openCatalog();
	expect(catalog.query("PRAGMA user_version").get()).toEqual({ user_version: CATALOG_MIGRATIONS.length });
	expect(() =>
		catalog
			.query(
				"INSERT INTO project_paths (path, project_id, role, git_config_mtime, added_at) VALUES ('x', 'p', 'main', 'soon', 'now')",
			)
			.run(),
	).toThrow(/cannot store TEXT value in INTEGER column/); // STRICT (0100)
	expect(() =>
		catalog
			.query(
				"INSERT INTO threads (id, kind, title, state, created_at, last_active_at) VALUES ('t-1', 'project', 'x', 'open', 'a', 'a')",
			)
			.run(),
	).toThrow(); // only the home thread lives in catalog.db (0125)
	catalog.close();
});
