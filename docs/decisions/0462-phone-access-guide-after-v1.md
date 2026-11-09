# 0462 Phone access is documented after v1

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
How does he reach his panes from a phone without a vendor relay?

## Facts
Verified 2026-10-23.
- Claude Code Remote Control routes "All traffic … through the Anthropic API" (https://code.claude.com/docs/en/remote-control).
- herdr supports Windows x64 hosts for remote attach (persistence-remote.mdx:76) and has a mobile layout (`ui.mobile_width_threshold`, `herdr --default-config`); herdr itself on Android is blocked by its installer (herdr #3857, NOT_PLANNED).
- Tailscale and an OpenSSH server are both absent on this PC.
- The herdr client inside a Windows sshd session is untested [INFERENCE].
- Phase 9 is after v1 (0304).

## Options
| Option | Pros | Cons |
|---|---|---|
| A guide after v1: Tailscale + OpenSSH Server + a phone SSH app | No vendor relay; no code | Two installs; untested |
| A vendor relay | Ready-made | Traffic leaves through a vendor (principle 6) |
| Nothing | No change | No phone access |

## Recommendation
A guide after v1: Tailscale + OpenSSH Server + a phone SSH app.

## Choice
After v1, a guide (Tailscale + Windows OpenSSH Server + a phone SSH app running `herdr --session jarvis`); no code; needs two installs approved then; untested; herdr on Android is blocked by its installer (herdr #3857).

## Reason
"A guide after v1".

## Consequences
- Phase 9 (0304): nothing is built or installed in v1.
- The two installs are asked for when the guide is written.
