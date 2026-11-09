# 0174 Context gauge

- Status: dropped
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
Is the catalog's context gauge row (0081) in v1?

## Facts
Verified 2026-10-07: Pi 1.0.1's footer already shows context used as a percentage of the window (red over 90%) and the latest cache hit rate (`dist/modes/interactive/components/footer.js:118-178`).
- The row's new parts are the next rollover's position (from 3.4's threshold) and the last turn's growth. Principle 8.
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1, built in 3.4 | Rollover is visible before it happens | One more status element |
| Dropped; Pi's footer is enough | Nothing to build | No warning of where rollover lands |
| After v1 | Decide after living with 3.4 | — |

## Recommendation
None was marked in the question.

## Choice
Now (0307): dropped; Pi's footer shows the window percentage.

Originally: v1, built in 3.4: a rollover mark and the last turn's growth beside Pi's window percentage.

## Reason
Not given.

## Consequences
- 3.4's agenda gains the gauge; appendix A's row is v1.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
