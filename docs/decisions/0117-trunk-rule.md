# 0117 The trunk rule

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What can happen in the project thread on trunk?

## Facts
- V9: main, master or dev is a scratch thread "where he can only chat".
- Phase 2's guard brings modes; `chat-only` is a planned seam (appendix I). What chat-only allows is 2.7's decision.
- Until phase 2 lands, the fixture gate (0026) keeps Jarvis in temp repos.

## Options
| Option | Pros | Cons |
|---|---|---|
| Chat-only, enforced by the guard (2.7) | V9 as written; an agent can't dirty trunk; mechanical | A one-line fix on trunk needs a workspace thread |
| Advisory warning | Never in the way | V9 becomes a suggestion |
| Full tools | No friction | Against V9 |

## Recommendation
Chat-only, enforced by the guard.

## Choice
The project thread on trunk is chat-only, enforced by phase 2's guard; 2.7 decides which tools chat-only allows.

## Reason
Not given.

## Consequences
- 2.7 defines the `chat-only` mode.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
