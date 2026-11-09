# 0205 History-aware rules

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Is the catalog's history-aware rules row (0082) in v1?

## Facts
- The only implementation is Strands Box, pre-release and macOS-only (0082).
- Jarvis's engine would keep a per-thread history of calls and results.
- 2.2's git-guard and 3.7's done check cover part of the same need.
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | Catches commits without tests | The most complex rule kind; a per-run history store |
| After v1 | The done check covers the main case first | Commits can skip tests |
| Dropped | Simplest engine | The same, for good |

## Recommendation
None was marked in the question.

## Choice
Now (0307): after v1.

Originally: v1: a rule can require that an earlier event happened in the thread's current run, for example that the tests passed after the last edit.

## Reason
Not given.

## Consequences
- The rule grammar (item 2) includes the history condition; the guard keeps a per-run log of calls and results.
- Appendix A's row is v1.
- Amended by 0307: after v1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
