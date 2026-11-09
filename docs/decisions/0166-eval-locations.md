# 0166 Eval locations

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
Where do eval cases and run output live?

## Facts
- The 0165 preview put run output under the config dir; 0095 puts machine-written data in the data dir, and 0094 keeps the config dir for files he edits.
- Eval cases are curated and may name clients; run output is machine-written. Both dirs are local and private.

## Options
| Option | Pros | Cons |
|---|---|---|
| Cases in the config dir, runs in the data dir | Follows 0094 and 0095; wiping the data dir drops only reproducible runs | Two places |
| Both in the config dir | One place | Large machine output among hand-edited files |

## Recommendation
Cases in the config dir, runs in the data dir.

## Choice
Now (0375): a real run takes the repo's cases (`evals/cases/`) and then his own from `~/.jarvis/agent/evals/cases/`; runs go to `<data>/evals/runs/<time>-<label>/`, the label naming the variants (`default` without one), and a comparison writes `compare.md` into the second run; the rest stands.

Originally: Cases: `~/.jarvis/agent/evals/cases/`. Runs: `<data>/evals/runs/<time>/` with `results.jsonl`, `transcripts/` and `report.html`.

## Reason
Not given.

## Consequences
- Replaces the run location previewed in 0165.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
