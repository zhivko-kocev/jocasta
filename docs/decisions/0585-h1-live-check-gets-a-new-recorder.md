# 0585 H1's live check gets a new recorder on omp 18.18.2

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
H1 scores the guard with omp's own matcher; its live check confirmed that those verdicts match what omp refuses in a running session (24/24 on omp 18.13.1). On the build's omp 18.18.2 the live check's recorder no longer works. Re-measure the live agreement on 18.18.2, or keep the 18.13.1 result?

## Facts
Found 2026-11-09 while fixing the review's BEN findings (0573, 0576).
- Since omp 18.15.0 the bash tool's own approval checks the tool-call interceptor first and denies an intercepted call, so the live check's recording overlay would mark every command as refused by itself; `bench/h1-guard/live-check.ts` now refuses to start on 18.18.2.
- The matcher runs still work on 18.18.2: `omp-verdicts.ts` (omp's own resolver on the corpus) and `git-guard-check.ts --run` in the VM.
- Every benchmark row measures the build it runs on (0533).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep the 18.13.1 live check; rerun the matcher runs on 18.18.2 | No new harness before the weekend | The live agreement isn't re-measured on the build |
| Build a new live recorder (an ACP client that gives omp a terminal and records what it runs) | Everything on one build (0533) | New harness code before the weekend |

## Recommendation
Keep the 18.13.1 live check and rerun the matcher runs.

## Choice
Build a new live recorder for H1's live check that works on omp 18.18.2, and re-measure the live agreement on the build; the matcher runs rerun on 18.18.2 as well.

## Reason
He chose "Build a new live recorder".

## Consequences
- Differs from the recommendation.
- Amends 0500 and 0501 (H1's live check) and 0576 (BEN-8's pin to 18.18.2).
- The recorder is bench code under `bench/h1-guard/`; it adds no product code, dependency or setting.
