# 0547 A thread's state comes from omp first, and the unused thread_runtime table is dropped

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-08

## Question
The Harness Playbook's first rule is one authoritative session. Where does Jocasta take a thread's state from?

## Facts
Verified 2026-11-08.
- Jocasta keeps each thread's shown state (`thread.shown`: idle, working, blocked, done) in memory, set from herdr's agent-state changes (`src/frontier/server/service.ts:1809-1816`, `:1872`) and from omp's session file: a proposed plan marks it blocked after a 2 s settle, since omp writes the plan through a buffered writer (`service.ts:414-418`, `:1897-1899`); the rail and `today` read it (`DayThread`).
- herdr's own sidebar shows herdr's state: herdr 0.9.4 ignores a second source's state while omp's integration owns a pane, so a plan waiting for approval stays green "idle" there, labelled "plan ready" (0433, herdr `src/terminal/state.rs:745-754`). Nothing Jocasta does changes that sidebar.
- `omp collab list --json` reports `busy` (a turn in flight) and `inputRequired` (a host dialog waiting for a writable guest) from omp itself; with 0536's local relay every thread publishes it. That omp's plan review counts as such a dialog is [INFERENCE] until 0536's spike checks it.
- `catalog.db`'s `thread_runtime` table is created by a migration (`src/shared/schema.ts:48`) and nothing in `src/` reads or writes it.
- The playbook: "Rewind, fork, resume, replication, and inspection must all derive from the same journaled state"; "views are projections".

## Options
| Option | Pros | Cons |
|---|---|---|
| omp first, drop the dead table | Jocasta's working and asking state from omp itself, not inferred from herdr's events and a settle timer; no dead schema | A migration; herdr's sidebar still shows herdr's own state |
| omp first, keep the table | No migration | Dead schema stays |
| As today | Nothing changes | State inferred from herdr's events and the settle timer |

## Recommendation
omp first, drop the dead table.

## Choice
- Working or asking: from `omp collab list` (`busy`, `inputRequired`).
- Plan waiting, review, test results, the report: from omp's session file along its active branch (0509).
- Panes, focus and whether omp runs at all: from herdr.
- A new `catalog.db` migration drops `thread_runtime`.

## Reason
"omp first, drop the dead table".

## Consequences
- Plan appendix I's migration ledger gains the drop when it's built.
- If 0536's relay is down, state falls back to herdr's events and the session file, as today.
- herdr's sidebar is unchanged (0433 stands).
- Amended by 0561: omp's state is each host's pipe snapshot; a pane without a registry entry is unknown, never idle.
