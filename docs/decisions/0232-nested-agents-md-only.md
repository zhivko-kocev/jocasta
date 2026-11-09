# 0232 Nested AGENTS.md on read, without path-scoped rules

- Status: decided
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-08

## Question
With Claude formats gone (0231), what remains of 0156's rules on read?

## Facts
- 0156 made path-scoped rules and nested `AGENTS.md` on read v1. Path-scoped rules come from `.claude/rules`; nested `AGENTS.md` is opencode's feature: the `AGENTS.md` files above a file that is read are attached once (0083).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep nested `AGENTS.md` only | A small extension over plain files | — |
| Drop both | Smaller | Subfolder instructions are missed |
| Decide in the pruning pass | — | — |

## Recommendation
Keep nested AGENTS.md only.

## Choice
Nested `AGENTS.md` (and `CLAUDE.md`, by Pi's order) above a file the model reads is attached once per window; path-scoped rules are dropped.

## Reason
Not given.

## Consequences
- Amends 0156.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
