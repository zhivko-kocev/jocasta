# 0567 A6 approves its plan as the product does, and only the keep-alive test pings

- Status: v1
- Kind: process
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
Phase F found two facts the benchmark records didn't foresee: A3, which A6 extends, approves plans with `--plan-yolo`, and omp's default cache warming already pings. How does A6 approve its plan, and which arms ping?

## Facts
Verified 2026-11-09 in omp 18.18.2 and Phase F's build.
- `--plan-yolo` approves the plan inside the same session (`session/prewalk.ts`, `#finalizePlanYoloProposal`): it behaves like "Approve and keep context", so 0545's execute-against-keep comparison on A6 would measure nothing.
- omp's plan picker offers "Approve and execute" (a fresh context with the plan) and "Approve and keep context"; Jocasta's threads get omp's default, "Approve and execute" (0545).
- `providers.cacheWarming`: omp's default is `idle` (keep-alive pings while a session waits); Jocasta's config sets `streaming` (0541). Arms on omp's defaults (A0–A3) would ping and A6 wouldn't.
- The follow-up study already compares pings on against off, paired, under 0543's keep rule.

## Options
| Item | Options (pros / cons) |
|---|---|
| A6's approval | Like the product: "Approve and execute", the follow-up variant "Approve and keep context" (A6 then differs from A3 in approval too) · Keep `--plan-yolo` and drop the comparison |
| Cache pings | Off in every arm but the keep-alive test (each arm's cost shows only what it tests) · Each arm as defined (A0–A3 ping, A6 doesn't) |

## Recommendation
Like the product; off everywhere but the test.

## Choice
- A6 doesn't use `--plan-yolo`: the harness answers omp's plan picker with "Approve and execute", as Jocasta's threads get when he approves; the follow-up variant answers "Approve and keep context".
- Every arm sets `providers.cacheWarming: streaming`; only the follow-up's keep-alive variant pings (`idle`). A0 is omp's defaults except cache warming.

## Reason
"recomended but what are the main differences"; on the pings, after the explanation: "try both and see what is better", which the keep-alive comparison does.

## Consequences
- Amends 0543 (A6's setup; A0–A4's cache warming) and 0545 (how the approval comparison is run).
- As built by Phase F: the follow-up study runs A6 again in the same seeded order (140 trials, pairs sharing hours and build); A7's $150 limit is checked before each trial, with the key's own limit as the backstop; every trial's environment drops `ANTHROPIC_API_KEY`.
