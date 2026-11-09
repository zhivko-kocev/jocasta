# 0422 No prompt diet: omp's defaults

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice A6 (0375): trim what omp sends per request?

## Facts
Verified 2026-10-14.
- Measured on omp 18.10.4 with "reply ok": 12,855 input tokens per request with 11 tools; the built-in tools cost about 6.1K; skills and rules about 190; read/bash/edit/write only gives 8,167.
- His config has the advisor on, a second model call per turn.
- billion-context is gone (0386).

## Options
| Option | Pros | Cons |
|---|---|---|
| Trim one variant at a time, keep by the rule | Measured savings | Eval runs, more config |
| Keep omp's defaults | Nothing to maintain | The load stays |
| read/bash/edit/write only | Smallest prompt | Loses most tools |

## Recommendation
Trim one variant at a time.

## Choice
No diet: threads and the boss run with omp's tools and defaults as he sets them.

## Reason
"Keep omp's defaults".

## Consequences
- Differs from the recommendation.
- Closes slice A6 of 0375.
