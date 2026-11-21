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
