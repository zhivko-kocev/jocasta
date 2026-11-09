# 0134 Lazy-loaded subcommands

- Status: decided
- Kind: product
- Phase/slice: 1/1.6
- Date: 2026-10-07

## Question
Is the catalog's "lazy-loaded subcommands" row (0077) in v1?

## Facts
Measured 2026-10-07 (median of five): `dist/windows-x64/jarvis.exe --version` takes about 296 ms, because Pi's modules load before the flag check; an empty Bun process takes about 15 ms.
- omp loads subcommands lazily (0077).

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | CLI commands, the boss's calls and the startup scan (0108) answer in tens of ms | The entry keeps Pi's imports dynamic |
| After v1 | Nothing now | Every CLI command pays about 0.3 s |
| Dropped | Simplest | The same, for good |

## Recommendation
v1.

## Choice
v1. The entry checks argv first and imports Pi only for the harness role; Jarvis's CLI commands never load Pi.

## Reason
Not given.

## Consequences
- `--version` and Jarvis's commands run without importing Pi.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
