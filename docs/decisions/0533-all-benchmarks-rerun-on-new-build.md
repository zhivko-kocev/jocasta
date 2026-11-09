# 0533 Every benchmark runs again on the harness-OS build

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
With the harness-OS layer in v1 (0532), which benchmarks run again on the new build?

## Facts
Verified 2026-11-08.
- H1–H9 ran on `c154055b…004b` (0520, 0530); the thesis kit's charts and evidence pack are built from those rows.
- The layer adds background processes and per-worktree state, which moves H3 (recovery), H4 (a shared index must not leak across projects), H5 (startup), H6 (memory), H7 (worktree caches and long paths), H8 (parallel threads) and H9 (install footprint). H1, omp's guard, is untouched.
- The task benchmark runs omp with config overlays, not through Jocasta (`bench/README.md`); the layer must be set up inside each trial for an arm to measure it.

## Options
| Option | Pros | Cons |
|---|---|---|
| All H1–H9 and the task run | One build of record for every row; H1 is cheap | H3, H8 and H9 take hours in the VM again |
| Affected only (H3–H9 and the task run) | Saves H1's run | The thesis cites two builds |
| Task run only | Least VM time | H5, H6 and H8 would describe a product that no longer exists |

## Recommendation
All H1–H9 and the task run.

## Choice
All of H1–H9 and the task run run again on the harness-OS build, which becomes the build of record. The rows on `c154055b…004b` are kept as the comparison, as 0517 kept the release's rows.

## Reason
"All H1–H9 + task run".

## Consequences
- Amends 0482: the build of record changes; the arms are decided in a later record of this program.
- The paused full run's 4 rows stay as history and don't count.
- The kit's charts, the evidence pack and the CHANGELOG's benchmark entries are redone from the new rows.
