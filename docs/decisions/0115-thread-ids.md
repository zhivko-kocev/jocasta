# 0115 Thread ids

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What form do thread ids take?

## Facts
- Plan 1.4: `t-` + 8 hex.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| `t-` plus 8 random hex, checked for collisions on create | Short; names the worktree dir | Not stable across machines |
| Per-project numbers | Readable | Collide across machines and after deletes |

## Recommendation
`t-` + 8 hex.

## Choice
`t-` plus 8 random hex digits, for example `t-9f3e2a1b`, checked for collisions on create.

## Reason
Not given.

## Consequences
- The id names a workspace thread's worktree dir (1.4's item 7).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
