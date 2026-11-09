# 0137 Jarvis never shadows Pi's commands

- Status: decided
- Kind: product
- Phase/slice: 1/1.6
- Date: 2026-10-07

## Question
Do Jarvis's command names ever take Pi's?

## Facts
Verified 2026-10-07 in Pi 1.0.1: Pi's first-arg commands are `install`, `remove`, `update`, `list`, `config`, `mcp` and `auth` (0133).
- Section 7 proposed that Jarvis's commands never shadow Pi's.
- Appendix I's phase 6 `mcp` collides with Pi's `mcp`.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Never: Jarvis's commands and flags avoid Pi's names | Pi's docs and commands keep working | Some names are taken |
| Shadow where useful | Freer naming | Pi's commands vanish |

## Recommendation
Never shadow Pi.

## Choice
Jarvis's commands and flags never take Pi's names: today `install`, `remove`, `update`, `list`, `config`, `mcp`, `auth` and Pi's flags.

## Reason
Not given.

## Consequences
- 6.7 renames its planned `mcp` command.
- Section 7's Commands convention is confirmed.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
