# 0163 Eval cases

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
Where do eval cases come from?

## Facts
Verified 2026-10-07 (0079): the claude-api skill's build-eval takes production transcripts first, then bug reports, five to ten hand-written cases, then synthesized variations; 15 to 100 cases; a pass rate's noise is about 1/sqrt(cases × reps).
- His 658 omp sessions and 86 Claude Code transcripts name clients (0035, principle 6).

## Options
| Option | Pros | Cons |
|---|---|---|
| Mined from his transcripts first, then bug reports, hand-written and synthesized | Cases mirror his real work (R15) | Mining and anonymizing take effort |
| Hand-written and synthesized only | All can be public | Misses how he works |

## Recommendation
Mined first.

## Choice
Mined from his omp and Claude Code transcripts, and from Jarvis's threads once they exist; then bug reports, hand-written cases and synthesized variations. 15 to 100 cases, with reps chosen so the noise floor is below the smallest change worth keeping. Cases that name clients live in the config dir; the repo keeps placeholder cases.

## Reason
Not given.

## Consequences
- None.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
