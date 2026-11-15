# 0619 The omp settings the trials decided

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-15

## Question
Which of the open omp settings change, by what the trials of 2026-11-15 showed (0618)?

## Facts
Verified 2026-11-15 on omp 18.20.0 (`docs/research/state-of-the-art.md`, section 13; scripts and raw results in `D:/jocasta-spikes/sota/settings-trials/`, outside the repo). omp on his PC became 18.20.2 while the trials ran: 13 of the 144 trials and the 6 `syncBacklog` trials ran on it, and the fixed-token differences below were measured again on it and are the same (0623).
- **The trials:** fixed tokens with no model call; counts over his omp session files of 2026-11-10 to 11-15 (208 files, $1,264.79 at omp's prices); 144 real trials, six arms on the 12 cases of `evals/cases/`, two reps each, Opus 5.5 at high, each arm one setting away from a Jocasta pane's config with the advisor off. Every trial passed, so the trials show cost and time at equal pass, and nothing about quality.
- **The advisor in `omp -p`:** on, the trials cost 61% to 83% more and ran 21% to 29% longer. It wrote 8 notes in 72 trials, and all 8 arrived after the run's last turn. With `advisor.syncBacklog: strict`, 1 of 2 notes in 6 more trials was followed by a turn. His weekly review, an `omp -p` run, paid $0.26 to its advisor beside $0.30 of its own and got no note.
- **The advisor's cadence:** against every update, every third update cost the advisor 17% less (95% interval 8 to 27) with half the requests, at equal pass. Replayed on his sessions it is an estimated 28% to 34% less under project panes. Most of the advisor's spend is cache writes, which a skipped update does not save.
- **The advisor in the boss:** $1.12 in five days, one session, one nit that came after the last turn.
- **Subagent depth:** `task` was called once in 137 subagent sessions; that call started two subagents. The tool and its prompt text are 1,495 fixed tokens in every subagent session. omp's bundled `reviewer` may start `scout` subagents; none did in the window. With `task.maxRecursionDepth: 1` a pane's session still starts subagents and a subagent cannot (read in omp's `task/types.ts` and `task/executor.ts`).
- **Cache retention:** `short` (5-minute entries) cost 23% less in trials with no pause. Replayed on his sessions: 6% less in all, 17% less for subagents, 19% less for threads, but 17% more for the advisors of project panes and 87% more for the boss, where he pauses. The token counts are the same or higher. omp's `auto` writes 5-minute entries on an API key and 1-hour entries on a subscription login.
- **Three always-on settings in his sessions:** `tools.intentTracing` is 1.4% of output tokens and 41 fixed tokens; `shellMinimizer` rewrote 1.4% of bash results and its raw output was fetched back for 13 of 101; `read.summarize` answered 222 of 4,776 reads (4.6%), leaving out about 1.9 MB, and 60% of those were followed by a re-read of the same file: 1.0 MB read back in 149 more calls. Its net is not measured, and its sign is unclear.
- **`--thinking auto`** cost 24% less at equal pass and chose `low` or `medium` in every trial.

## Options
Each setting: change it, or leave it.

## Recommendation
None was asked: the trials decide (0618). He was shown the table on 2026-11-15 and overruled nothing.

## Choice
- **The weekly review runs without the advisor:** `advisor.enabled: false` in `config/headless.yml`.
- **The benchmark's base arm pins the advisor off** (`evals/variants/a0.yaml`), so no omp arm takes it from the machine's own settings.
- **A subagent in a project or thread pane starts no subagents:** `task.maxRecursionDepth: 1` in the panes' overlay, and in arm A1, which mirrors the overlay.
- **Unchanged, now with a measurement:** `advisor.reviewInterval: 3` (0616); `providers.cacheRetention` stays omp's `auto`; `shellMinimizer`, `read.summarize` and `tools.intentTracing` stay on, as omp ships them, and get no benchmark arms: the first and the last move under 2% of their traffic, and about 3% of read bytes against 149 calls is below what the follow-up study can show; the boss keeps his advisor setting.
- **Not decided here:** the thinking level and the advisor in panes and threads trade quality (0620); `snapcompact.toolResults` and the compaction order need sessions long enough to compact and stay benchmark proposals.

## Reason
The measurements in the facts, under 0618.

## Consequences
- Amends 0593 (the shipped omp settings), 0470 and 0583 (the weekly review's run), and 0482 and 0567 (what the base arm pins).
- In a pane, omp's `reviewer` can no longer start `scout` subagents, and a research subagent cannot fan out. Both happened once or never in five days.
- A local `bun run evals` no longer pays for his advisor.
- Plan appendix I's settings lines gain the two keys (0027).
