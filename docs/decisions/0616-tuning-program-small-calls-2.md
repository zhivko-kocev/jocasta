# 0616 Tuning program: four more small calls

- Status: v1
- Kind: product, process
- Phase/slice: program
- Date: 2026-11-14

## Question
Six small calls were left from the research. They were asked as one tick-list (0089). Which are taken?

## Facts
Verified 2026-11-14.
- **The advisor:** in the window 2026-11-10 to 11-14 it cost $232.94 over 3,321 turns and wrote 173 notes. Of 60 notes sampled, 18 were acted on (30%): all 6 blockers, none of 15 nits. omp 18.20.0's `advisor.reviewInterval` (default 1) reviews every Nth eligible update and sends the skipped updates with the next review (`advisor/settings.ts`). The saving at 3 is estimated at $70 to $80 per such window; it is not measured.
- **The plan:** `AGENTS.md` tells every agent to read `docs/plan.md` before any work. The file is 393 KB, about 98k tokens. Section 3 (the decisions table, 140 KB) and appendix I (100 KB) are 61% of it; sections 1, 4, 5 and 7 are 32 KB. The machine checks of 0608 compare the records' index with the record files and appendix I's environment variables with the code. In the window, 126 of 2,979 tool calls read the plan.
- **The scorecard** (0610, 0614) holds Biome's counts, jscpd's duplicated share (0.18% in `src` and 1.98% in `test` that day) and the `bun audit` line. The weekly review (0470, 0583) runs per project. Jocasta finds a project's check as its mise task `verify` (0542).
- **The test workers:** `bun run test` runs one worker per core, 14 on his PC. With 34.8 of 39.5 GB of memory committed, that run died with out-of-memory errors and 60 false failures. With 4 workers the suite passes in about 100 s. Since 0610 the suite runs by itself after a turn that leaves edits, and a failure is given to an agent.
- **`eval` in threads:** its description is 1,246 tokens a session. In the window a thread's own session called it once in 335 tool calls; its subagents 101 times in 638, almost all Python that fetches and computes during research; 9 of those calls ran a shell command, which the guard does not see (0485).
- **Who decides small things:** 101 of 613 records say in their own text that his answer differed from the recommendation.

## Options
| Call | Options | Recommended |
|---|---|---|
| The advisor's cadence | Every third update; every update | Every third |
| What `AGENTS.md` tells agents to read | Sections 1, 4, 5 and 7, with lookups; the whole plan | The sections |
| How the scorecard reaches the weekly review | A mise task named `scorecard`; Jocasta computes it for Biome projects only | The mise task |
| Test workers | 4; one per core | 4 |
| `eval` in thread panes | Off; on | On |
| Builders choose more without asking (0363) | Widen; keep | Keep |

## Recommendation
The first four as in the table's last column; the last two unchanged.

## Choice
- **The advisor reviews every third update:** `advisor.reviewInterval: 3` in Jocasta's shipped omp settings, which setup offers to his own config.
- **`AGENTS.md` names what to read:** plan sections 1, 4, 5 and 7; a decision is looked up in the records' index, and a seam in appendix I, when the task touches one. Nothing is deleted from the plan.
- **The scorecard is a mise task named `scorecard`.** The weekly review runs it when the project has one, keeps each week's output in Jocasta's data folder, and gives this week's and last week's to the review. Jocasta's own task prints Biome's counts per rule, jscpd's duplicated share and the `bun audit` line.
- **`bun run test` runs 4 workers at a time.**
- **`eval` stays on in thread panes, and the builders' class stays as 0363 has it.** He asked for both to be explained; with the explanation (2026-11-15) he kept both.

## Reason
His ticks, and for the last two: "explain the two unticked ones", then `keep` for each.

## Consequences
- Amends 0593 (the shipped omp settings), 0470 and 0583 (the weekly review), and the first rule of `AGENTS.md`.
- A blocker from the advisor can arrive up to two updates later.
- An agent can miss a decision it did not look up; the review and the weekly drift review are what catch that.
- A project without a `scorecard` task has no scorecard.
- Plan appendix I gains the `scorecard` task and the file that keeps each week's output (0027).
