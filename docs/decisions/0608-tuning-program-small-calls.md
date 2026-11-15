# 0608 The tuning program's small calls

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-14

## Question
The tuning research (`docs/research/state-of-the-art.md`) left a list of small proposals. Which does he take?

## Facts
Verified 2026-11-14; the section numbers are the research document's.
- **herdr's config (section 6):** `herdr config check` with `HERDR_CONFIG_PATH` set reads a config file and names each unknown key. Nothing in `src/` or `mise.toml` runs it, so a mistyped or renamed key is ignored in silence at runtime.
- **The weekly review (sections 2 and 3):** once a week, for each project with merges, the service runs `omp -p` with nobody watching; its only limit is a 30-minute timer (`WEEKLY_REVIEW_MS`). The one run so far cost $10.88: the drift agent $10.32 over 42 turns, its advisor $0.26. omp's `--max-cost` stops a run when its own spend reaches the value.
- **Prewalk (section 9):** the follow-up arm `f-prewalk` is A6 with `--prewalk`. A6 starts each trial in plan mode, and omp turns prewalk's automatic switch off there, so the arm would measure nothing (read from omp's `prewalk.ts`, not run). Nothing records whether the switch happened in a trial.
- **Plan approval (section 9):** `f-keep-context` answers omp's plan review with "Approve and keep context". omp can answer the same review with a role for the work that follows; no arm tests that.
- **omp² (section 9):** no release. npm's dist-tags hold only `latest: 18.20.0`, and the `omp2` branch last moved on 2026-09-04.
- **Stale rule texts (section 10):** in a published study a stale instruction was followed whenever the task led through what it named: the test command in 78 of 80 runs (arXiv 2611.09802). Jocasta's always-on texts name commands and paths; nothing checks that they still exist.
- **Process rules checked by hand (section 11):** the decisions index has one row per record file; every `JOCASTA_*` variable is in plan appendix I; Bun's version is the same in its three files. All three held on the day they were counted.
- **whiteboard (section 5):** Jocasta pins v0.2.4; upstream is at v0.5.2. Whether Jocasta's seams still fit was not known.
- **Session titles (section 2):** they cost $0.05 in the four measured days.
- **Bun's cache (section 7):** with the cache on the repos' volume, installs were 1 to 3 s faster, measured with 2 to 3 runs per cell.

## Options
| Question | Options | Recommended |
|---|---|---|
| `herdr config check` | In setup and in a test that skips without herdr; in setup only; neither | Setup and the test |
| A cost cap on the weekly review | `--max-cost` on that run; none | On that run, at $25 |
| The prewalk arm | Replace `f-prewalk` with a clean arm on A1 with a detector; keep it | Replace it |
| Plan approval on a cheaper role | Add `f-keep-smol`; `f-keep-context` only | Add it |
| Watching for omp² | A mise task that prints the two facts; by hand | The task |
| Always-on texts and process rules | Two deterministic checks in `mise run check`; by hand | The checks |
| whiteboard v0.5.2 | A spike now; a spike after the benchmarks | After the benchmarks |
| Session titles from the title role | Yes; no | No |
| Bun's cache on the repos' volume | Yes; no | No |

## Recommendation
As in the table's last column.

## Choice
- **herdr:** after setup writes `herdr.toml` it runs `herdr config check` on that file and prints what it reports. A test checks the shipped file and skips where herdr isn't installed.
- **The weekly review** runs with `--max-cost 25`, about 2.3 times the one run measured. It caps nothing else: not his panes, not threads, not the boss.
- **Prewalk:** `f-prewalk` becomes `f-prewalk-clean`: A1 with `--prewalk`. Each trial records whether the switch to the cheaper model happened. Its baseline is A1, run again inside the follow-up study on the same tasks, as A6 is for the other arms.
- **Plan approval:** a new arm `f-keep-smol` beside `f-keep-context`: the plan approved with its context kept, the work done on the `smol` role.
- **The follow-up study** is then A6, A1 and four arms (`f-keep-context`, `f-keep-smol`, `f-ponytail-if`, `f-prewalk-clean`) on 10 tasks with 2 repetitions: 120 trials.
- **omp²:** `mise run omp2-watch` prints npm's dist-tags for omp and the tip of its `omp2` branch. Plan 5.5's paragraph stays.
- **Two checks in `mise run check`**, as tests:
  - every command and path named in the always-on texts (`config/ponytail.md`, the boss manual, the repo's `AGENTS.md`) still resolves;
  - the decisions index equals the record files, every `JOCASTA_*` variable is in appendix I, and Bun's version is equal in its three files.
- **whiteboard v0.5.2:** a spike now, in `D:/jocasta-spikes/`, with `pnpm install` of whiteboard's own packages allowed there only. A bump is its own decision.
- **Not taken:** session titles stay on the session's model, and Bun's cache stays where it is.

## Reason
His ticks. On the cap he asked "what is the cap for?" and took $25 once it was answered: it is for the one run nobody watches. The whiteboard spike was offered unticked and he ticked it.

## Consequences
- The prewalk and plan-approval changes amend 0543 and 0545; the follow-up study grows from 80 to 120 trials.
- The cap amends 0583's weekly review. A review stopped by the cap is reported like one that ran out of time.
- Plan appendix I gains the `omp2-watch` task (0027).
- The whiteboard spike's report decides whether a bump is asked.
- The same list carried a pre-push hook, unticked; his answer to it became the push gate's own record.
