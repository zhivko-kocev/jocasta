# 0651 The snapshot's settings come from trials on omp 18.22.2

- Status: v1 (decided 2026-11-19; for his review)
- Kind: product, benchmark
- Phase/slice: fork build, phase 7
- Date: 2026-11-19

## Question
Which of omp's settings does the snapshot (`config/omp/config.yml`, 0650) set away from omp's defaults, on which omp, and from what evidence?

## Facts
Verified 2026-11-18/19 (`docs/research/settings-trials.md`).
- He updated omp to 18.22.2 on 2026-11-18. Compared with 18.22.0 it adds three settings (`live.endpoint`, `live.apiKey`, `providers.webKeyless`), removes none and changes no default. The guard's decision files are byte-identical.
- 632 settings: P 156, J 32, F 79, Q 18, C 347. The no-model replay called 498 of 573 values inert and 75 live.
- The base passed all 60 of its runs on the 12 cases, and every model arm passed 36 of 36. The cases show cost and time at an equal pass rate, not quality.
- The ladder's greedy step picked `todo.eager: preferred`, `edit.mode: none` and `security.enabled: true`.
  - `todo.eager` cut cost by 21%, and its interval excludes zero.
  - `edit.mode: none` removes omp's edit tool. It cut time by 20%.
  - `security.enabled` gained 4.5% in time, inside the base's own drift of 3–4% over the night.

## Options
- **The ladder's three.** Pros: the trials' own result. Cons: one is inside the drift, and one changes how every edit on real code is made.
- **`todo.eager` and `edit.mode: none`.** Pros: the two real effects. Cons: the edit tool's loss isn't priced by small cases.
- **`todo.eager` only, `edit.mode` staying hashline.** Pros: the clear cost win, with no capability removed. Cons: leaves the time win of `edit.mode: none` unused.

## Recommendation
`todo.eager` only, with `edit.mode: none` tried as an arm of the 30-task study.

## Choice
His answer of 2026-11-19: `todo.eager: preferred`, and `edit.mode` stays hashline ("edit mode hashline let it stay"). The task study runs the seven model arms only. As built (`docs/research/fork-build.md` choices 80–83, 91):
- **The pin.** omp 18.22.2 is pinned everywhere current, and its seams and cites are re-checked.
- **The trials.** They ran from the start on 18.22.2, with no dollar budget: his subscription's windows are the limit. The base and the confirmation ran 5 times per case, and a run folder refuses a second omp version.
- **The snapshot.** `config/omp/config.yml` gains `todo: { eager: preferred }`, and nothing else from the trials.
- **The model study.** Twelve model and thinking arms are reported in `settings-trials.md`; none is adopted. The roles stay as 0590 and 0591 have them, until the 30-task study (record 0652) is shown to him.

## Reason
His words of 2026-11-18: "for development use latest"; "dont worry about the budget … deliver most optimal solution"; "test with different models different thinking efforts". Of 2026-11-19: "the first one + edit mode hashline let it stay".

## Consequences
- Amends 0623: the pinned omp is 18.22.2.
- Amends 0619: the settings decided by trials are now the snapshot's, from this run. 0619's values carried into `config/omp/` stay.
