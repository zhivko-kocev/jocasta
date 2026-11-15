# 0599 Tune first, then the VM weekend

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-14

## Question
AGENTS.md schedules the VM benchmarks (omp 18.18.6, every benchmark from the start) for this weekend. The state-of-the-art tuning program (`docs/research/state-of-the-art.md`) would change the build the benchmarks measure. Which comes first?

## Facts
- The benchmarks must run on the final build (0533). A run on 18.18.6 would describe a build the product no longer ships.
- The baseline (window 2026-11-10 to 2026-11-14, copies counted once) shows $977 of own spend, of which subagents are $437 and project panes $287 (`docs/research/state-of-the-art.md` section 1).
- The completion probe alone is $8.23 of side spend in the window (section 2).
- Several proposals (R1–R9, P1–P6) change settings or add arms the benchmarks would need to include.

## Options
| Option | Pros | Cons |
|---|---|---|
| Tune first, then the VM weekend | Benchmarks test the final build; no re-run | The weekend run slips by the length of the tuning |
| VM weekend first on 18.18.6, tune after | Keeps the schedule | Results describe a build later changed; must be re-run |
| Tune only the cheap deterministic items first, defer the rest | Small change before the run | Two rounds of benchmark work |

## Recommendation
Tune first (option 1). The benchmark is the measurement of the product; measuring a build before it is final wastes the run.

## Choice
Tune first. The VM weekend waits for the tuning slices to land and pass `mise run check`. The benchmark rerun then runs once on the final build (0533, 0588).

## Reason
His answer: "tune-first".

## Consequences
- AGENTS.md's "Next" line is updated when the tuning slices land: the VM weekend follows them, not the reverse.
- The 0588 requirement (each benchmark script runs once in the VM before the full runs) stands.
