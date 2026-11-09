import type { Database } from "bun:sqlite";
import { join } from "node:path";
import { openDb } from "./db.ts";
import { catalogPath, dataDir } from "./paths.ts";

// The `threads` columns of 0126; `kind` differs per file (0125, 0126).
const threadsTable = (kinds: string) => `CREATE TABLE threads (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN (${kinds})),
  parent_id      TEXT REFERENCES threads(id),
  title          TEXT NOT NULL,
  brief          TEXT,
  branch         TEXT,
  path           TEXT,
  state          TEXT NOT NULL CHECK (state IN ('open','closed')),
  created_at     TEXT NOT NULL,
  last_active_at TEXT NOT NULL,
  closed_at      TEXT
) STRICT;`;

/**
 * `catalog.db` migrations (appendix I ledger): v1 projects, paths and the home thread (0104, 0125); v2 the home thread's
 * pending question (0246); v3 Frontier's thread runtime state (0266): one row per thread that has a run state (no row
 * is idle); `queued_text` is the message a `queued` thread runs when a slot frees (0268); `boss_run` marks threads the
 * boss started (0270); v4 memory proposals waiting for his word (0279, 0335) and the transcripts `jocasta memory import`
 * has read (0281). A proposal's `text` is the new line (NULL drops the `replaces` lines); `replaces` is a JSON array of
 * the lines it replaces (0281); v5 the event queue (0298): one row per source id; `raised_at` is set when it was
 * delivered, `reminded_at` after the one reminder (0303), `resolved_at` when its cause went away; v6 the session
 * watcher's compaction and handoff summaries from omp sessions (0389, 0400): one row per session file and compaction
 * entry id; `at` is the entry's time in epoch ms; `sent` is 1 once the summary went to Graphiti as an episode; v7 the
 * summary's watched project, stored when first resolved so it still posts after its cwd folder is gone (0440 (4)); v8
 * drops v3's `thread_runtime`, which nothing read or wrote: a thread's state comes from omp and herdr (0547).
 */
export const CATALOG_MIGRATIONS: readonly string[] = [
	`CREATE TABLE projects (
  id         TEXT PRIMARY KEY,
  identity   TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
) STRICT;
CREATE TABLE project_paths (
  path             TEXT PRIMARY KEY,
  project_id       TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  role             TEXT NOT NULL CHECK (role IN ('main','worktree','clone')),
  git_config_mtime INTEGER,
  added_at         TEXT NOT NULL
) STRICT;
${threadsTable("'home'")}`,
	"ALTER TABLE threads ADD COLUMN pending_question TEXT;",
	`CREATE TABLE thread_runtime (
  thread_id   TEXT PRIMARY KEY,
  project_id  TEXT,
  state       TEXT NOT NULL CHECK (state IN ('queued','starting','running','blocked','done','failed','cancelled','interrupted')),
  boss_run    INTEGER NOT NULL DEFAULT 0 CHECK (boss_run IN (0,1)),
  queued_text TEXT,
  updated_at  TEXT NOT NULL
) STRICT;`,
	`CREATE TABLE memory_proposals (
  id         INTEGER PRIMARY KEY,
  scope      TEXT NOT NULL CHECK (scope IN ('project','personal')),
  project_id TEXT,
  text       TEXT,
  replaces   TEXT NOT NULL DEFAULT '[]',
  source     TEXT NOT NULL CHECK (source IN ('remember','promotion','consolidation','import','decision')),
  thread_id  TEXT,
  state      TEXT NOT NULL CHECK (state IN ('pending','accepted','rejected')),
  created_at TEXT NOT NULL,
  decided_at TEXT,
  CHECK ((scope = 'project') = (project_id IS NOT NULL)),
  CHECK (text IS NOT NULL OR replaces <> '[]')
) STRICT;
CREATE INDEX memory_proposals_state ON memory_proposals(state, id);
CREATE TABLE memory_imports (
  path        TEXT PRIMARY KEY,
  imported_at TEXT NOT NULL
) STRICT;`,
	`CREATE TABLE events (
  id          INTEGER PRIMARY KEY,
  source_id   TEXT NOT NULL UNIQUE,
  source      TEXT NOT NULL CHECK (source IN ('github','azure','calendar','thread','machine','inbox')),
  kind        TEXT NOT NULL,
  priority    TEXT NOT NULL CHECK (priority IN ('urgent','normal','low')),
  text        TEXT NOT NULL,
  url         TEXT,
  project_id  TEXT,
  thread_id   TEXT,
  created_at  TEXT NOT NULL,
  raised_at   TEXT,
  reminded_at TEXT,
  resolved_at TEXT
) STRICT;
CREATE INDEX events_open ON events(resolved_at, priority, id);`,
	`CREATE TABLE summaries (
  session_file TEXT NOT NULL,
  entry_id     TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('compact','handoff')),
  cwd          TEXT NOT NULL,
  at           INTEGER NOT NULL,
  summary      TEXT NOT NULL,
  sent         INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (session_file, entry_id)
) STRICT;
CREATE INDEX summaries_unsent ON summaries(sent) WHERE sent = 0;`,
	// v7 (0440 (4)): the project a summary belongs to, kept once known.
	"ALTER TABLE summaries ADD COLUMN project_id TEXT;",
	// v8 (0547): v3's thread_runtime goes; nothing read or wrote it.
	"DROP TABLE thread_runtime;",
];

/** `me.db` migrations (appendix I ledger): v1 the inbox (0302): notes for a project's next work and timed reminders. */
const ME_MIGRATIONS: readonly string[] = [
	`CREATE TABLE inbox (
  id         INTEGER PRIMARY KEY,
  text       TEXT NOT NULL,
  project_id TEXT,
  due_at     TEXT,
  created_at TEXT NOT NULL,
  done_at    TEXT,
  CHECK (project_id IS NOT NULL OR due_at IS NOT NULL)
) STRICT;`,
];

/** `<data>/me.db` (appendix I ledger). */
export function openMeDb(): Database {
	return openDb(join(dataDir(), "me.db"), ME_MIGRATIONS);
}

export function openCatalog(): Database {
	return openDb(catalogPath(), CATALOG_MIGRATIONS);
}
