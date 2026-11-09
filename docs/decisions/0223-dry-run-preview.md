# 0223 Dry-run preview in approvals

- Status: decided
- Kind: product
- Phase/slice: 2/2.4
- Date: 2026-10-08

## Question
Is the dry-run preview row (0081) in v1?

## Facts
- The row: show what a risky command would delete, discard or overwrite, from the tools' own dry runs (`git clean -n`, `git status --porcelain`, file counts) (0081).
- Destructive git commands are denied in every mode (0210); asks remain for recursive deletes of tracked files inside the project (0213), `branch -D`, `push --delete` and force push with lease (0218).
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | He sees what an ask would destroy | Dry-run commands per rule |
| After v1 | — | — |
| Dropped | — | — |

## Recommendation
None was marked in the question.

## Choice
Now (0307): after v1.

Originally: v1: the approval overlay shows what an ask for a destructive command would remove or discard, measured with the tools' own dry runs.

## Reason
Not given.

## Consequences
- Preview: `would remove: 42 files (3 with uncommitted changes)`.
- Appendix A's dry-run row is v1.
- Amended by 0307: after v1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
