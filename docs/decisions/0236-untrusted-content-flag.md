# 0236 Untrusted content flag

- Status: decided
- Kind: product
- Phase/slice: 2/2.6
- Date: 2026-10-08

## Question
Is an untrusted-content flag on web, MCP and file results in v1?

## Facts
- Pi has no such flag; Jarvis would wrap those results. It would serve 5.3 (memory) and 6.9 (web). Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| After v1, with its first consumer | No unused surface | — |
| v1 | Ready for 5.3 and 6.9 | Built before it's needed |
| Dropped | — | — |

## Recommendation
After v1.

## Choice
After v1: built with the first slice that consumes it, if it survives the pruning pass.

## Reason
Not given.

## Consequences
- Slice 2.6 is decided.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
