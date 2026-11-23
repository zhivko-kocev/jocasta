# 0655 Normal threads on Opus 5.5 at medium

- Status: v1
- Kind: product
- Phase/slice: fork build, review
- Date: 2026-11-23

## Question
The model study on the 30 tasks ran on 2026-11-22. Does any role change (`fork-build.md` For him 17)?

## Facts
Measured 2026-11-22 on omp 19.1.0, one run per task (`docs/research/settings-trials.md`; data in `D:/thesis/data/bench/tasks/runs/models-2026-11`):

| Arm | Passed of 30 | $ per task | Median minutes |
|---|---|---|---|
| A1, Opus 5.5 high | 22 | 0.403 | 1.4 |
| Opus 5.5 medium | 22 | 0.303 | 1.1 |
| Opus 5.5 xhigh | 22 | 0.592 | 1.9 |
| Fable 5.2 high | 23 | 0.867 | 1.7 |
| Fable 5.2 xhigh | 26 | 1.464 | 2.6 |
| Sonnet 5.5 high | 21 | 0.087 | 0.6 |
| Sonnet 5.5 medium | 19 | 0.053 | 0.3 |

No pass difference against A1 is established (exact sign test on the tasks where the two differ: Fable 5.2 xhigh 4–0, p = 0.125; Opus 5.5 medium 1–1, p = 1.0); the cost and time differences are.

## Options
- **Opus 5.5 at medium for normal threads.** Pros: the same 22 passes for 25% less and faster. Cons: one run per task; a small loss can hide in 30 tasks.
- **Keep Opus 5.5 at high.** Pros: no change. Cons: pays 33% more for no measured gain.
- **A second run of high against medium first (about $21).** Pros: separates the arms from noise. Cons: cost and a wait for a change the data already supports.

## Recommendation
Opus 5.5 at medium for normal threads; the boss and heavy threads as they are.

## Choice
His answer of 2026-11-23: Opus 5.5 at medium. The snapshot's `modelRoles.default` (`config/omp/config.yml`) becomes `anthropic/claude-opus-5-5:medium`; it is the role of every normal thread and project pane, and of his plain omp. The boss (`config/boss.yml`, Fable 5.2 high), heavy threads (`slow`, Fable 5.2 xhigh), `plan`, subagents and the fallback chains stay as they are. Every benchmark arm keeps Opus 5.5 at high (A0's pin, A6's roles, A4's `--effort high`): the main run compares harnesses on one model, and the study's A1 is its baseline.

## Reason
His answer in the review form, and "keep every arm at Opus 5.5 high" for the main run.

## Consequences
- Amends 0590: `default` is Opus 5.5 at medium.
- The thesis says the product ships medium, from the study's 22 against 22, while the main run measures at high.
- His installed Jocasta takes the new value at its next `jocasta setup`.
