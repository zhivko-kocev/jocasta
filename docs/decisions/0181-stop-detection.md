# 0181 Stop detection

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
How does a run decide it is done?

## Facts
Verified 2026-10-07: Pi 1.0.1 ends turns on its stop reasons `stop`, `toolUse`, `length`, `error` and `aborted` (`packages/agent/src/agent-loop.ts:245,263-270`).
- Jarvis adds the loop guard (0152) and run budgets (0080).
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Mechanical rules only | Repeatable | — |
| A model verdict | — | Varies with phrasing |

## Recommendation
Mechanical rules only.

## Choice
Mechanical rules only: Pi's stop reasons, the loop guard and run budgets; never a model verdict.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
