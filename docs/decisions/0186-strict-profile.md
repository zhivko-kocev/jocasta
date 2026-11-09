# 0186 Strict profile

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
What does the strict profile pin?

## Facts
- The strict profile is for reproducible runs: phase-end evals (0169) and runs to compare (R15).
- Pi 1.0.1 has model fallback only for Anthropic models whose compat lists `allowedFallbackModels` (`packages/ai/src/api/anthropic-messages.ts:1282-1284`); account switching (6.2) and background summaries (3.4) are Jarvis's.
- His first answer left every item unticked; asked again under 0020.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pins all five: no model fallback, no account switching, no background summaries, fixed effort, no cache warming | Comparable eval runs | A run fails instead of falling back |
| Pins four; cache warming stays | Cheaper repeated turns | Warming refreshes cost money during evals |
| No strict profile | One less profile | Scores may move for unrelated reasons |

## Recommendation
Pins all five.

## Choice
Now (0313): the file is seeded once as `defaultThinkingLevel: high`, `modelFallback: false`, `providers.rotateAccounts: false`; Pi has no background summaries to pin.

Originally: Free text: "strict pins four, warming stays but also other profiles can exist right not only strict ?" The strict profile pins no model fallback, no account switching, no background summaries and a fixed effort; cache warming stays as set (0170). Other profiles can exist beside it.

## Reason
"strict pins four, warming stays but also other profiles can exist right not only strict ?"

## Consequences
- `profiles/strict.yaml` ships with these four deltas; its run budgets are decided separately.
- Any number of profile files can exist (0185): the boss and worker profiles come in 4.11, and he can add his own.
- Differs from the recommendation, which also turned warming off.
- Amended by 0313: contents and shipping set by 0313.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
