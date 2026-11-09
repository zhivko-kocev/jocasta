# 0173 Cost display

- Status: dropped
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
Where does he see cost?

## Facts
- Pi's `/session` shows cost per model and cache use (plan 5.6).
- Appendix G recommends tokens, turn time and a cache-miss marker. Extensions can set status text (Pi 1.0.1 `docs/extensions.md:244-246`).

## Options
| Option | Pros | Cons |
|---|---|---|
| A Jarvis status segment for the last turn, plus `/session` for totals | Each turn's cost at a glance | One more status element |
| Pi's `/session` only | Nothing to build | Cost only on request |

## Recommendation
A status segment plus `/session`.

## Choice
Now (0307): dropped; Pi's footer and `/session` show cost.

Originally: A status segment shows the last turn's input and output tokens, cache-read share and turn time, with a cache-miss marker when the share is under 50%; `/session` keeps the totals.

## Reason
Not given.

## Consequences
- Preview: `↑12.4k ↓0.8k  cache 96%  14s` and `● cache miss` under 50%.
- Appendix G's cost-display lever is decided.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
