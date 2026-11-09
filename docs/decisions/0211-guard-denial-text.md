# 0211 Guard denial text

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
What does a denial say, and how are rules named?

## Facts
- Lesson L04 asks whether a blocked call should name the rule or return a generic error. Principle 2 prefers loud, specific failures; `guard log` (appendix I) searches by rule.

## Options
| Option | Pros | Cons |
|---|---|---|
| Name the rule and a one-line reason | The model picks another approach; the log is searchable | Reveals the rule |
| A generic error | Reveals nothing | The model retries blindly |

## Recommendation
Name the rule.

## Choice
Rule ids are `<group>/<name>`; the tool result reads `blocked by guard: <id> — <reason>.`

## Reason
Not given.

## Consequences
- Preview: `blocked by guard: git-guard/push-force — force push rewrites remote history. Ask Zhivko to push.`
- 2.6's item on what the model is told when the guard blocks is decided here.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
