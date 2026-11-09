# 0164 Eval graders

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
How are eval cases graded?

## Facts
Verified 2026-10-07 (0079): for an agent acting on an environment, a programmatic end-state check is the primary grader; a model judge scores a rubric of checkable claims with a model other than the one under test, after a graded pilot is signed off.

## Options
| Option | Pros | Cons |
|---|---|---|
| End-state checks first; a judge only for open-ended output | Cheap and deterministic | Rubric work for open-ended tasks |
| A model judge for every case | Uniform | Tokens per grade; judge noise |

## Recommendation
End-state first.

## Choice
End-state checks first (tests pass, files and git state); a model judge only for open-ended output, with a rubric of checkable claims, a model other than the one under test, and his sign-off on a graded pilot.

## Reason
Not given.

## Consequences
- None.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
