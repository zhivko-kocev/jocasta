# 0140 Exit codes

- Status: decided
- Kind: product
- Phase/slice: 1/1.7
- Date: 2026-10-07

## Question
Which exit codes does Jarvis use?

## Facts
Verified 2026-10-07 in Pi 1.0.1:
- Print and JSON modes return 0, or 1 when the last assistant message errored or was aborted, or the run threw (`dist/modes/print-mode.js:17,117,128`); SIGTERM exits 143, and SIGHUP 129 off Windows (`:32-40`).
- `main()` sets `process.exitCode` from that return instead of exiting (`dist/main.js:811-821`), so the entry can set its own code afterwards.
- 0101 uses 1 for DB failures. Phase 2 needs "approval needed and denied headless"; phase 4 needs "cancelled by the boss".
- What changed (0004): the wrapper the v4 set needs is a few lines after `main()` returns.

## Options
| Option | Pros | Cons |
|---|---|---|
| v4's set: 0 done, 1 run failed, 2 invocation error, 3 approval needed and denied headless, 4 cancelled by the boss, 130 interrupted | The boss and scripts can tell why a run stopped | A small wrapper |
| Pi's codes: 0, 1, 143 | Nothing to build | A denied approval looks like a crash |

## Recommendation
v4's set.

## Choice
Now (0246): exit 3 also means a question is waiting for his answer.

Originally:
0 done, 1 the run failed, 2 invocation error, 3 an approval was needed and denied headless (phase 2), 4 cancelled by the boss (phase 4), 130 interrupted. The entry sets them after `main()` returns.

## Reason
Not given.

## Consequences
- Appendix I's exit-code table is decided.
- DB failures exit 1 (0101).
- Amended by 0246: exit 3 also covers a waiting question.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
