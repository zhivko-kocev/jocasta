# 0215 Broken guard rules or corpus

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
What happens when the compiled guard rules or corpus are broken?

## Facts
- The rules and corpus are compiled into the binary (0209). Principle 4: a broken guard fails closed.

## Options
| Option | Pros | Cons |
|---|---|---|
| The build fails if `guard test` fails, and a rule that throws at runtime blocks the call | A broken guard never ships; runtime faults fail closed | — |
| Runtime blocking only | Simpler build | A broken corpus could ship |

## Recommendation
Build gate and blocking.

## Choice
`scripts/build.ts` fails when `jarvis guard test` fails; at runtime a rule that throws blocks the call with a message naming the rule.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
