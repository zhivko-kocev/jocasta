# 0176 Sampling parameters

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Are sampling parameters set?

## Facts
Verified 2026-10-07 in the Pi v1.0.1 clone: Pi sends temperature to Anthropic only when it is set and the model supports it without thinking (`packages/ai/src/api/anthropic-messages.ts:912-917`); Jarvis thinks at `high` (0056).
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Leave them unset | Nothing looks deterministic that isn't | — |
| Set them where a provider accepts them | — | Only looks deterministic |

## Recommendation
Leave unset.

## Choice
Sampling parameters stay unset.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
