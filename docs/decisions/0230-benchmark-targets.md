# 0230 Benchmark targets

- Status: decided
- Kind: product
- Phase/slice: 1/1.10 (raised in 2.5)
- Date: 2026-10-08

## Question
Which benchmarks must Jarvis beat?

## Facts
Verified 2026-10-08 (https://www.tbench.ai/leaderboard/terminal-bench/2.0): Terminal-Bench 2.0 ranks harness and model pairs, accepts harnesses as custom agents, and submissions may not change timeouts.
- SWE-bench (Verified, Pro) ranks repository bug fixes.
- Jarvis's own evals (1.10) run scripted cases and cases mined from his sessions (0163), with hillclimbing (0167, 0168).
- Frontier, voice and the boss don't appear in public benchmarks; the harness in a tab does.

## Options
| Option | Pros | Cons |
|---|---|---|
| Terminal-Bench 2.0 | Harness-sensitive; public | — |
| SWE-bench (Verified or Pro) | Widely cited | — |
| Jarvis's own evals | Measures his work | Not comparable to others |

## Recommendation
None was marked in the question.

## Choice
All three: Terminal-Bench 2.0 and SWE-bench (Verified or Pro) for public comparison, and Jarvis's own evals (1.10) for his work. They are the measure the pruning pass uses (0229).

## Reason
Not given.

## Consequences
- How each runs (adapters, models, budgets, target scores) is decided with 1.10's evals when the build reaches it, or reopened at the pruning pass; public benchmark runs cost model tokens and are asked before they run.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
