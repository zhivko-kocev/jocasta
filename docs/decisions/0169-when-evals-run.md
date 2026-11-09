# 0169 When evals run

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
When do evals run?

## Facts
- Scripted cases are free and deterministic; real-provider runs bill extra usage (0062) and need reps.
- v4 ran real evals at each phase end.

## Options
| Option | Pros | Cons |
|---|---|---|
| Scripted cases in CI on every change; the real provider at each phase-end review | Free regression checks; one real snapshot per phase | Phase-end runs cost money |
| Real runs only when a measured surface changes | Pays only when it matters | Model drift unseen between changes |
| Scripted only | Free | Never measures real behavior |

## Recommendation
CI scripted plus real at phase end.

## Choice
Now (0376): scripted cases run Jarvis with the scripted-model test fixture; the rest stands.

Originally: Scripted cases run in CI on every change; real-provider evals run at each phase-end review (0003).

## Reason
Not given.

## Consequences
- Appendix F's phase-end verification gains the real eval run.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
