# 0356 Phase 8 as-built

- Status: decided
- Kind: product
- Phase/slice: 8/8.1–8.5, 8.7
- Date: 2026-10-09

## Question
Are the visible choices made while building phase 8 accepted?

## Facts
- Azure DevOps has no notification inbox: its notifications are emails sent from subscriptions, and its REST API manages subscriptions only. Jarvis builds Azure events from what `az` can read.
- Built and tested with fake `gh`/`az` runners, a local iCal feed, a fake clock and the faux provider.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept as built | Done | — |
| Add Azure PR comments | More signal | More calls per poll |
| Add Azure work items assigned to him | More signal | One more call per org per poll |

## Recommendation
Accept as built.

## Choice
- GitHub: only "review requested" notifications become events (normal); a failing check on one of his open PRs (up to 10) is urgent; other notifications are ignored. Azure DevOps: a PR where he is a reviewer and hasn't voted is normal; the newest finished pipeline run failing on main/master or on a branch he pushed is urgent. He asked whether Azure notifications show too ("we show dev azure notifications also yes?"); after the facts above he kept these two kinds.
- GitHub polls whenever Frontier runs; Azure only with orgs in settings; Calendar only with `JARVIS_CALENDAR_ICAL_SECRET`. Poller settings are read at start, quiet hours live.
- A failing poller shows one warning line in the boss chat, again only when its message changes.
- Event lines don't start a boss turn; urgent lines cut in mid-turn, and the boss reacts when it next runs.
- In quiet hours normal events and reminders show only in `today`; blocked threads still toast but aren't spoken.
- `today` opens on the first page connection of a day outside quiet hours (stamped in `<data>/today.json`), and from a HUD `today` button that toggles it; a panel over the chat under the tabs, closed with ×; empty rows read `no meetings`, `nothing`, `nothing to confirm`.
- The worklog is written when quiet hours start (else at 23:00), on request through the boss's `worklog` tool, and for yesterday at start if missing; time per project from session activity, gaps over 15 minutes are breaks. Layout `## <project> · <time>`, then Threads and Commits with `[sha7](url)` links; entries he edited are kept, untouched ones refreshed, a deleted one comes back on the next write.
- Boss tools `today`, `note(project, text)`, `remind(text, at, project?)` with `HH:MM` or `YYYY-MM-DD HH:MM`, `inbox_done`, `worklog`; the inbox lives in `<data>/me.db`; notes show under the project in the sidebar as `✎ text`.
- Event lines: a boss line, `reminder: ` on the 30-minute repeat, an `open` link for web URLs, a red left bar when urgent. Urgent toasts stay until clicked; normal ones last 6 seconds and appear only when Frontier isn't focused. No quiet-hours marker on the HUD.
- An urgent event is spoken as its own line when speakable and 20 words or fewer, else a fixed sentence per kind; the meeting line reads `<title> at HH:MM, in N minutes`, the disk line `disk almost full: N% free`.
- Spoken reports name a thread whose title equals its project's name once ("demo is blocked: …"); other threads stay "<project>'s <title> is blocked: …". The free-text row of a question isn't read out as an option. Found in the exe exit smoke, which heard "demo's demo is blocked".

## Reason
All ticked; Azure "That's enough".

## Consequences
- Appendix I: catalog.db v5 `events`, me.db v1 `inbox`, settings `pollers.*`, files `<data>/worklog/`, `<data>/today.json`, `<data>/me.db`, the phase 8 protocol messages and boss tools.
