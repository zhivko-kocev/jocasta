# 0014 Changed decisions

- Status: decided
- Kind: process
- Phase/slice: planning
- Date: 2026-10-02

## Question
How are changed decisions recorded?

## Facts
Back-filled on 2026-10-04 from `docs/plan.md` (section 3 and the sections cited below). The planning-era facts and options live in the v4 and v5 plans, which are read-only references (plan section 4).

## Options
Not carried into the final plan; see the v4 and v5 plans.

## Recommendation
A new record that supersedes the old.

## Choice
Edited in place; git history keeps the old version.

## Reason
Not recorded.

## Consequences
Differs from the recommendation, which was a new record that supersedes the old. Records have one status, `decided`; feature records use `v1`, `after v1` or `dropped`. An edit updates Choice, Reason and Consequences together, and the reason for a change goes in the phase-end commit message.
