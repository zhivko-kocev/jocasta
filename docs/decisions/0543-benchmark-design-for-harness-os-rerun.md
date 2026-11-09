# 0543 The rerun's design: seven arms, a follow-up study of the layer's parts, H8 extended with four threads, a keep rule, and a pacer that waits for every account

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
Every benchmark runs again on the harness-OS build (0533). Which arms does the task run have, where do the on/off comparisons of 0541 and 0542 go, how is K11's multi-thread scenario run, when does a tried feature stay, and how does the quota pacer treat two accounts?

## Facts
Verified 2026-11-08 (`bench/README.md`, `docs/status.md`, 0482, 0510).
- The task run is 30 main tasks × 3 repetitions; the arms stack: A0 omp alone, A1 + the pane config, A2 + plan first, A3 + the review; A4 is Claude Code; A5 is his case study. The pilot's median trial was $0.53 API-equivalent and about 8.5 minutes with two at once.
- The new records ask for: the whole layer (0532), opencode bare (0537), prewalk and keep-alive pings on and off (0541), the code index and the `check` tool on and off, and pitchfork against omp's daemons (0542), plus a multi-thread scenario, since one-thread tasks can't show what shared services save.
- Each extra main arm is 90 trials, about $48 API-equivalent and some 6 hours of VM time at two trials at once; 30 × 3 detects only differences of about 20 points (0482).
- H8 ran 2, 4 and 8 threads on one repo; at 8 the VM ran out of memory once and peaked at 17.6 of 18.3 GB on the clean rerun (0531).
- The pacer waits while any window of any signed-in account is at 85% or more (`evals/pacer.ts`); on 2026-11-05 the second account's 7-day window held the run while the first stood at 7%. omp spreads sessions across his accounts.

## Options
| Item | Options (pros / cons) |
|---|---|
| Main arms | 7 arms (A0–A4, A6 = A3 + the layer, A7 = opencode bare; 630 trials) · 6 arms without A2 (540; loses plan-first alone) · Everything as arms (900+; weeks of quota) |
| On/off parts | A follow-up study, 10 tasks × 2 reps each (big differences only) · Full size, 30 × 3 each (smaller differences; doubles the quota) |
| K11 | H8 extended, 4 threads, layer on and off, 3 runs each · 8 threads (out-of-memory risk) · Skip (savings unmeasured) |
| Keep rule | Cheaper, not worse · Pass rate first |
| Pacer | Wait when every account is full · As now, any account (stalls for days) · One account only |

## Recommendation
7 arms; a follow-up study; H8 extended with 4 threads; cheaper, not worse; wait when every account is full.

## Choice
- **Main run:** A0–A4 as 0482 has them, plus **A6** = A3 + the whole harness-OS layer, and **A7** = opencode, bare, on an Anthropic API key with a $150 spend limit (0537; opencode's only Anthropic sign-in, 0506). 30 tasks × 3 repetitions × 7 arms = 630 trials, priced with the benchmark's one price table (0538).
- **Follow-up study,** after the main run: each part switched off from A6 (the code index, the `check` tool, pitchfork in place of omp's daemons) and each tried on top of it (prewalk, keep-alive pings), on 10 tasks × 2 repetitions per comparison, paired against A6.
- **K11:** H8 extended: H8's repo with 4 threads at once through Jocasta, the layer on and off, 3 runs each, in the VM; it measures cost per finished thread, shared check hits, index reuse and memory.
- **Keep rule,** written before any run: a tried feature stays when its paired cost per passing task is lower and its pass rate is not significantly lower (McNemar with Holm's correction).
- **Pacer:** it waits only while every signed-in account's 5-hour or 7-day window is at 85% or more; a rate limit on a full account counts as an infra error and is retried, as 0482's rules say.

## Reason
"7 arms"; "A follow-up study"; "H8 extended"; "Cheaper, not worse"; "Wait when all are full".

## Consequences
- Amends 0482: the arms, the follow-up study, H8's extension, the keep rule and the pacer.
- 0510's exploratory harness study still runs after the main run, beside this follow-up.
- The thesis reports the follow-up study as exploratory: 20 trials per comparison show only large differences.
- A6's arm builds the layer into each trial (the ledger reads its transcript, `report_result`, the task file's brief, the cached checks and the index); its exact setup is listed as built.
- Amended by 0545: the follow-up study also compares omp's "Approve and execute" with "Approve and keep context", and Ponytail with its test rules in an `<important if=…>` block against plain Ponytail.
- Amended by 0567: A6 approves plans with "Approve and execute" (not `--plan-yolo`); every arm sets `providers.cacheWarming: streaming` except the keep-alive variant.
- Amended by 0571: `f-check-off` leaves the follow-up study (five comparisons, 120 trials); A6's check runs as the product does on a repo without a mise check.
