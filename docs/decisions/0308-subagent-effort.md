# 0308 Subagent thinking effort

- Status: decided
- Kind: product
- Phase/slice: 6/6.4
- Date: 2026-10-08

## Question
How is a subagent's thinking level set?

## Facts
- Appendix G proposed: the parent picks per spawn, capped at `high`, left for the pruning pass. Roles carry thinking levels (0283); bundled agents run on roles (0286). His omp thinking levels: max 287, medium 58, low 55, high 31, xhigh 9, off 5 (appendix A).

## Options
| Option | Pros | Cons |
|---|---|---|
| The role's level; a spawn may override; no cap | No code | No ceiling on cost |
| The parent picks per spawn, capped at `high` | A ceiling | A cap check |

## Recommendation
The role's level, no cap.

## Choice
Each bundled agent inherits its role's thinking level; a spawn may override it; no cap.

## Reason
Not given.

## Consequences
- Appendix G's subagent effort lever is decided.
