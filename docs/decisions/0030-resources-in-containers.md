# 0030 Resources in containers

- Status: decided
- Kind: product
- Phase/slice: planning
- Date: 2026-10-03

## Question
Where do Jarvis's resources run?

## Facts
Back-filled on 2026-10-04 from `docs/plan.md` (section 3 and the sections cited below). The planning-era facts and options live in the v4 and v5 plans, which are read-only references (plan section 4).

## Options
Not carried into the final plan; see the v4 and v5 plans.

## Recommendation
Not recorded in the final plan.

## Choice
Jarvis's resources run in containers managed through Docker wherever a container can host them; slice 0.7 decides the boundary, the runtime and the details.

## Reason
Zhivko's direction.

## Consequences
The tabs, mic, speakers, keyring and repo toolchains stay on the host, because a container can't reach them well on Windows (0.7).
