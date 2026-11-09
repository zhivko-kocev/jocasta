# 0139 Headless flags

- Status: decided
- Kind: product
- Phase/slice: 1/1.7
- Date: 2026-10-07

## Question
Which flags does headless mode take?

## Facts
Verified 2026-10-07 in Pi 1.0.1 (`dist/cli/args.js`): Pi has `-p`/`--print`, `--mode text|json|rpc`, `--model`, `--thinking` and more, and no `--thread`.
- 0132 rejects Pi's session flags; 0137 never shadows Pi.

## Options
| Option | Pros | Cons |
|---|---|---|
| `--thread <id>` plus Pi's own flags | One Jarvis flag; Pi's docs still apply | — |
| Jarvis-only flags | One vocabulary | Duplicates Pi |

## Recommendation
`--thread <id>` plus Pi's flags.

## Choice
`--thread <id>` runs in the named thread from any folder; every other flag is Pi's.

## Reason
Not given.

## Consequences
- Appendix I claims `--thread`.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
