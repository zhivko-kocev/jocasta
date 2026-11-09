# 0298 Event queue and quiet hours

- Status: decided
- Kind: product
- Phase/slice: 8/8.2
- Date: 2026-10-08

## Question
How are events queued, ranked and held?

## Facts
- Pollers (8.1) and threads produce events; Frontier's server keeps running (0253); `catalog.db` is the shared database (appendix I).
- Plan 7.4's leftover item: quiet hours.

## Options
| Option | Pros | Cons |
|---|---|---|
| One queue in `catalog.db`, deduplicated by source id; urgent, normal and low; quiet hours in `settings.json` let only urgent through | Calm nights | — |
| No priorities; quiet hours hold everything | Simpler | A red main waits until morning |

## Recommendation
As described.

## Choice
One queue in `catalog.db`, deduplicated by source id. Priorities: urgent (a red build on main, a blocked thread, a meeting in 5 minutes), normal (a review requested, a thread finished), low (digest only). Quiet hours in `settings.json` (`"frontier": { "quietHours": "22:00-08:00" }`); during them only urgent events go through, and the rest wait for the morning `today`.

## Reason
Not given.

## Consequences
- Appendix I: `catalog.db` gains the event queue in phase 8.
