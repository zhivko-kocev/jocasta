# 0151 Thread-local scratch files

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
Do threads get `local://` scratch files?

## Facts
- omp has `local://`, used in his omp sessions; the plan bundled it with the spill (v4 P1.5-14).
- Principle 8: minimal by default. Phase 3 adds a state doc per thread (3.1).

## Options
| Option | Pros | Cons |
|---|---|---|
| After v1 | The state doc and the spill may cover it | No scratch files until then |
| v1 with the spill | Plans and subagents (6.4) can hand files around | Another URL scheme before a consumer |
| Dropped | Simplest | No shared scratch |

## Recommendation
None was marked in the question.

## Choice
v1, with the spill: `local://<name>.md` files live in `<thread dir>/local/`, written and read through the write and read tools.

## Reason
Not given.

## Consequences
- The internal URL router (appendix I) serves `local://` with `artifact://`.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
