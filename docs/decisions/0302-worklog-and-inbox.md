# 0302 Worklog and inbox

- Status: decided
- Kind: product
- Phase/slice: 8/8.5
- Date: 2026-10-08

## Question
What do the worklog and the inbox do?

## Facts
- Threads have titles and reports (0247); his commits are in git. Repos never get Jarvis files without his word (0067).

## Options
| Option | Pros | Cons |
|---|---|---|
| Worklog per day from threads and commits, in the data dir; an inbox through the boss chat or voice that files items | Both habits | — |
| Worklog only | Smaller | No quick capture |
| Inbox only | Smaller | No timesheet help |

## Recommendation
As described.

## Choice
- Worklog: at day's end or on request, one entry per project per day from that day's threads and his commits, with links and time per project from thread activity, in `<data>/worklog/YYYY-MM-DD.md`; he edits it freely.
- Inbox: "note: …" or "remind me …" in the boss chat or by voice adds an item; Jarvis files it into a project's next work or into `today` at a given time, and asks only when it can't tell where it goes.

## Reason
Not given.

## Consequences
- Appendix I drops the `worklog` and `inbox` commands; both live in Frontier.
