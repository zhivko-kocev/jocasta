# 0510 An exploratory harness study after the full run: skill sets and three more arms

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
He asked to "try different skill approaches or maybe mix (karpathy skills, matt pocock, ponytail …) and see whatever works". Does a study compare them, with which arms, how big, and when?

## Facts
Verified 2026-11-01 (`D:/thesis/sources/proposals-2026-11/skill-approaches.md`, `humanlayer.md`, `stencil-playbook.md`, `tools-and-pi.md`).
- The "Karpathy skills" are one four-principle CLAUDE.md written by Jiayuan (forrestchang), not by Karpathy, with "MIT" in its README and no LICENSE file. Matt Pocock's skills are 39 small skills (MIT); his `tdd` and `code-review` stop to ask the user. Ponytail 5.3.3's AGENTS.md is byte-identical to `config/ponytail.md`. Superpowers (MIT) is a mandatory workflow whose bootstrap forces its skills and brainstorming before plan mode.
- A variant appends one prompt file only (`append_system_prompt`: the last one wins, `evals/variants.ts`); skill folders load through `skills.customDirectories`, and whether a trial read a skill is not counted today.
- 10 of the 30 main tasks bring their own AGENTS.md, CLAUDE.md or skills, which every arm loads.
- The pilot's medians: $0.53 API-equivalent and about 8.5 awake minutes per trial with two at once. About 90 trials per arm detect only ~20-point pass-rate differences (0482).
- The tool roster: "Limit it to five essential tools and you get `36.6s`, ahead of Codex's `42.2s`" (Stencil's playbook); 0422 kept omp's defaults ("no prompt diet").
- HumanLayer reports `<important if="…">` blocks gave "noticeably better adherence", without a rigorous explanation, and moved its context warning to 100k tokens on 1M windows; Jocasta compacts at 300k (0448).

## Options
| Skill study | Pros | Cons |
|---|---|---|
| Stage 1 screen, then decide | 50 trials, ~$27, ~7 h; he sees it before more | A screen only |
| Both stages | Stage 2: K0 and up to 3 survivors × 30 tasks × 3 reps (360 trials, ~$191, ~46 h) | Quota and time; pass rate still shows only ~20-point gaps |
| Not now | No quota | Future work only |

More arms in stage 1 (10 trials each, about $5): a lean tool roster; conditional rules; a 100k compaction point.

## Recommendation
Stage 1 screen, then decide; the lean tool roster added.

## Choice
- After the pre-registered A0–A4 run, never interleaved with it, an exploratory study on A1's pane config, only the instructions changing:
  - K0: no style text;
  - K1: Ponytail (A1, reused from the full run);
  - K2: the Karpathy set;
  - K3: Ponytail with the Karpathy set's §3–4;
  - K4: Ponytail with three Pocock skills (`tdd`, `diagnosing-bugs`, `codebase-design`);
  - K5: Ponytail with three Superpowers skills, without its bootstrap.
- Stage 1 also has three arms: the lean tool roster (read, edit, write, bash, grep), conditional rules (Jocasta's own test rules in `<important if>`, Ponytail verbatim), and the 100k compaction point.
- Both stages run: stage 1 on 10 main tasks × 1 rep; stage 2 on all 30 × 3 reps for K0 and up to three stage-1 survivors.
- Variants gain a `skills:` key; each trial's row counts its skill reads.
- Two follow-on arms, each only if the data calls for it: a quieter-output arm if the post-hoc measures show test output is a real share of tokens; conditional blocks in the boss manual too if the conditional-rules arm gains.

## Reason
"Both stages"; for the extra arms, "Lean tool roster", "Conditional rules", "100k compaction point"; then "Quieter-output arm if A5 shows a load", "If the arm gains, apply it to the boss manual", "Wait for the 100k arm".

## Consequences
- Differs from the recommendation: both stages, and all three extra arms.
- Outside 0482's pre-registration: reported as exploratory, its analysis written down before its runs.
- The study's arm files pin each set's text at a commit with its licence notice; the Karpathy set's missing LICENSE file is named.
- Claims about a set rest on lines changed, cost and time paired per task, with pass rate read as non-inferiority.
