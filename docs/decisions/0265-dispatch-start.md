# 0265 Starting work the boss is asked for

- Status: decided
- Kind: product
- Phase/slice: 4/4.5
- Date: 2026-10-08

## Question
Does the boss start "do X in project Y" right away, and do boss-run threads get extra rules?

## Facts
- Boss-run threads run unattended; a guard ask blocks the thread and the boss relays it (0207, 0224, 0259). 4.11's worker profile is after v1 (0252).

## Options
| Option | Pros | Cons |
|---|---|---|
| Start right away; the guard only | No confirmation step; one rule set | — |
| Always confirm first | He sees the brief | A step per task |

## Recommendation
Start right away.

## Choice
Now (0285): the thread starts in plan mode, writes its plan and waits for his approval before it changes anything.

Originally: The boss resolves the project, creates the workspace thread, writes the brief, opens its tab and starts it. Boss-run threads follow the same guard as any thread; its asks become questions the boss relays.

## Reason
Not given.

## Consequences
- Amended by 0285: every workspace thread starts in plan mode and waits for his approval.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
