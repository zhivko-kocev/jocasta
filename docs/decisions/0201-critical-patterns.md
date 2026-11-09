# 0201 Critical patterns

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Is the catalog's capability tiers and critical patterns row (0077) built as its own mechanism?

## Facts
- omp's tiers (read, write, exec) map onto the rule grammar's capabilities (read, write, exec, net, MCP).
- The open part was a built-in critical list that holds even in `yolo`. Slice 2.2's git-guard and corpus cover destructive commands.

## Options
| Option | Pros | Cons |
|---|---|---|
| A separate built-in critical list | `yolo` has a floor | A list beside 2.2's |
| Fold into 2.2's git-guard | One list | Non-git dangers live in 2.2 too |
| Dropped | Only his rules decide | `yolo` has no floor |

## Recommendation
Fold into 2.2.

## Choice
Folded into 2.2: the git-guard rules and corpus are the built-in critical list, and they hold in every mode.

## Reason
Not given.

## Consequences
- 2.2's agenda covers non-git destructive commands (recursive deletes, disk tools) as part of the critical list.
- Appendix A's capability-tiers row is decided.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
