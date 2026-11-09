# 0452 The day's spend counts every model call

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
"$ today" (0433) sums assistant messages only. Does it count omp's side calls?

## Facts
Verified 2026-10-23.
- omp writes side calls (memory extraction, cache warming, advisor, judges) as `model_usage` entries, not assistant messages (`session/session-entries.ts:105`); omp's own session manager counts `model_usage.usage.cost.total` (`omp-src/session/session-manager.ts:414-415`).
- Jarvis sums only assistant `usage.cost.total` (`src/frontier/server/sessions.ts:152-160`).
- `@oh-my-pi/omp-stats` (installed) exports `syncAllSessions()`/`getDashboardStats()` over session JSONL, with its own `~/.omp/stats.db` (omp `user-facing-packages.md` §packages/stats).

## Options
| Option | Pros | Cons |
|---|---|---|
| Count assistant turns and `model_usage` | Matches what omp bills | Cost code changes |
| Read omp-stats only | No cost code in Jarvis | Its totals may not match per session; writes its own database |
| Keep assistant turns only | No change | Undercounts memory, cache warming, advisor and judges |

## Recommendation
Count assistant turns and `model_usage`.

## Choice
"$ today" counts assistant turns and `model_usage` entries. Jarvis reads the totals from `@oh-my-pi/omp-stats` when its per-session totals include side calls and match; otherwise Jarvis sums `model_usage` itself and its own cost code stays (the builder checks and records which in CHANGELOG).

## Reason
"Count assistant turns and `model_usage`".

## Consequences
- Amends 0433 (what the figure sums).
- The source used (omp-stats or Jarvis's own sum) is recorded in CHANGELOG when built.
