# 0541 K7–K10: omp already covers tool lists and models per phase; prewalk and keep-alive pings are measured; each thread warns at $50

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-08

## Question
The brainstorm's K7 (lean tool lists, pushed events), K8 (model and effort per phase), K9 (cache-aware scheduling) and K10 (per-thread budgets): what does Jocasta build, now that omp has most of them?

## Facts
Verified 2026-11-08 in omp 18.18.2's installed source and his config.
- `tools.xdev` (default on, docs `catalog`) mounts MCP and rarely used tools as `xd://` devices: the prompt lists one line per tool and the definition is read on demand (`tools/settings.ts:1013-1047`). Threads have two Jocasta tools (`graph_search`, `report_result`, 0539); the boss seventeen. Events reach the boss through herdr and the service's queue; the relay adds live ones (0536).
- His omp roles route by phase: `plan` Opus 5.5 at max, `default` Opus 5.5 at high, `task` Opus at medium, `slow` (the reviewer, 0488) Opus at max, `smol` Haiku 4.5. `--prewalk` switches to `smol` at the first edit after the plan's todo list exists.
- `cacheWarming: streaming` is set; Jocasta's own messages to a thread (brief, the report steer, review results) go when a turn ends, while the cache is warm; holding his messages would block him (V11).
- `--warn-cost <usd>` warns once in the TUI, `--max-cost <usd>` stops sending requests, `--max-time` ends the session; cost is omp's API-equivalent figure, also on his subscription. 0187: "i wouldnt want a budget check on a subscription … but i would love it on api".
- His omp session files with spend over the last 30 days (782): median $1.77, 75th percentile $5.82, 90th $13.67, 95th $23.16, 99th $140.38.

## Options
| Item | Options (pros / cons) |
|---|---|
| K7 | omp covers it (nothing to build) · Trim further per role (a small gain) |
| K8 | omp's roles as they are · Prewalk in threads (cheaper; quality unmeasured) |
| K9 | No scheduler, count misses (K1) · Keep-alive pings (cost money to maybe save money) |
| K10 | Warn at $25 · Warn at $10 · No warning (0187) |

## Recommendation
K7 nothing to build; K8 omp's roles; K9 no scheduler; K10 warn at $25.

## Choice
- **K7:** nothing to build; omp's `xd://` catalog covers it.
- **K8:** the product keeps omp's roles; the benchmark compares threads with and without `--prewalk`, and the product follows the result in a later record.
- **K9:** no scheduler in the product; the benchmark compares keep-alive pings before the 5-minute cache ends with none, by cost per passing task, and the product follows the result in a later record.
- **K10:** every thread starts with `--warn-cost 50`; the warning shows in its pane and the boss tells him. No `--max-cost` on the subscription.

## Reason
"No, omp covers it"; "we can try both and see what is better" (K8); "we can maybe try also both approaches and see" (K9); "maybe at 50 dollars" (K10).

## Consequences
- Differs from the recommendation on K8 and K9 (measured first) and K10 ($50).
- Amends 0187: a warning (never a stop) on the subscription.
- The arm set (a later record) carries the prewalk and keep-alive comparisons.
