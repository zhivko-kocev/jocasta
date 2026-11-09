# 0034 Bare `jarvis` until phase 4

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Vision: V1, V7
- Date: 2026-10-04

## Question
What does bare `jarvis` do before Frontier exists?

## Facts
Frontier, the single entry point (V1), arrives in phase 4. Until then the harness role is the only role.

## Options
| Option | Pros | Cons |
|---|---|---|
| Opens the harness TUI; phase 4 switches bare `jarvis` to Frontier | Usable from day one, like `pi` | The bare command's meaning changes at phase 4 |
| Prints "Frontier arrives in phase 4"; the harness needs an explicit subcommand | The bare command never changes meaning | Claims a subcommand name now; extra typing for four phases |

## Recommendation
Opens the harness TUI.

## Choice
Bare `jarvis` opens the harness TUI; phase 4 switches it to Frontier.

## Reason
Not given.

## Consequences
In phase 0 every invocation runs the harness role (plan section 8, build sketch). Phase 4 decides how tabs start the harness role once bare `jarvis` opens Frontier.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
