# 0065 Day-one guard

- Status: decided
- Kind: product
- Phase/slice: 0/0.5
- Date: 2026-10-04

## Question
Does a git-guard ship before phase 2?

## Facts
- The fixture gate (0026) keeps Jarvis out of real repos until phase 2's guard lands.
- Pi 1.0.1's `tool_call` handler can block, and a handler failure blocks the tool (plan 5.4).

## Options
| Option | Pros | Cons |
|---|---|---|
| Wait for phase 2's guard | One implementation | No real repos until phase 2 |
| Day-one guard as a compiled-in extension, failing closed | Real repos before phase 2 | A second implementation |

## Recommendation
Wait (changed from the earlier day-one recommendation, because the fixture gate makes waiting safe).

## Choice
Wait for phase 2's guard.

## Reason
Not given.

## Consequences
The fixture gate stays until phase 2. The phase 0 exit smoke's guard check doesn't apply.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
