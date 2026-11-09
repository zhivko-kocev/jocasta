# 0494 A plan that awaited him is shown again after a restart

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
After Stop everything and a reopen, a thread whose plan awaited his approval comes back in plan mode without omp's Plan Review. How does he get the Approve/Refine picker back?

## Facts
Verified 2026-10-24 in the campaign's beta day (0481) and omp 18.12.6's source.
- omp keeps an open Plan Review in memory only (`modes/interactive-mode.ts:1688, 6910`); on `--continue` it re-enters plan mode with the plan file (`#reconcileModeFromSession`, `:5126-5128`) but never reopens the review.
- `/plan-review` (`slash-commands/builtin-modes.ts:348-358`) opens the newest `local://<slug>-plan.md` in the same picker and works across restarts (`interactive-mode.ts:6730-6736`).
- The campaign's fixes already keep such a thread counted as awaiting him after a reopen (the HUD, the rail's Awaiting-you row, the close question).
- Typing into a pane while omp's picker is open picks its default, "Approve and execute" (the beta day's blocker, B1).

## Options
| Option | Pros | Cons |
|---|---|---|
| Jocasta reopens it for him | The same picker is back without him doing anything | Jocasta types into the thread's pane |
| He types `/plan-review` | Nothing typed for him | He has to know the command |
| Leave it | No change | The picker is gone; he decides in the pane by hand |

## Recommendation
Jocasta reopens it for him.

## Choice
When a reopened thread's session shows a plan awaiting him, Jocasta types omp's `/plan-review` into its pane once, after its omp is idle, so the same picker shows again. It types only while no picker or question is on the pane's screen.

## Reason
"Jocasta reopens it for you".

## Consequences
- Amends 0434 (threads resume where they were, a waiting plan included).
- Appendix I's thread paragraph names the command and its guard.
