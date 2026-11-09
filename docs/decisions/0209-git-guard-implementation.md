# 0209 git-guard implementation

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
How is the git-guard built?

## Facts
- Phase 0 shipped no day-one guard (0065). The git-guard is the built-in critical list in every mode (0201).
- The exit smoke expects a denial naming `git-guard/push-force`. Appendix I claims `guard check` and `guard test`.

## Options
| Option | Pros | Cons |
|---|---|---|
| TypeScript rules with ids, a YAML corpus compiled in, `jarvis guard test` and `jarvis guard check "<cmd>"` | Every rule is tested and explainable | Two commands |
| Rules only | Less surface | No way to test or explain a decision |

## Recommendation
TS rules, corpus and the two commands.

## Choice
TypeScript rules with ids, a YAML corpus of must-block and must-pass cases compiled into the binary, `jarvis guard test` (runs the corpus) and `jarvis guard check "<cmd>"` (explains one command).

## Reason
Not given.

## Consequences
- Preview: `deny  git-guard/push-force  force push rewrites remote history`; `214 cases: 214 pass`.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
