# 0715 Brief retention

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M2 R8
- Date: 2026-11-27

## Question
`<data>/briefs/` keeps every brief and every screenshot a review left, forever. How long are they kept, and what removes them?

## Facts
- Briefs are written only through `store::save` (the service; `jocasta brief open` without a running Jocasta, 0716). A thread brief is written again at each review, a morning brief once a day, a `show` brief once.
- Screenshots of `review_try` live in `<data>/briefs/assets/<brief id>/` (0706).
- No timer runs for briefs; the service has a refresh tick, the command has none.

## Options
| Option | Pros | Cons |
|---|---|---|
| On each save, delete briefs last written over 30 days ago, with their assets | No timer; works for the service and the command alike; a brief in use is rewritten and stays | A day with no save deletes nothing that day |
| A daily sweep on the refresh tick | Regular | Only while the service runs; another tick job |
| Keep everything | Nothing to build | The folder grows without end |

## Recommendation
Delete on save, by file modification time, 30 days.

## Choice
His answers of 2026-11-26 and the approved plan: when a brief is saved, every `<data>/briefs/<id>.json` whose modification time is more than 30 days old is deleted, and its `<data>/briefs/assets/<id>/` with it. A file that can't be read or removed stays until the next save. Only files named as brief ids are looked at.

## Reason
Not given; a review page is worth having for a month, and saving is the one moment both writers share.

## Consequences
- `store::save` calls `prune`; `store::KEEP` is 30 days. A thread open longer than 30 days without a new review loses its brief; its next review writes it again.
- `show-<n>` numbers can be reused once every higher one is gone.
