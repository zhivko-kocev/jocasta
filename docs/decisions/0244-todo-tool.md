# 0244 Todo tool

- Status: decided
- Kind: product
- Phase/slice: 3/3.2
- Date: 2026-10-08

## Question
Which todo tool does Jarvis have? (fresh agenda of one question, 0088)

## Facts
Verified 2026-10-08: Pi has no built-in todo; the installed package ships `examples/extensions/todo.ts` (297 lines, MIT), which keeps the list in tool-result details so branches stay correct, and adds `/todos`.
- omp's todo has phases and init/done operations; his usage: 553 calls, done 477, init 34 (appendix A).

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's example plus bulk init and done | Small; state lives in the session | No phases |
| omp-style with phases | What he used | Bigger |
| A Pi package from another developer | No code | Fit unknown |

## Recommendation
Pi's example plus init and done.

## Choice
Pi's `todo` example adapted with a bulk init (replace the list) and marking items done; state stays in the session, and `/todos` shows it.

## Reason
Not given.

## Consequences
- Slice 3.2 is decided.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
