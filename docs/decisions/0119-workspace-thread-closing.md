# 0119 Closing a workspace thread

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
When does a workspace thread close, and what does closing do?

## Facts
Probed 2026-10-07 with git 2.54.0 in a temp dir.
- After a squash merge, `git branch --merged main` did not list the branch; `git cherry` matched a one-commit squash, and can't match a multi-commit one.
- A deleted branch is certain. PR merge and close signals arrive with phase 8's pollers (appendix I). `thread close` is a phase 3 command.

## Options
| Option | Pros | Cons |
|---|---|---|
| Signals propose and he confirms: merged (including patch-equivalent), deleted, or PR closed; closing sends the final report to the project thread, makes the thread read-only and searchable, and handles the worktree per item 10 | A false "merged" never hides live work | One confirmation per thread |
| Auto-close on a signal | Zero clicks | Squash-merge detection is unreliable |
| Manual only | No detection | Merged threads linger |

## Recommendation
Signals propose, he confirms.

## Choice
Jarvis detects a merged branch (`--merged` or patch-equivalent through `git cherry`), a deleted branch, or a closed PR (phase 8), and asks in the project thread whether to close. Closing sends the thread's final report to the project thread and leaves the thread read-only and searchable; the worktree follows item 10.

## Reason
Not given.

## Consequences
- The close prompt, as previewed: `Add retry (t-9f3e2a1b): feat/add-retry is merged into main. Close it? [close] [keep]`.
- Phase 3's `thread close` and its hooks run the same close.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
