# 0511 What the analysis adds and what the thesis takes from the research

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
The 2026-11-01 research (`docs/research/enhancement-proposals.md`) offers measures, readings and sources that change neither the product nor the pre-registered runs. Which does the analysis add, and which does the thesis use?

## Facts
Verified 2026-11-01.
- The full run's transcripts hold every tool call: test commands and their output sizes, pipes, edits, and each turn's token counts (`D:/thesis/data/bench/tasks/pilot/transcripts/`).
- Destiner gives no reasons for "'Plan mode' was a thing; now it's legacy. Human code review is dying."; they are one-line observations (https://destiner.io/blog/post/pi-is-the-last-harness/). A1 → A2 adds plan-first and A2 → A3 the review step (0482).
- One-shot task benchmarks "do not measure whether code stays easy to change" (https://www.humanlayer.dev/blog/humanlayer-resources).
- About 142 of 549 tests name a record (a regex count over test names).
- Stencil's playbook draws five consequences from four ways a harness is used: "One authoritative session… A trusted control plane… Bounded work… Explicit compatibility… Views are projections." He chose them as the thesis's checklist; the playbook explained simply is in `D:/thesis/sources/playbook-explained.md`.
- HumanLayer open-sourced its orchestrator (Apache 2.0; macOS desktop app, a daemon over Claude Code, Codex, OpenCode and Pi, a cloud API).

## Options
Each item was asked on its own with its alternative (measure or not; build or name as a limitation; use in the thesis or not); his picks are below.

## Recommendation
Every item below, with maintainability named as a limitation.

## Choice
- **Post-hoc measures** from the full run's transcripts, labelled exploratory: test-output bytes and reruns without an edit, piped test commands in review turns, type and lint escape hatches added, peak context per trial, and any access outside the trial root.
- **Readings stated before the results:** A1 → A2 answers "plan mode is legacy" and A2 → A3 "human code review is dying"; A5's interventions are coded for context lost between the boss's brief and the thread.
- **Maintainability** is named as a limitation of chapter 5; no staged-requirements eval.
- **Record-to-test traceability:** a read-only script in `D:/thesis/analysis/` gives the share of records a test names, for chapter 3.3.
- **The playbook in the thesis:** its five consequences are the checklist chapters 3 and 6 score Jocasta against; the playbook is explained and its philosophy discussed in the thesis.
- **Related work:** HumanLayer's open-source orchestrator is Jocasta's closest relative in chapter 2: the same kind of tool, with the opposite deployment choices.

## Reason
"Post-hoc transcript measures", "Readings stated before the results"; "Name it as a limitation only"; "Record-to-test traceability number"; "(1) The five rules as the thesis checklist" and "i loved it and i want it sumarized simplified explained and also the philosophy as part of the thesis"; "Closest related work in chapter 2".

## Consequences
- Adds to 0482's analysis without changing its primary measures; the readings are written down here, before the full run's results.
- The evidence pack for chapters 1, 2, 6 and 7 carries the playbook, HumanLayer and the other sources (bibliography [222]–[248]).
- Amended by 0527: the playbook is explained in 2.7 with omp², the checklist gets its own section (6.2) and appendix (Прилог Д), 6.3 covers the path to omp², and three voices of the new wave join 6.5.
