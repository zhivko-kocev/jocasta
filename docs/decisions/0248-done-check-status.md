# 0248 Done check status

- Status: decided
- Kind: product
- Phase/slice: 3/3.7
- Date: 2026-10-08

## Question
Is the catalog's done check row (0082) in v1?

## Facts
- The row: a command in the brief must pass before a workspace thread reports done; failures go back to the thread up to a set number of attempts (Strands GoalLoop).
- Public benchmarks run their own tests; the check matters for his own tasks. Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| After v1 | Smaller | Threads can report done with failing tests |
| v1 | Catches false "done" | More code |
| Dropped | — | — |

## Recommendation
After v1.

## Choice
After v1; it returns at the pruning pass if the evals show threads reporting done with failing checks.

## Reason
Not given.

## Consequences
- Slice 3.7 is decided, and with it phase 3.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
