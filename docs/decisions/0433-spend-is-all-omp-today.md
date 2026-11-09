# 0433 The day's spend is all omp spend

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
The HUD's "$ today" skipped subagents, the advisor and merged threads. What does the figure cover?

## Facts
Verified 2026-10-14.
- Fixed on 2026-10-14: a session's cost now includes its subagent and advisor transcripts, as `omp stats` counts.
- Counting every omp session of the day also includes his omp outside Jarvis, the boss and removed worktrees.
- herdr 0.9.4 ignores a second source's state while omp's integration owns a pane (`src/terminal/state.rs:745-754`), so a plan waiting for approval stays green "idle" in herdr's sidebar with the label "plan ready".

## Options
| Option | Pros | Cons |
|---|---|---|
| All omp spend today | Matches `omp stats` | Includes his own omp use |
| Only Jarvis's own sessions | Jarvis's cost only | A second number to explain |

## Recommendation
All omp spend today.

## Choice
The HUD's and the rail's "$ today" is every omp session's cost today, subagents and advisor included, wherever it ran. A plan waiting for approval keeps herdr's green colour with the label "plan ready" (no herdr fork for it).

## Reason
"All omp spend today"; plan colour: "Leave it".

## Consequences
- Amends 0403's figure; UX-03 is closed as accepted.
- Amended by 0452: "$ today" also counts `model_usage` entries (side calls).
