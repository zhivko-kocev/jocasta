# 0437 The HUD every 5 s

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
herdr's tab bar reads `<data>/hud.txt` every 2 s (0403), starting a new `cmd.exe` each time. Is a slower read acceptable?

## Facts
Verified 2026-10-15.
- `config/herdr.toml`'s HUD runs `type …hud.txt` every 2 s: 30 `cmd.exe` starts a minute while herdr runs; herdr's own default is 5 s.
- The service rewrites `hud.txt` on every change; the rail, `[event]` lines, toasts and speech don't go through it; herdr's clock is its own.
- Only the tab bar's lag behind a change grows (to at most 5 s).

## Options
| Option | Pros | Cons |
|---|---|---|
| 5 s | 12 starts a minute; herdr's default | Tab bar up to 5 s behind |
| 2 s (as now) | Tab bar quickest | 30 starts a minute |
| 10 s | Fewest starts | Tab bar visibly late |

## Recommendation
5 s.

## Choice
herdr's tab bar reads `<data>/hud.txt` every 5 s instead of 2 s; the service still rewrites it on every change; herdr's clock, the rail, `[event]` lines, toasts and speech are unchanged.

## Reason
"5 s" (after the explanation that only the tab-bar lag changes).

## Consequences
- Amends 0403 (every 2 s).
- His installed `~/.jarvis/agent/herdr.toml` gets it through `jarvis setup` (0440).
- Amended by 0456: the HUD adds the subscription windows `5h N% · 7d N%`.
- Amended by 0554: the subscription windows move from the HUD into `today`.
