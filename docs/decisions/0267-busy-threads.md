# 0267 Work sent to a busy thread

- Status: decided
- Kind: product
- Phase/slice: 4/4.5
- Date: 2026-10-08

## Question
What happens when a busy thread is sent new work, and can two runs share a folder?

## Facts
- Re-asked in plain words after "i dont understand the second statement" (0266).
- Pi queues messages typed while an agent works and delivers them as steering or follow-ups (appendix A's steering row).
- Each workspace thread has its own worktree (0118).

## Options
| Option | Pros | Cons |
|---|---|---|
| Messages to a busy thread are steering; new tasks get a new workspace thread with its own worktree | No folder has two runs; no waiting | More threads |
| Queue until it finishes | One thread | Waits |
| Refuse while busy | Simplest | He retries |

## Recommendation
Steering plus a new thread.

## Choice
A message to a busy thread joins it as steering; a new task gets a new workspace thread with its own worktree, so no folder ever has two runs.

## Reason
Not given.

## Consequences
- Slice 4.5 is decided.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
