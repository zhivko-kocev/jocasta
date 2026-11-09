# 0582 Starting, recovering and counting cost

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3, V9, V10
- Date: 2026-11-09

## Question
The review found that a failed start waits 90 s and attaches anyway, projects.yaml errors name no file or line, a hung-service takeover kills herdr and every pane, a thread cut mid-step stays stuck, and "$ today" and the thread figures miss or double-count cost; check results grow without bound. How does each behave?

## Facts
Verified 2026-11-09 by the review (0573), reports SHR, SRV-A (SRV-A-4), SVC (SVC-6), SRV-B and PERF (PERF-11).
- `startService(): void` gives `openSurface` no way to see the child exit; bare `jocasta` polls health for 90 s, then attaches herdr without the service (`src/shared/cli.ts:162-169`).
- A YAML error in projects.yaml prints Bun's source excerpt and a stack; `project add <a linked worktree>` exits with "was added … but not registered" and leaves an `add:` line every sync reinserts.
- A takeover runs `killTree(holder.pid)` (`instance.ts:112`), and the service is herdr's server's parent, so a spike's tree held the "herdr server" and its pane.
- After an omp restart mid-review, the thread reads "done — in review" and `thread_finish` says "still in its review turn" with nothing running.
- Graphiti's model calls are never recorded (`llm-endpoint.ts:151-168`); Jocasta's own `/handoff` is written as `{type: "compaction", method: "handoff"}` and counted as one; the sidebar shows the current session's cost while `thread_status` shows the ledger's ($0.42 vs $1.84).
- `<data>/checks/<tree>/` folders are never removed.

## Options
| Item | Options (pros / cons) |
|---|---|
| SHR-5 failed start | A. stop and exit 1 with the reason · B. say it, then attach (an empty surface) · C. keep the wait, point at diagnose (still 90 s) |
| SHR-6 projects.yaml | A. file:line errors, add the worktree's main checkout (it adds a path he didn't type, said) · B. refuse, naming the main checkout · C. wrap the parse error only |
| SRV-A-4 takeover | A. end the tree except herdr's server (threads keep working) · B. the whole tree, as decided · C. the service only (children linger) |
| SVC-6 cut steps | A. run the step again once omp is idle; a cut PR goes back to `task` · B. back to `task` always · C. `--restore` · D. keep |
| SRV-B-12 graph cost | (a) counted in "$ today" · (b) shown apart · (c) document it |
| SRV-B-14 handoff | (a) not counted · (b) counted · (c) only summarizing methods |
| SRV-B-18 thread cost | (a) the ledger's total everywhere · (b) label the session figure · (c) keep |
| PERF-11 check results | (a) keep everything · (b) remove folders untouched for 14 days · (c) a 2 GB cap |

## Recommendation
SHR-5 A; SHR-6 A; SRV-A-4 A; SVC-6 A with the PR case as B; SRV-B-12 (a); SRV-B-14 (a); SRV-B-18 (a); PERF-11 (b).

## Choice
- SHR-5 A: `startService()` reports its exit; `openSurface` races it against the health poll; on an early exit it prints the last ERROR line and a pointer to `jocasta diagnose`, then exits 1 (no herdr attach).
- SHR-6 A: projects.yaml goes through a typebox schema and `parseYamlFile`, errors as `<file>:<line>: <reason>` (UserError, exit 1); `project add <a linked worktree>` adds its main checkout, and the line says so: "added acme-web (p-…) at D:/acme-web (the main checkout of D:/acme-web.wt/feature)"; `add:` is written only once the path registers.
  As built: on Windows the paths are shown with its own separators ("at D:\acme-web (the main checkout of D:\acme-web.wt\feature)").
- SRV-A-4 A: a hung-service takeover ends the holder's process tree except herdr's server and what runs under it (the service records the herdr pid it started beside its lock; the kill uses the start-checked walk); the log says "…ending it (herdr's session keeps running) and starting".
- SVC-6 A: after an omp restart mid-step, once the new omp is idle: a cut review runs again on the same commits; a cut `/handoff` runs again; a cut PR turn goes back to `task` with the event "no PR: its omp started again mid-turn; everything stays, finish it again".
- SRV-B-12 (a): Graphiti's model calls count in "$ today": the LLM endpoint appends each call's usage (time, model, tokens) to `<data>/graph-usage.jsonl`, priced from omp's models.db; `spendToday` adds today's lines; the dashboard's activity names the graph share.
- SRV-B-14 (a): Jocasta's own `/handoff` (`method: "handoff"`) is not counted as a waste compaction, in the product's line and in the ledger's definition, and this is documented.
- SRV-B-18 (a): the sidebar and `today` show the thread's ledger total (cached per thread on the refresh), not the session figure.
- PERF-11 (b): once a day at service start, check result folders untouched for 14 days are removed; `hits.log` stays; one log line on days something was removed ("checks: removed 41 stored results older than 14 days (380 MB)").
  As built: the log's area prefix is the logger's (`INFO [checks] removed 41 stored results older than 14 days (380 MB)`); the day of the last pass is kept in `<data>/checks/pruned-on`, and a service running past midnight prunes daily.

## Reason
"A" for SHR-5, SHR-6, SRV-A-4 and SVC-6; "(a)" for SRV-B-12, SRV-B-14 and SRV-B-18; "(b)" for PERF-11. SRV-B-14 was asked again with a plain explanation at his request, then answered "(a)".

## Consequences
- Amends 0565 (the takeover spares herdr's server), 0431, 0438 (R3 for steps cut by a restart), 0452 ("$ today" includes the graph), 0463, 0538 and 0546 (the ledger's compaction count and one cost per thread), 0542 and 0559 (stored check results age out).
