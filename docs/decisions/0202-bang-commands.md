# 0202 `!` commands and the guard

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Do his own `!` commands go through the guard?

## Facts
Verified 2026-10-08 in Pi 1.1.0: `!` commands raise `user_bash`; a handler can block or replace them, and a throwing handler blocks the command (`dist/core/extensions/runner.js:973-999`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny rules and the git-guard only | No prompts for his own commands; still protected | One more rule path |
| The full guard | Uniform | He is asked about his own command |
| Unguarded | Like a terminal | No protection |

## Recommendation
Deny rules only.

## Choice
His `!` commands skip ask rules and run unless a deny rule or the git-guard (2.2) matches.

## Reason
Not given.

## Consequences
- Appendix A's `!` row is decided.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
