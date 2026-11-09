# 0429 The dashboard's extra column

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
Which extras does the dashboard show beyond Jarvis's own parts, and from where?

## Facts
Verified 2026-10-14.
- Jarvis's own parts: Presence and voice state, clock and date, Calendar with links and the next-meeting countdown, Needs you, threads by project, spend, Next and reminders, the canvas.
- brutalist.report has no RSS and no terms; robots.txt allows all; an undocumented JSON endpoint `/api/v1/headlines?topic=<t>&limit=<n>` answers in about 0.5 s (67 KB for 30 items).
- Open-Meteo is keyless JSON, free for non-commercial use under 10,000 calls a day, CC-BY 4.0.
- Mail needs a login per provider; 0297 put it after v1.
- The meeting link the rail expects (`calendar[].url`) is never filled today.

## Options
| Option | Pros | Cons |
|---|---|---|
| News | Headlines at a glance | An undocumented endpoint |
| Weather | Glanceable | His location in jarvis.yaml |
| Mail | Unread at a glance | A login and a provider app |
| Machine and services | Sees a stuck service | More polling |
| Today's activity | Recent events and summaries | More to read |

## Recommendation
News; mail later.

## Choice
The third column shows: News from brutalist.report's JSON endpoint, topics tech and gaming, titles, source, age and link only (opened in the system browser), refreshed every 15 min, never spoken, never sent to the boss or the graph; on failure it says so once and keeps the last list. Weather from Open-Meteo for the place in `jarvis.yaml` (`dashboard.weather`), refreshed every 30 min, hidden until set. Machine and services: CPU, RAM and disk, and whether Graphiti, whiteboard and herdr are up. Today's activity: the latest `[event]`s, the worklog so far and the memory summaries saved today. Mail stays after v1 (0297). Meeting links are filled from the iCal entry (URL, location or a meeting link in the description).

## Reason
Ticked: News (brutalist.report), Weather, Mail, Machine and services, Today's activity; news "tech + gaming"; mail "Later".

## Consequences
- Mail was ticked but its question was answered "Later": no mail now (0297 stands).
- Amended by 0439: the third column follows the window's mode (dashboard, from 1200 CSS px).
- Amended by 0442: News is Hacker News and LWN in full and the newest 10 gaming, set by `pollers.news.feeds`.
- Amended by 0443: machine and services refresh every 10 s.
- Amended by 0444: Graphiti's row says when it is idle, starting, or down because Docker isn't running.
- Amended by 0456: "Awaiting you" keeps reviewed, unfinished threads with their wait; leftover worktrees are listed.
- Amended by 0548: the third column's future (news, weather, clock, machine) is decided in the herdr experience pass.
- Amended by 0555: clock, weather and machine sit in `today`'s header; news fills its last rows while there is room.
