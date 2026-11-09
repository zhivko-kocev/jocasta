# 0380 Review the diff after every task

- Status: v1
- Kind: feature
- Phase/slice: 4/4.3, 7/7.3
- Date: 2026-10-10

## Question
Does Frontier show a thread's diff after each task?

## Facts
Verified 2026-10-10.
- Frontier has no diff view.
- Whiteboard: a merge-base diff in VS Code's diff editor, files in reading order, a viewed tick, tours over diff ranges. opencode: `@pierre/diffs` with shiki in workers, virtualized, ←/→ between files, files over 500 changed lines rendered only on request. Codex `/diff` includes untracked files.

## Options
| Option | Pros | Cons |
|---|---|---|
| A diff review after every task, built with code review (D) | He sees what each thread changed, with the reviewer's findings on their lines | More Frontier surface |
| No diff view | Less to build | He reads diffs elsewhere |

## Recommendation
Build it with D.

## Choice
v1: a review of the diff after every task, "good smooth and watchable", built together with code review (D). Its details are grilled at R.

## Reason
"good smooth and watchable".

## Consequences
- R's grilling decides the data path, the UI, the Play tour, triggers and limits.
- Amended by 0394: the review is a hunk pane beside the finished thread with omp's reviewer notes.
