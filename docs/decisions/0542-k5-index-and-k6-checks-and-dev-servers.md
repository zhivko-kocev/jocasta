# 0542 K5 and K6: a shared code index behind graph_search, cached service-side checks, and dev servers, each tried and kept if it measures better

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V8, V13
- Date: 2026-11-08

## Question
The harness-OS layer (0532) proposed a shared code index (K5) and a shared test runner with dev servers owned outside the threads (K6). What is tried, how does it reach threads, and when does it stay?

## Facts
Verified 2026-11-08.
- omp 18.18.2's `lsp.shared` (default on) starts language servers through the project's daemon broker: "Each connection gets its own server. While another omp instance in the project is running, a released server stays warm and the next connection reuses it" (`lsp/settings.ts:33-43`). omp's `find` is keyword search re-ranked by a model, off in the pane overlay (`find.enabled: off`).
- codebase-memory-mcp (github.com/DeusData/codebase-memory-mcp, MIT, one C binary, v0.12.1 of 2026-10-16, Windows supported): a parser-built graph of symbols, calls and imports, one daemon per user serving any number of clients; it keys projects by path, so worktrees need seeding from a snapshot (documented for clones, not worktrees). Its size and memory on his repos are unknown.
- Graphiti holds model-extracted facts with time windows, sealed per project (H4); the code graph changes on every edit and needs no model.
- The Netflix and JetBrains study measured 20–30% efficiency from deterministic code intelligence (2026-10).
- Agents run tests in bash; Jocasta runs the project's check in the review and `done_when` (0540); 0513 chose a check on idle after v1 (failures only, silent on pass).
- mise's task cache (experimental) keys a check by its sources' contents and shares entries across worktrees, with no single-flight for concurrent identical runs.
- omp's hub supervises background processes per project folder (`omp ps`: names, logs, restart policies, reuse across omp instances); pitchfork, through mise `[daemons]` (experimental), gives each worktree its own ports and URL and waits for readiness, declared in each project's `mise.toml`.

## Options
| Item | Options (pros / cons) |
|---|---|
| K5 | Spike codebase-memory-mcp, then measure (fewer exploration calls / an install, unproven on worktrees) · omp's LSP only (nothing new / position queries only, servers per thread) |
| Index and Graphiti | Separate tools · One front door through `graph_search` (one tool; answers merged, stores apart) · Links after v1 |
| K6 checks | Service-side checks, cached · Also a thread `check` tool · Nothing new |
| K6 dev servers | omp's own daemons (nothing to install) · pitchfork through mise (ports per worktree / an install, experimental, his own projects only) |

## Recommendation
Spike the index, then measure; separate tools; service-side checks, cached; omp's own daemons.

## Choice
- **K5:** codebase-memory-mcp is installed for a spike on Windows: worktree seeding from a snapshot at the merge-base commit, index size and daemon memory on his repos, and no config or hook rewrites (`--skip-config --no-hooks`). If it passes, threads get it and the benchmark compares threads with and without it; it stays if it does better than omp's LSP alone.
- **One front door:** threads don't get the index's own tools. `graph_search` keeps its name and answers with Graphiti's facts plus the index's code locations (paths and lines, not code bodies), capped at 10 of each. The two stores stay apart; an arm without the index is `graph_search` with facts only.
- **K6 checks:** Jocasta runs the review's check, `done_when` and the check on idle (0513's, now v1) through mise's task cache with its own single-flight by git tree; threads get failures only, with the log's path, and nothing on a pass. A `check` thread tool, for threads to call instead of running tests in bash, is tried too and kept if it measures better. mise's cache is spiked on Windows first.
- **K6 dev servers:** omp's own daemons and pitchfork are both tried; pitchfork is used if it works better.

## Reason
"i understand now and i love it we can try it and if it works better than the lsp use it"; on the index and Graphiti, "one front door is better but also what is the difference are they both knowledge graphs and would they bloat a lot"; "recommended + try a thread pool if it works we use it" (read as the `check` thread tool); "lets try both and if pitchfork works better use it".

## Consequences
- Differs from the recommendation: one front door; the `check` tool and pitchfork are tried.
- Installs approved for spikes: codebase-memory-mcp, and pitchfork through mise (0385's list gains them); mise's experimental task cache and daemons are switched on for the spikes.
- Amends 0513: the check on idle moves into v1.
- `graph_search`'s description and reply change; the code part is sealed per project like the facts (H4 covers it).
- What "works better" means for K5, the `check` tool and pitchfork is set with the arm set (a later record): cost per passing task, paired.
- Amended by 0546: checks answer as gates (PASS/FAIL per check, failures, log path, the narrowest rerun), silent when all pass.
- Amended by 0559: Jocasta keys check results by git tree (PASS and FAIL), runs one check per key, and uses mise's artifact cache underneath. Amended by 0560: pitchfork isn't kept.
- Amended by 0562: the code index is measured on each project's main checkout only, behind `graph_search` for relationship questions.
- Amended by 0571: no arm switches the `check` tool off; it stays, and cached checks are measured in H8.
