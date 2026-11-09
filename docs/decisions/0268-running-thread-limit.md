# 0268 How many threads run at once

- Status: decided
- Kind: product
- Phase/slice: 4/4.5
- Date: 2026-10-08

## Question
How many threads may run at once?

## Facts
Read 2026-10-08 in firstmate (commit b062eb9): per-project capacity in `config/project-capacity`, and a concurrent-worker cap while away (`bin/fm-spawn.sh:1646-1656`).
- Subscription rate limits apply across all threads.

## Options
| Option | Pros | Cons |
|---|---|---|
| 3 | Gentle on limits | Waits sooner |
| 5 | More parallel work | Hits rate limits sooner |
| No limit | — | Rate limits decide |

## Recommendation
3.

## Choice
5 at once; more wait as `queued`. The number is `frontier.maxRunningThreads` in `settings.json`.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
