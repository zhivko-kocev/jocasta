# 0671 The check queue learns what each check costs

- Status: v1
- Kind: product
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
Of the additions beyond the dock he ticked a shared check queue, "but very smart i guess deterministic, for example we record how much resources does some run of buiold or tests take and we know for the future planings to maybe even do both in parrallel if they can run but so we dont run off resources".

## Facts
Verified 2026-11-23: Jocasta runs a thread's checks (`check`, `done_when`, the review's and the push gate's; `src/frontier/server/checks.rs`) as soon as asked, so several threads' builds can run at once; the service already samples the machine's CPU and memory for the dock (0443) and walks process trees (`src/shared/proctree.rs`). omp's own bash jobs inside a thread are not Jocasta's checks.

## Options
- **Learned admission:** each run records its process tree's peak memory and CPU by project and command; a check starts when its recorded peak plus the running checks' peaks fits in free memory minus a reserve and the CPU in use stays under the core count; else it waits, first come first served; a check with no record runs alone once. A reserve of 2 GB, a setting.
- **The same with a hard cap of N checks at once.**

## Recommendation
Learned admission, reserve 2 GB.

## Choice
His answer of 2026-11-23: learned admission as above, with the reserve a setting (2 GB by default). The dock shows `checks: <n> running · <m> waiting` while any run or wait.

## Reason
His own rule for the agent, one heavy job at a time, made a measurement: checks that fit run side by side, ones that don't wait, and nothing guesses.

## Consequences
- A file of check costs in Jocasta's data folder.
- A waiting check counts toward its own time limit only once it starts.
