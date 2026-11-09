# 0024 Harness name

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-03

## Question
What is the harness called?

## Facts
Back-filled on 2026-10-04 from `docs/plan.md` (section 3 and the sections cited below). The planning-era facts and options live in the v4 and v5 plans, which are read-only references (plan section 4).

## Options
Not carried into the final plan; see the v4 and v5 plans.

## Recommendation
Not recorded in the final plan.

## Choice
Jarvis: the binary and command, the repo, `piConfig.name`, the `JARVIS_` env prefix and the config dir.

## Reason
Not recorded.

## Consequences
Slice 0.1 checked the name for collisions on 2026-10-04; see 0033.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
