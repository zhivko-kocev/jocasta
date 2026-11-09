# 0421 Ponytail as an eval variant, kept by the rule

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice E (0375): is Ponytail tried, and how?

## Facts
Verified 2026-10-14.
- Ponytail is `DietrichGebert/ponytail` (MIT, npm 5.2.0): one "lazy senior dev" prompt; its short `AGENTS.md` is about 860 tokens, its skill about 1,144.
- Its own benchmark (Claude Code, Opus 5.5, 39 tasks × 5, bash off): about 53% fewer lines, 45% fewer output tokens, 26% lower cost, the same pass rate.
- omp loads skills from `~/.omp/agent/skills/` or `.omp/skills/`, and always-on text from AGENTS.md files.
- The omp baseline is 36/36 (`<data>/evals/runs/2026-10-13T19-23-14-654Z-default`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Eval variant, keep by the rule | Measured on his suite | About 36 paid trials |
| On-demand skill, no eval | Cheap | Unmeasured |
| Drop E | Nothing to do | Leaves a likely win untested |

## Recommendation
Eval variant, keep by the rule.

## Choice
Ponytail's short `AGENTS.md` text becomes an eval variant, run 12 × 3 and compared with the omp baseline run of 2026-10-13 (36 trials, as he was told); on a KEEP verdict from 0375's rule, `jarvis setup` adds it to the threads' context; on REVERT it is not used. The verdict is reported to him.

## Reason
"Yes, eval it, keep by the rule" (after the rule was explained).

## Consequences
- Carries out slice E of 0375.
- Amended by 0427: kept on his word after a marginal KEEP; added by the pane command from `config/ponytail.md`.
