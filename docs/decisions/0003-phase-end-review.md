# 0003 Phase-end review

- Status: decided
- Kind: process
- Phase/slice: planning
- Date: 2026-10-02

## Question
How do phases end?

## Facts
Back-filled on 2026-10-04 from `docs/plan.md` (section 3 and the sections cited below). The planning-era facts and options live in the v4 and v5 plans, which are read-only references (plan section 4).

## Options
Not carried into the final plan; see the v4 and v5 plans.

## Recommendation
Not recorded in the final plan.

## Choice
Now (0312): phase-end reviews no longer block the next phase; he reviews and commits whenever he chooses.

Originally: Slices in a phase run back to back; after the last one Zhivko reviews and commits, then the next phase opens.

## Reason
Not recorded.

## Consequences
Agents never commit (plan section 4, execution rules).
- Amended by 0312: phases run on without stopping for review.
- Amended by 0564: the agent commits each finished phase once its gates pass, under the repo's own identity; he reviews the commits.
