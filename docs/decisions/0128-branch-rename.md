# 0128 Branch renamed

- Status: dropped
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What does a workspace thread do when its branch is renamed?

## Facts
Probed 2026-10-07 with git 2.54.0 in a temp dir: after `git branch -m`, `git worktree list --porcelain` showed the worktree on the new name, and the reflog recorded `Branch: renamed refs/heads/feat/add-retry to refs/heads/feat/retry-v2`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Read the branch from the worktree; for a reused checkout, follow the reflog's rename entry | Both are git's own records | A rename in another clone shows only as the old branch disappearing |
| Ask when the bound branch is gone | Never wrong | A question per rename |

## Recommendation
Worktree first, then reflog.

## Choice
Now (0307): dropped; a disappeared branch without a thread is still a close signal (0119).

Originally: A workspace thread reads its branch from its worktree; a thread on a reused checkout follows the reflog's rename entry. A branch that disappears without a rename entry is a close signal (0119).

## Reason
Not given.

## Consequences
- The `branch` column (0126) is updated when the thread opens.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
