# 0318 Real eval runs

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-08

## Question
How many reps and how long per trial for real-provider eval runs?

## Facts
- Real runs cost tokens, need `JARVIS_EVALS_REAL` and never run in CI (0169). 0163 sizes reps to the noise floor; nothing is measured yet. No per-trial time limit existed.
- He asked why; it was explained.

## Options
| Option | Pros | Cons |
|---|---|---|
| 3 reps, 10 minutes per trial, `--reps` and `--timeout` flags | Luck shows; stuck trials stop | 3× the cost |
| 1 rep, no limit | Cheapest | Noisy; a stuck trial runs on |
| 5 reps, 20 minutes | Steadier | Costlier |

## Recommendation
3 reps, 10 minutes.

## Choice
3 reps and 10 minutes per trial by default; `bun run evals --reps <n> --timeout <minutes>` changes them.

## Reason
"and why is this"

## Consequences
- Scripted runs keep their own rep count.
