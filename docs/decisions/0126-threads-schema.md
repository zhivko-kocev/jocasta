# 0126 The threads schema

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What does the `threads` table hold?

## Facts
- One `project.db` per project (0096); `STRICT` tables and RFC3339 timestamps (0100).
- Dormant is computed from `last_active_at` (0116).

## Options
| Option | Pros | Cons |
|---|---|---|
| The previewed table | Every 1.4 decision has a column; nothing speculative | — |
| Change it | — | — |

## Recommendation
The previewed table.

## Choice
```sql
CREATE TABLE threads (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('project','workspace')),
  parent_id      TEXT REFERENCES threads(id),
  title          TEXT NOT NULL,
  brief          TEXT,
  branch         TEXT,
  path           TEXT,
  state          TEXT NOT NULL CHECK (state IN ('open','closed')),
  created_at     TEXT NOT NULL,
  last_active_at TEXT NOT NULL,
  closed_at      TEXT
) STRICT;
```

## Reason
Not given.

## Consequences
- `project.db` v1 holds this table; `catalog.db` v1 holds the same columns for the home thread, with `kind` `home` (0125).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
