# 0403 The HUD in herdr's tab bar

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
How does the HUD show in herdr?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- `ui.tab_bar_right` takes `{type = "command", command, interval_seconds}` and `{type = "datetime"}` entries.

## Options
| Option | Pros | Cons |
|---|---|---|
| Tab-bar line from a file | No new command; cheap | A file the service rewrites |
| `jarvis hud` command | No file | A new command; a process every 5 s |
| Jarvis sidebar row | In the sidebar | Only on one row |
| No HUD in herdr | Nothing | Spend only in the companion |

## Recommendation
Tab-bar line from a file.

## Choice
The service writes `<data>/hud.txt` ("3 at work · 1 awaiting you · $4.82 today") on every change; herdr's tab bar shows it with `type` every 2 s, then the time.

## Reason
"Tab-bar line from a file".

## Consequences
- Appendix I gains `<data>/hud.txt`.
- Carries agent-deck's idea (0410).
- Amended by 0433: "$ today" counts every omp session of the day.
- Amended by 0437: herdr reads `hud.txt` every 5 s.
- Amended by 0554: the HUD keeps the blocked count, spend and the time; quota moves into `today`.
