# 0456 What the dashboard and HUD show about threads and quota

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
What do the HUD, the dashboard and the thread tools add about quota, waiting reviews, overlapping threads and leftover worktrees?

## Facts
Verified 2026-10-23.
- `omp usage --json` exists in 18.12.6 and returns `reports[].limits[]` with `window.label` ("5 Hour", "7 Day"), `amount.usedFraction`, `window.resetsAt` and `status`; nothing in Jarvis reads quota (no `omp usage` in `src`).
- The HUD is "N at work · N awaiting you · $X today" (`service.ts:1068`); he said budgets don't fit a subscription (0187), so the 5-hour and weekly windows are the real limit.
- "Awaiting you" drops a reviewed, unfinished thread after a day and labels it "landed" (`day.ts:678-707`).
- `thread_status` prints subject, branch and state, with no files (`service.ts:1651-1666`).
- A thread closed in herdr keeps its worktree and branch (`service.ts:661-666`), and nothing lists them; auto-cleanup destroyed unpushed work in Claude Code (https://github.com/anthropics/claude-code/issues/79899).

## Options
| Option | Pros | Cons |
|---|---|---|
| All four | Quota, waiting reviews, overlaps and leftovers where he looks | Four small changes |
| Quota only | Smallest | Reviews still fall off after a day |
| None | No change | The limits and waiting work stay hidden |

## Recommendation
All four.

## Choice
1. The HUD (`<data>/hud.txt`) adds the subscription windows `5h N% · 7d N%` from `omp usage --json` (exists in 18.12.6), with one toast when a window passes a threshold (builder's default).
2. "Awaiting you" lists reviewed, unfinished threads oldest first with how long they've waited (today `day.ts:678-707` drops them after a day and labels them "landed").
3. `thread_status` and the `thread_finish` reply name the open threads that touch the same files.
4. The dashboard lists leftover worktrees (closed in herdr, still on disk) with dirty and ahead counts; they are removed only on his word, never on a timer.

## Reason
"All four".

## Consequences
- Amends 0429 (the dashboard's rows) and 0437 (the HUD's text).
- The threshold, toast and row texts are chosen by the builder and listed as built.
- No WIP cap on reviews.
- Amended by 0554: the subscription windows move from the HUD into `today`.
