# 0182 Thinking level

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
How is the thinking level set?

## Facts
Verified 2026-10-07: Pi 1.0.1's levels are `off`, `minimal`, `low`, `medium`, `high`, `xhigh` and `max`, with no automatic or per-turn classifier (`packages/ai/src/types.ts:85-87`); `defaultThinkingLevel` and per-model `modelThinkingLevels` live in `settings.json`. Jarvis seeds `high` (0056).
- Roles arrive in 4.11 and 6.2.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pinned per role, never automatic | Repeatable effort (principle 2) | An easy turn pays the role's level |
| Automatic where offered | Cheaper easy turns | Pi has none; effort would vary with phrasing |

## Recommendation
Pinned per role.

## Choice
Pinned per role and profile; never automatic.

## Reason
Not given.

## Consequences
- 4.11 and 6.2 set each role's level.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
