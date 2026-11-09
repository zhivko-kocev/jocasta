# 0184 Prompt-cache retention

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Which prompt-cache lifetime does Jarvis request?

## Facts
Verified 2026-10-07 in the Pi v1.0.1 clone: retention defaults to `short` (Anthropic's 5-minute lifetime); `ttl: "1h"` is sent only for `long`, which comes only from `PI_CACHE_RETENTION=long` or a per-request option; no settings key exists (`packages/ai/src/api/anthropic-messages.ts:66-93`, `packages/coding-agent/src/core/cache-warmer.ts:40-44`).
- What changed (0004): appendix G expected subscription sign-in might already get 1-hour entries; it doesn't.
- Warming is `streaming` (0170). A 1-hour cache write costs more than a 5-minute one.
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's default (5 minutes) with warming | Cheapest writes; warming covers active work | A long pause re-writes the cache |
| Long (1 hour) through the entry | Breaks don't lose the cache | Every write costs more |

## Recommendation
None was marked in the question.

## Choice
Pi's default, 5 minutes, with warming (0170); `PI_CACHE_RETENTION` is not set.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
