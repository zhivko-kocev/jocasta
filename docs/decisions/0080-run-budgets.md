# 0080 Run budgets

- Status: decided
- Kind: product
- Phase/slice: 0/appendix G
- Date: 2026-10-07

## Question
Does the plan add caps on a run's turns, spend and wall clock?

## Facts
Verified 2026-10-07.
- mini-swe-agent v2.4.6 (`src/minisweagent/agents/default.py:26-33,132-140`): `step_limit`, `cost_limit` (default 3.0), `wall_time_limit_seconds` and `max_consecutive_format_errors` (default 3), checked before every model query; a breach exits with `LimitsExceeded` or `TimeExceeded`.
- Claude Agent SDK 0.3.280 (`sdk.d.ts:1865-1880,5449`): `maxTurns`; `maxBudgetUsd` stops the query with an `error_max_budget_usd` result; an alpha `taskBudget` tells the model its remaining tokens (`output_config.task_budget`, beta `task-budgets-2026-03-13`).
- Strands Agents lifecycle controls, https://strandsagents.com/docs/user-guide/sdk/agents/lifecycle-controls/: limits on turns, output tokens and total tokens, "checked at the top of each loop iteration, not mid-call"; stop reasons `limitTurns`, `limitTotalTokens`, `limitOutputTokens` and `cancelled`; a cancel signal for deadlines.
- The plan had the loop guard (5 identical calls) and 4.12's warnings at 50, 80 and 95% of the day's usage, and no cap on a single run.
- Subscription use through Pi bills as extra usage per token (0062), and the boss runs threads while Zhivko is away (4.5).

## Options
| Option | Pros | Cons |
|---|---|---|
| A cost-ledger lever (1.12, 4.11) and a catalog row | A looping or stuck worker stops loudly, naming the cap it hit (principle 2) | A cap can stop a good run mid-task; one more value per profile |
| Catalog row only (1.12) | The evidence enters | The cost ledger, where levers are tracked, doesn't show it |
| A daily spend cap (4.12) | One number | One runaway thread can spend the day before it trips |
| Nothing | — | Spend is bounded only by the loop guard |

## Recommendation
The ledger lever and the catalog row; values per profile keep principle 1.

## Choice
The ledger lever and the catalog row.

## Reason
Not given.

## Consequences
- Appendix G's cost ledger gains "Run budgets", decided in 1.12 (the mechanism and the strict profile) and 4.11 (the worker profile).
- Appendix A gains the "run budgets" row, with mini-swe-agent, the Claude Agent SDK and Strands as sources.
