# 0009 Harness base

- Status: decided
- Kind: product
- Phase/slice: planning
- Vision: V7
- Date: 2026-10-02

## Question
What is the harness built on?

## Facts
Back-filled on 2026-10-04 from `docs/plan.md` (section 3 and the sections cited below). The planning-era facts and options live in the v4 and v5 plans, which are read-only references (plan section 4).

## Options
Not carried into the final plan; see the v4 and v5 plans.

## Recommendation
Not recorded in the final plan.

## Choice
Own build on Pi 1.0's SDK (TypeScript on Bun). No Go rewrite and no source fork; a fork happens only under the condition in plan section 5.5.

## Reason
Not recorded.

## Consequences
Plan section 5.5 (fork condition) and appendix E (fallback to stock `pi` plus a Jarvis package).
- Amended by 0383: upstreams are consumed as released; a fork only when a patch is needed, pinned as a submodule.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
