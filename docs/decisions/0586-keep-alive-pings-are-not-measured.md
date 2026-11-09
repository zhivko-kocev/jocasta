# 0586 Keep-alive pings are not measured

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
0541 (K9) has the benchmark compare keep-alive pings before the 5-minute cache ends with none, by cost per passing task; 0575 moved that measurement to H8's threads waiting on his Allow. Building it showed facts that don't fit. Measure the pings in H8 as built, measure them in a dedicated probe, or drop the measurement?

## Facts
Found 2026-11-09 while building 0575's H8 measurement, in omp 18.18.2's installed source.
- An OAuth subscription seat writes 1-hour cache entries on Anthropic models that support them (`session/cache-warmer.ts:109-120`); idle warming stops 30 minutes after the request that started it (`:61`) and refreshes at 90% of an entry's lifetime (`:73-77`), so a 1-hour entry is never refreshed while idle. His omp, in the VM too (0504), signs in with his subscription.
- A ping is sent only when it is expected to save at least $0.05, with a 15% chance assumed that a real request comes before the entry expires (`:62-69`); with Opus that takes a prompt of roughly 96k tokens [INFERENCE: omp's catalog prices equal to the pinned table], more than H8's fixture threads may reach.
- H8's threads start without his Allow (0544, 0576) and their plans are approved unread (0516): they never wait on his Allow.
- The shipped `config/omp-config.yml` sets `providers.cacheWarming: streaming` (0541).

## Options
| Option | Pros | Cons |
|---|---|---|
| Drop the measurement | No noise data; three VM runs saved; no code kept for it | No measured figure for pings on 5-minute (API key) caches |
| Keep H8's `--keepalive idle|off` with 5-minute caches forced, 3 runs each | Already built | Likely 0 pings (noise); 3 more runs of up to an hour; not his subscription |
| A dedicated probe: one session past 100k tokens, fixed waits, pinging on vs off | Measures pings where they fire | New benchmark code before the weekend; API-key pricing only |

## Recommendation
Drop the measurement.

## Choice
Keep-alive pings aren't measured. On his subscription omp writes 1-hour cache entries, which its idle warmer never refreshes; elsewhere omp sends a ping only when it expects it to save at least $0.05. H8 has no keep-alive condition (the `--keepalive` switch built during the review's fixes is removed), and the product keeps `providers.cacheWarming: streaming`.

## Reason
He chose "Drop it".

## Consequences
- Amends 0541 (K9: answered by omp's own behavior, not measured) and 0575 (H8 doesn't measure pings; the task run's "not compared: … keep-alive pings (no idle gap in a trial)" line stays true).
- No product, setting or result-format change.
