# 0534 The reruns' three product bugs are fixed in the harness-OS build

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
0530 put three product bugs after the benchmarks so every row stayed on one build. Every row now runs again (0533). When are they fixed?

## Facts
Verified 2026-11-08 (`docs/status.md`, `docs/testing.md`).
- A hung companion holds the single-instance lock, so every new one quits.
- 0518's heal takes a herdr server the companion already restarted for him quitting the client (`herdrBack` in `src/shared/cli.ts`).
- One of 8 parallel `thread_start`s failed with EPERM renaming the project's `omp.yml`; K11 runs several threads on one repo.

## Options
| Option | Pros | Cons |
|---|---|---|
| Now, in the new build | The rerun measures the fixed product; K11 isn't hit by the EPERM | More work before the benchmarks |
| After the benchmarks | As 0530 decided | The benchmarks measure known bugs on a build that's rebuilt anyway |

## Recommendation
Now, in the new build.

## Choice
The three bugs are fixed in the harness-OS build, each under its own record, before the benchmarks run again.

## Reason
"Now, in the new build".

## Consequences
- Amends 0530 (fixed after the benchmarks).
- Built as 0565 (a hung service no longer blocks starts), 0566 (a healed herdr crash isn't quitting) and 0570 (parallel starts share the overlay).
