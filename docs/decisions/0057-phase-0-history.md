# 0057 Where history lives in phase 0

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Vision: V9
- Date: 2026-10-04

## Question
Where are conversations stored before phase 1?

## Facts
The data dir is decided in 1.1. The fixture gate (0026) keeps phase 0 to throwaway repos.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's default `<config dir>/sessions/` until phase 1 | Nothing to build | Not migrated |
| The data dir now | No later move | Pre-empts 1.1 |

## Recommendation
Pi's default.

## Choice
Pi's default `<config dir>/sessions/` until phase 1 moves storage under threads.

## Reason
Not given.

## Consequences
Phase 0 history is not migrated in phase 1.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
