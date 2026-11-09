# 0145 Slice 1.12 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Which decisions does slice 1.12 (determinism, cost, strict profile) grill, now that the omp plan's C1, C2 and B11 are decided fresh (0088)?

## Facts
- Appendix G assigns to 1.12: sampling parameters, thinking level (with 6.2), tool-call format, drifted edits, stop detection, fabricated tool results, speculative tool execution, prompt-cache retention, the workspace tree, model fallback in the strict profile, and run budgets (0080).
- Plan 1.12 also lists the profiles mechanism and the strict profile.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of twelve: the nine levers, the profiles mechanism, the strict profile with model fallback, and run budgets | Every lever assigned to 1.12 is asked once, with what Pi 1.0.1 already does | Twelve questions, many tick-listable |
| The plan's ten | Fewer | Run budgets and fabricated results wait |

## Recommendation
The fresh agenda of twelve.

## Choice
The fresh agenda of twelve.

## Reason
Not given.

## Consequences
- Plan 1.12's list becomes the twelve items, citing this record.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
