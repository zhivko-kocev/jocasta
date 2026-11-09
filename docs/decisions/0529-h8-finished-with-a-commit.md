# 0529 H8 counts a thread finished when it is idle or done with a commit on its branch

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
H8's 8-thread run on 0520's build measured before any thread had worked. When does H8 count a thread finished, and when does its 8-thread run happen?

## Facts
Verified 2026-11-02 in the VM.
- H8 polls `thread_status` every 10 s and measures once every thread of the run reads idle or done (`bench/h8-parallel.ts`).
- The 8-thread run: its 8 start boxes were allowed at 19:45:51–52Z, and H8 measured at 19:46:27Z with no plan approved: 0 flagged, 0 conflicts, where a same-file pair that worked always conflicts (`D:/thesis/data/bench/h8/h8-jocasta-n8-early/`).
- Eight omps starting at once on 4 virtual CPUs read idle before their briefs land, and a waiting plan reads done in herdr (0517's H8 notes).
- The 2- and 4-thread runs of 14:56–16:00Z worked: plans approved, and in each a same-file conflict found and flagged before its merge.
- Each brief ends "Commit when done."

## Options
| Option | Pros | Cons |
|---|---|---|
| Finished = idle or done with a commit, rerun now | The brief's own end; a thread that asks instead runs to the 60-minute limit, as 0517's did; the 8-thread run before the full run | Amends 0482's H8; the full run starts about 70 minutes later |
| Same fix, rerun after the full run | The full run starts right after H3 | The 8-thread row waits about 64 hours, then needs the sandbox and setup's files back |
| Report 2 and 4 threads only | No change | No 8-thread row, the hardest case for parallel threads not interfering |

## Recommendation
Finished = idle or done with a commit, rerun now.

## Choice
A thread counts finished when `thread_status` shows it idle or done and its branch has a commit beyond `main`; a thread that asks instead runs to H8's 60-minute limit. The 8-thread run runs again right after H3, on a sandbox started clean, before the full run; the early run's rows stay in the kit as `h8-jocasta-n8-early`.

## Reason
"Finished = idle or done with a commit, rerun now".

## Consequences
- Amends 0482's H8 before the run, as 0482 asks of any change.
- The full run starts after H8's 8-thread run.
- Amended by 0530: the 19:45Z run's threads, it turned out, sat at omp's setup wizard (setup's files were out of tester's omp folder from 19:31Z), not before their briefs had landed; the rule stands, since a thread at the wizard has no commit.
- Amended by 0531: the 8-thread run runs once more on a clean sandbox, and running out of memory again is its result.
