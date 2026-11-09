# 0187 Run budgets status

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Are run budgets (0080) built in v1, and with which strict-profile numbers?

## Facts
- 0080 added run budgets as a cost-ledger lever and a catalog row: caps on turns, spend and wall clock for unattended runs (headless runs, and threads the boss runs), checked before each model request; hand-driven tabs are uncapped.
- Pi 1.0.1 has no per-run limits; an extension can see each provider request and abort (`dist/core/extensions/types.d.ts:242,672,1166`).
- Asked three times: the first answer asked what the budgets do, the second what an unattended run is. A follow-up asked whether "after v1" meant the whole feature or only the numbers.

## Options
| Option | Pros | Cons |
|---|---|---|
| An extension, strict at 40 turns, $5 and 30 minutes | A runaway stops early | A long legit task stops |
| An extension, strict at 100 turns, $20 and 2 hours | Rarely stops real work | A runaway spends more first |
| The mechanism now, numbers later | Numbers from real costs | Evals uncapped until then |

## Recommendation
An extension; 40 turns, $5, 30 minutes.

## Choice
Free text: "let's handle that after v1", confirmed by the follow-up as the whole feature. Run budgets are after v1: neither the mechanism nor any profile's numbers are built in v1.

## Reason
"i wouldnt want a budget check on a subscription lets say but i would love it on api but lets leave it for after v1"

## Consequences
- Appendix G's run-budgets lever and appendix A's run-budgets row are after v1; 4.11 doesn't ask worker budgets.
- Stop detection (0181) relies on Pi's stop reasons and the loop guard until then.
- When run budgets return after v1, they apply to API-key use, not to subscription sign-in.
- Differs from the recommendation.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
- Amended by 0541: every thread starts with omp's `--warn-cost 50`, a warning only (never a stop) on the subscription.
