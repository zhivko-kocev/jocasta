# 0170 Cache warming

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
Does Jarvis warm Anthropic's prompt cache?

## Facts
Verified 2026-10-07 in Pi 1.0.1:
- Modes are `off`, `streaming` and `idle`; with nothing set, Pi uses `streaming` (`dist/core/settings-manager.js:679-681`). Jarvis's seeded settings (0056) don't set it, so warming is on today.
- A refresh is a cache read of the prompt plus one output token, sent at 90% of the cache lifetime when the expected savings are at least $0.05 (`dist/core/cache-warmer.d.ts:5-37`).
- On the subscription each refresh bills as extra usage (0062). Appendix G recommended off.

## Options
| Option | Pros | Cons |
|---|---|---|
| Off, seeded into `settings.json` | No unasked spend | A long pause costs a full cache write |
| `streaming` (Pi's default) | Keeps the cache warm around active runs, only when Pi expects savings | Spends while he isn't typing |
| `idle` | Warmest cache | Most spend |

## Recommendation
Off.

## Choice
`streaming`, Pi's default; nothing is seeded.

## Reason
Not given.

## Consequences
- Appendix G's cache-warming lever is decided as `streaming`.
- Differs from the recommendation, which was off.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
