# 0266 Thread runtime states

- Status: decided
- Kind: product
- Phase/slice: 4/4.5
- Date: 2026-10-08

## Question
Which runtime states does a thread have?

## Facts
- The plan's list, plus `interrupted` (0256). Asked as a tick-list (0089); the second item, one live run per thread and per checkout, came back unticked with "i dont understand the second statement" and is asked again (0267).

## Options
| Option | Pros | Cons |
|---|---|---|
| `queued`, `starting`, `running`, `blocked`, `done`, `failed`, `cancelled`, `interrupted` | — | — |

## Recommendation
The list.

## Choice
`queued`, `starting`, `running`, `blocked`, `done`, `failed`, `cancelled`, `interrupted`, shown in the sidebar and HUD.

## Reason
Not given.

## Consequences
- None.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
