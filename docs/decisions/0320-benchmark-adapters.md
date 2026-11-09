# 0320 Public benchmark adapters

- Status: decided
- Kind: product
- Phase/slice: 1/1.10 (0230)
- Date: 2026-10-08

## Question
When are Terminal-Bench 2.0 and SWE-bench adapters built?

## Facts
- 0230 left how they run to this slice. Terminal-Bench runs agents through its own harness in Docker tasks; SWE-bench runs a Docker evaluation per instance; both need Docker and Python tooling, and real runs cost tokens (details re-verified before building).

## Options
| Option | Pros | Cons |
|---|---|---|
| After phase 6; ask before installs and runs | Scores reflect the tools that matter | Later numbers |
| Now, in phase 1 | Early baseline | A baseline of a bare harness |
| After v1 | Nothing now | No public score in v1 |

## Recommendation
After phase 6.

## Choice
The adapters are built at the end of phase 6; any install or real run is asked first.

## Reason
Not given.

## Consequences
- Phase 6's exit adds the adapters.
