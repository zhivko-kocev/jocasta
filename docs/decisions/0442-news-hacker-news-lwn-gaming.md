# 0442 News: Hacker News and LWN in full, gaming 10

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
He wants Hacker News and Linux Weekly News (all their links) plus the last 10 gaming headlines in the dashboard's News. What does the column show, and can `jarvis.yaml` change it?

## Facts
Verified 2026-10-15.
- brutalist.report's `/api/v1/headlines` filters by source (`?source=hn`, `?source=lwn`) and keeps about a day; with no `limit` it answers every item it holds: Hacker News 67–69, LWN 6 on 2026-10-15.
- The site's front page shows 50 Hacker News and 6 LWN links; its Hacker News page lists 72.
- `?topic=gaming&limit=10` answers the newest 10 of PC Gamer, Kotaku, Polygon and GameSpot.
- Today the column shows topics tech and gaming, 10 each (`pollers.news.topics`, `pollers.news.limit`, one limit for every topic).

## Options
| Option | Pros | Cons |
|---|---|---|
| HN + LWN all, gaming 10 | His words; every link of the day | A long list (~75 lines) that scrolls |
| HN 50 + LWN all, gaming 10 | As the site's front page | Older HN links cut |
| HN 30 + LWN all, gaming 10 | Shorter | Most of the day's HN cut |

## Recommendation
HN + LWN all, gaming 10; a `pollers.news.feeds` list in `jarvis.yaml`.

## Choice
News shows, in this order, Hacker News (every link brutalist.report holds, about a day), LWN (every link) and Gaming (the newest 10), each newest first, refreshed every 15 min as before (0429). `jarvis.yaml` `pollers.news.feeds` replaces `pollers.news.topics` and `pollers.news.limit`: a list of `{source: <name>}` or `{topic: <name>}`, each with an optional `limit`; the default is `[{source: hn}, {source: lwn}, {topic: gaming, limit: 10}]`.

## Reason
"HN + LWN all, gaming 10"; "pollers.news.feeds list".

## Consequences
- Amends 0429 (topics tech and gaming, 10 each).
- `pollers.news.topics` and `pollers.news.limit` retire; a `jarvis.yaml` that still sets them is told to use `pollers.news.feeds` (plan appendix I).
- Amended by 0555: news shows in `today`'s last rows while there is room.
