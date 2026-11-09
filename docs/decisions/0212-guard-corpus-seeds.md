# 0212 Guard corpus seeds

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
Which tricky inputs does the guard corpus cover?

## Facts
- Lesson L26's corpus (52 cases) passed on this machine; its gotchas: Git Bash turns `..\.env` into `...env` while cmd reads it raw; `echo '$(git push)'` must pass while `echo "$(git push)"` must block.
- Asked as a multi-select; every family was ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Continuations, extra spaces and case | — | — |
| Separators and substitution | — | — |
| Quoting that must pass | — | — |
| PowerShell and cmd forms | — | — |
| Secret files through the shell | — | — |

## Recommendation
None was marked in the question.

## Choice
All five families: line continuations, extra spaces and case; separators, substitution and pipes; quoting that must pass; PowerShell and cmd forms, routed to their own rules; secret files read through the shell (`.env` files denied, except `.env.example`, `.env.sample` and `.env.template`).

## Reason
Not given.

## Consequences
- Secret-file reads through the shell are a deny rule of the git-guard group.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
