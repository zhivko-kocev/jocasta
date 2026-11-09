# 0388 Threads are herdr worktrees

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What is a thread when the harness is omp and the surface is herdr?

## Facts
Verified 2026-10-13.
- herdr (`herdrdev/herdr`) 0.9.4 (2026-10-13), Apache-2.0, Rust; Windows is GA through ConPTY, `herdr terminal attach` is unsupported there. It has an NDJSON socket API on a named pipe (`workspace.*`, `tab.create`, `pane.*`, `agent.*`, `worktree.*`, `notification.show`, `events.subscribe`), env `HERDR_SESSION` and `HERDR_CONFIG_PATH`, and `herdr integration install` supports OMP on Windows.
- omp stores sessions per cwd (`sessionDirForCwd`), format v3 with compaction entries; `-c/--continue` continues the newest session there; `/handoff` and `/compact` carry long work.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr worktree + `omp --continue` | One session line per worktree; no thread DB; herdr's omp integration reports states | Jarvis's thread records and report tool retire |
| Keep Jarvis's thread model on omp | Keeps the thread table and `report` | Jarvis code inside omp (against 0386) |

## Recommendation
herdr worktree + `omp --continue`.

## Choice
A workspace thread is a herdr tab on a git worktree running `omp --continue` there (one session line per worktree); the project thread is a pane in the main checkout opened in plan mode; longevity is omp's compaction and `/handoff`. Thread state comes from herdr's omp integration. Jarvis's thread tables, `report` tool and harness exit codes retire; old Pi sessions and DB rows stay on disk, unmigrated.

## Reason
"maybe i can simulate long threads with one session per worktree and the longevity of the thread via the /handoff or /compact".

## Consequences
- The chat-only trunk is replaced by plan mode in the project pane (0391).
- Appendix I's harness exit-code table retires.
- Amended by 0412: a thread is its own herdr workspace from `worktree.create`, grouped under the project.
