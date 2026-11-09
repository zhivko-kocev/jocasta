# 0096 DB files

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
How many SQLite files does Jarvis keep?

## Facts
Verified 2026-10-07.
- Section 7: one writer per thread. SQLite in WAL mode allows one writer per file at a time, and readers don't block it (L66; WAL ran in a compiled Windows binary today, 0090).
- Phase 4 runs many tabs, each a harness process, over the same data; phase 5 adds memory DBs.

## Options
| Option | Pros | Cons |
|---|---|---|
| `catalog.db` plus one `project.db` per project, with memory DBs in phase 5 | Tabs on different projects never share a write lock; a project is one file to archive; a corrupt project DB leaves the catalog alone | Cross-project queries open many files; migrations run per file |
| One DB | One migration path; joins across projects | Every tab shares one write lock; one corrupt file loses everything |

## Recommendation
`catalog.db` plus one `project.db` per project.

## Choice
`<data>/catalog.db` plus `<data>/projects/<pid>/project.db`, with memory DBs arriving in phase 5.

## Reason
Not given.

## Consequences
- Migrations run per file; each file tracks its own `PRAGMA user_version`.
