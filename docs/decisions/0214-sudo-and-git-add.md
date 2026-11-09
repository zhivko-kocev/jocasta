# 0214 Elevation and `git add`

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
Do `sudo`-style elevation and `git add` get rules?

## Facts
- Elevation (`sudo`, `runas`, `Start-Process -Verb RunAs`) lets a command escape other rules. `git add` only stages; committing is governed by his rules (0067).
- Asked as a multi-select: ticked meant deny for elevation and ask for `git add`; neither was ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny elevation | — | — |
| Ask before `git add` | — | — |

## Recommendation
None was marked in the question.

## Choice
Elevation asks in every mode; `git add` has no rule.

## Reason
Not given.

## Consequences
- With 0210 and 0213, 2.2's disagreement items are all decided.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
