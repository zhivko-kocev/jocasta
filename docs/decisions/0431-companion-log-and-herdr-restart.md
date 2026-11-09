# 0431 Companion log file and herdr restart

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
The installed companion discards its warnings, and nothing restarts herdr's server when it stops. Change that?

## Facts
Verified 2026-10-14.
- Every service warning goes to stderr, which the installed companion throws away (the decided herdr-version warning included).
- When herdr's server stops, the service only logs "unreachable" until he runs `jarvis` again; the service already relaunches panes herdr restores as shells.

## Options
| Option | Pros | Cons |
|---|---|---|
| Companion log file | Warnings survive | A file per day |
| Restart herdr if it dies | The surface heals itself | Restarts a server he stopped on purpose |

## Recommendation
Both.

## Choice
The companion writes its warnings and errors to `<data>/logs/companion-YYYY-MM-DD.log`, keeping 7 days. When herdr's server for session `jarvis` stops while the companion runs, the service starts it again within a few seconds (with `~/.jarvis/agent/herdr.toml`) and relaunches Jarvis's panes with their flags; quitting the companion stops nothing.

## Reason
Ticked both.

## Consequences
- None beyond the choice.
- Amended by 0472: `jarvis diagnose` opens a read-only omp pane with the recent companion log and a diagnose skill.
