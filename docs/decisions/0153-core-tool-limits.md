# 0153 Core tool limits

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
What output limits do the core tools keep?

## Facts
Verified 2026-10-07: Pi 1.0.1 caps tool output at 2,000 lines or 50 KB and grep match lines at 500 characters (`dist/core/tools/truncate.js:10-12`).
- Asked as a tick-list (0089); both items ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep Pi's caps | Known behavior; the spill handles the rest | — |
| Other caps | — | — |

## Recommendation
Keep Pi's caps.

## Choice
Jarvis's `read` and Pi's `grep`, `find` and `ls` keep 2,000 lines or 50 KB, and 500 characters per grep line; output over 32 KiB spills (0150).

## Reason
Not given.

## Consequences
- Appendix A's core-tools row (limits in 1.8) is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
