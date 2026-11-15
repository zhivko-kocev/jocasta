# 0620 The advisor stays; the follow-up study gains an advisor arm and a thinking arm

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-15

## Question
The advisor is the second largest item of his spend, and no benchmark arm runs it. `--thinking auto` is cheaper on easy cases. Both trade quality, which the local trials cannot show (0618, 0619). What happens with each?

## Facts
Verified 2026-11-15 (`docs/research/state-of-the-art.md`, section 13). Of the 144 trials, 131 ran on omp 18.20.0 and 13 on 18.20.2 (0623).
- **The advisor in his sessions, 2026-11-10 to 11-15:** under 15 project panes $285.20, 66% of those sessions' own spend, for 12 blockers, 121 concerns and 28 nits; under 4 threads $15.69, 87%, for 5 blockers, 11 concerns and 2 nits. Together 23% of all spend. Every note arrived while its session was still working. In an earlier sample all 6 blockers were acted on and none of 15 nits (0616).
- **It runs on his `advisor` role** (Fable 5.2 at medium, 0590), every third update (0616).
- **No benchmark arm runs it.** The arms' omp has the advisor off, so the benchmark measures a Jocasta without it.
- **In `omp -p` its notes arrive after the run** (0619). A6's first turn is interactive omp in a herdr tab (0567); whether notes land in time there was not tried.
- **A thread's work is reviewed at its end** by Jocasta's own review, with or without the advisor.
- **`--thinking auto`:** 24% less spend (95% interval 10 to 33) and 8% less time at equal pass on the 12 local cases; it chose `low` in 12 trials and `medium` in 12, never `high`. Its classifier cost $0.001 in all.
- **The follow-up study** (0543, 0608) is 120 trials: two baselines and four arms, 10 tasks and 2 reps each. It shows large differences only.

## Options
| The advisor | For | Against |
|---|---|---|
| Keep it, and add an arm to the follow-up study | the keep rule weighs it on tasks with headroom | 20 more trials of quota; needs a pilot trial in A6's interactive turn |
| Keep it, no arm | no quota | its value stays unmeasured; the benchmark stays cheaper than his daily use |
| Off in thread panes | saves the 87% | loses the 5 blockers it caught mid-run in 2 of 4 threads |

| `--thinking auto` | For | Against |
|---|---|---|
| An arm of the follow-up study, on A1 | harder tasks can judge it | 20 more trials |
| No arm | no quota | the 24% stays untested where `high` matters |

## Recommendation
Keep the advisor and add the arm; add the thinking arm.

## Choice
- **The advisor stays as it is** in project panes and threads: his `advisor` role, every third update.
- **The follow-up study gains `f-advisor`:** A6 with the advisor on, on the `advisor` role's model and level of 0590 and every third update, against A6.
- **And `f-think-auto`:** A1 with `--thinking auto`, against A1.
- **The study is 160 trials:** two baselines and six arms.
- **Before `f-advisor` runs, one pilot trial in the VM shows a note landing inside A6's turn** (0588's single run of each script). If none can land there, the arm stops and he is asked (0020).

## Reason
His answers: `keep-and-arm`, and the tick on `think-auto-arm`.

## Consequences
- Amends 0543 and 0608 (the follow-up study's arms and size).
- The advisor's cost stays in his daily use until the study reports; the keep rule (0543) then applies to it as to any part.
- The thesis says which arms run the advisor and that the main run does not.
- Plan appendix I's benchmark lines gain the two arms (0027).
