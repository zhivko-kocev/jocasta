# 0144 Slice 1.11 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
Which decisions does slice 1.11 (engine defaults) grill, now that v4 P1.6-8 is decided fresh (0088)?

## Facts
Verified 2026-10-07: Pi 1.0.1's cache warming defaults to `streaming` when `settings.json` doesn't set `cacheWarming` (`dist/core/settings-manager.js:679-681`); modes are `off`, `streaming` and `idle` (`settings-manager.d.ts:53`). Jarvis's seeded settings (0056) don't set it, so warming is on today.
- Catalog rows naming 1.11: self-extension and the context gauge (0081). Open items: Pi's prompt may call the model Pi, and routes extension questions to `examples/`, which releases don't ship (0044).

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of six: cache warming; the system prompt section; Pi's self-references; cost display; the context gauge row; the self-extension row | The open items and both rows land here | Six questions |
| The plan's three | Fewer | The open items and rows wait |

## Recommendation
The fresh agenda of six.

## Choice
The fresh agenda of six.

## Reason
Not given.

## Consequences
- Plan 1.11's list becomes the six items, citing this record.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
