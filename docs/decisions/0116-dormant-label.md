# 0116 Dormant label

- Status: dropped
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
When is a thread labeled dormant?

## Facts
- Plan 1.4 (v4): after 14 days idle.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| After 14 idle days, as a label only | Old threads stand out | — |
| His number, or none | — | — |

## Recommendation
After 14 idle days.

## Choice
Now (0307): dropped.

Originally: A thread idle for 14 days is labeled dormant in lists and the sidebar; nothing is closed or deleted.

## Reason
Not given.

## Consequences
- A label, not a state change.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
