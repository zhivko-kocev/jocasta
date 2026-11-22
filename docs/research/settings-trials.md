# The snapshot's settings trials (2026-11-18/19, omp 18.22.2)

Phase 7 of `fork-plan.md`, run with `jocasta-bench evals settings enumerate|replay|screen|confirm|models|report --dir
target/settings-run-2` on his PC, one trial at a time, from 22:48 to 07:50 UTC; record 0651 carries the outcome. A first
run on omp 18.22.0 was set aside when he updated omp mid-run (`fork-build.md` choice 81). Spend on omp's own meter:
$146.30 (no dollar budget, choice 82).

## Steps

| Step | What ran | Result |
|---|---|---|
| A, enumerate | omp's registry: 632 settings | P 156, J 32, F 79, Q 18, C 347 |
| B, replay | 573 values of the C and Q keys against the loopback stand-in, no model | 498 inert, 75 live |
| C, screen | base 12 cases × 5; 75 live arms on 4 cases, the best half on the other 8, the best third (24) on all 12 again | 3 arms out (`tools.format` `qwen3`, `gemma`, `xml`); 11 subsystem winners |
| D, confirm | the 11 winners combined, against the base, 12 cases × 5, interleaved | 58 of 60 against 60 of 60: it failed two runs of cases the base always passed; tokens ×0.78, time ×0.81 |
| D, greedy | winners added one at a time onto the best single arm, 12 cases × 1 | the ladder's pick: `todo.eager: preferred` + `edit.mode: none` + `security.enabled: true` |
| models | 12 model and thinking arms and the base, 12 cases × 3, interleaved | every arm 36 of 36 |

The 12 cases can't show quality: the base passed all 60 runs, and so did every model. What the trials measure is cost
and time at an equal pass rate.

## Drift

Screen and greedy compare an arm with the base's 60 runs from the start of the night; confirm and the model study
interleave the base. The base's own mean moved little over the night (18.5 s and $0.093 at the start, 17.8 s and $0.090
in confirm, 17.9 s and $0.092 in the model study): cost holds, and time drifts by 3–4%, so a time win of that size is no
evidence.

## Adopted (his answer of 2026-11-19)

| Setting | Evidence (screen, 12 cases) | Verdict |
|---|---|---|
| `todo.eager: preferred` | cost ×0.79, interval of the per-case difference −$0.017 to −$0.009; time ×0.98, interval across zero | adopted into `config/omp/config.yml` |
| `edit.mode: none` | time ×0.80 (−5.4 s to −2.5 s), cost ×0.97 | not adopted: it removes omp's edit tool, so edits on real code become bash or whole-file rewrites, which 12 small cases can't price; omp's `hashline` stays |
| `security.enabled: true` | time ×0.96 (−1.9 s to −0.1 s) in the screen; one run per case in the greedy step | not adopted: inside the night's drift |

The other eight winners (`includeWorkspaceTree: true`, `disassembler.backend: none`, `tools.format: hermes`,
`debug.enabled: false`, `includeModelInPrompt: false`, `personality: none`, `eval.js: false`,
`task.isolation.enabled: true`) stay at omp's defaults. `tools.format: hermes` doubled the cost and, in the greedy step,
took 63 s a trial against 18 s.

## The model study on the 12 cases

Interleaved with the base, 3 runs per case; every arm passed 36 of 36. Means per trial:

| Arm | Cost | Seconds | Tokens |
|---|---|---|---|
| Base: Opus 5.5 high | $0.092 | 17.9 | 82,957 |
| Opus 5.5 low | $0.053 | 13.3 | 53,368 |
| Opus 5.5 medium | $0.082 | 17.4 | 79,129 |
| Opus 5.5 xhigh | $0.092 | 21.2 | 80,563 |
| Opus 5.5 auto | $0.075 | 16.0 | 71,046 |
| Fable 5.2 medium | $0.128 | 14.0 | 56,613 |
| Fable 5.2 high | $0.153 | 19.2 | 61,357 |
| Fable 5.2 xhigh | $0.233 | 30.7 | 81,279 |
| Sonnet 5.5 medium | $0.030 | 10.6 | 53,318 |
| Sonnet 5.5 high | $0.035 | 12.0 | 59,853 |
| Haiku 5.5 high | $0.003 | 16.9 | 89,585 |
| Subagents on Haiku 5.5 low | $0.091 | 17.6 | 84,909 |
| Subagents on Opus 5.5 medium | $0.092 | 18.3 | 85,924 |

Nothing here is adopted: the roles are records 0590 and 0591, and which model does real work better is the 30-task study's
question (`fork-build.md` For him 17). The two subagent arms land within 3% of the base on cost and time.

## The model study on the 30 tasks (2026-11-22, omp 19.1.0)

The task bench's 30 main tasks (10 Go, 10 Python, 5 TypeScript, 5 JavaScript), each run once per arm on his PC, two
trials at a time (`jocasta-bench evals bench --set main --reps 1`, results in the thesis kit at
`data/bench/tasks/runs/models-2026-11`). A1 is Jocasta's pane config on Opus 5.5 at high; each other arm is A1 with
another model and a last `--thinking` (fork-build choice 88). 210 trials, none lost to the harness; $113.06 in all, by the
bench's ledger. A pass is the hidden tests passing on the arm's change.

| Arm | Passed | Cost | Cost per trial | Cost per pass | Median minutes | Median tokens |
|---|---|---|---|---|---|---|
| A1: Opus 5.5 high | 22 | $12.09 | $0.403 | $0.550 | 1.4 | 287,663 |
| Opus 5.5 medium | 22 | $9.08 | $0.303 | $0.413 | 1.1 | 206,741 |
| Opus 5.5 xhigh | 22 | $17.76 | $0.592 | $0.807 | 1.9 | 392,244 |
| Fable 5.2 high | 23 | $26.01 | $0.867 | $1.131 | 1.7 | 301,558 |
| Fable 5.2 xhigh | 26 | $43.93 | $1.464 | $1.690 | 2.6 | 493,382 |
| Sonnet 5.5 high | 21 | $2.60 | $0.087 | $0.124 | 0.6 | 103,310 |
| Sonnet 5.5 medium | 19 | $1.59 | $0.053 | $0.084 | 0.3 | 76,666 |

Paired with A1 task by task (95% bootstrap intervals over the 30 tasks):

| Arm | Only the arm passed | Only A1 passed | Δ passes | Δ cost per task | Δ minutes per task |
|---|---|---|---|---|---|
| Opus 5.5 medium | 1 | 1 | +0.00 (−0.10, +0.10) | −$0.100 (−0.215, −0.028) | −0.1 (−0.5, +0.5) |
| Opus 5.5 xhigh | 1 | 1 | +0.00 (−0.10, +0.10) | +$0.189 (+0.052, +0.388) | +1.1 (+0.1, +2.8) |
| Fable 5.2 high | 2 | 1 | +0.03 (−0.07, +0.17) | +$0.464 (+0.267, +0.779) | −0.0 (−0.5, +0.5) |
| Fable 5.2 xhigh | 4 | 0 | +0.13 (+0.03, +0.27) | +$1.061 (+0.606, +1.808) | +1.2 (+0.4, +2.4) |
| Sonnet 5.5 high | 0 | 1 | −0.03 (−0.10, +0.00) | −$0.316 (−0.555, −0.177) | −1.6 (−2.5, −0.9) |
| Sonnet 5.5 medium | 0 | 3 | −0.10 (−0.23, +0.00) | −$0.350 (−0.605, −0.199) | −2.0 (−3.4, −1.2) |

19 tasks passed in every arm and 4 in none (`tox-dev__tox-4104`, `prettier__prettier-20211`, `mvdan__sh-1409`,
`eslint__eslint-21373`); the arms differ on the other 7. Fable 5.2 at xhigh is the only arm whose gain over A1 clears its
interval: four more tasks (two of them, `cli__cli-14215` and `conan-io__conan-20371`, no other arm solved), at 3.6 times
A1's cost and nearly twice its time. Opus at medium matches A1's passes for a quarter less money. Sonnet at high misses one
task of A1's for a fifth of the cost and under half the time; at medium it misses three. One run per task and arm, so a
single task moves an arm by 0.03; the intervals carry that.

Nothing is adopted: the roles stay records 0590 and 0591 until he decides (`fork-build.md` For him 17).
