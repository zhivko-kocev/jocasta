# 0229 Lean Jarvis and the pruning pass

- Status: decided
- Kind: process
- Phase/slice: all (raised in 2.5)
- Date: 2026-10-08

## Question
How is Jarvis kept minimal?

## Facts
- His words: "at the end we should prune the heavy feature i want bare PI with simple extensions + the good stuff so simple optimal minimal and it beats everyone in benchmarks".
- By 0228 the pass had marked many features v1; in phase 2 alone: history-aware rules (0205), hooks on six events (0225, 0226), a trust prompt with hashes (0219–0221), grants (0222) and dry-run previews (0223).
- Appendix A holds his usage counts from omp and Claude Code.

## Options
| Option | Pros | Cons |
|---|---|---|
| Lean recommendations from now on, plus a pruning pass after the grilling pass | Cuts early and catches what was already decided | One more pass |
| A pruning pass at the end only | No change to the pass | Remaining questions keep growing v1 |
| Stop and prune phases 1–2 now | Immediate | Breaks the pass's order |

## Recommendation
Both.

## Choice
Now (0306): the pruning pass re-judges only harness features that Pi already covers or that show no use; Jarvis's own features stay as decided.

Originally: - From 0230 on, every recommendation favors the smallest option that does the job: Pi as it is, or Pi plus a small extension, and says when it does.
- After the grilling pass ends, a pruning pass re-judges every v1 feature decided from 0090 on as keep, after v1 or drop. A feature stays in v1 only if it measurably raises a chosen benchmark score (0230), he uses it daily (appendix A's usage counts), or it is safety he asked for. Each cut is a record that amends the feature's record.

## Reason
"at the end we should prune the heavy feature i want bare PI with simple extensions + the good stuff so simple optimal minimal and it beats everyone in benchmarks"

## Consequences
- Plan section 4 gains the rule. The build starts after the pruning pass, not after the grilling pass.
- Amended by 0306: the pass is narrowed to harness features.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
