# 0508 The full run goes ahead, with three fixes from the pilot

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
0482's pilot ran 25 trials and its quota per trial and time were shown to him. Does the full run start, and does the harness change first?

## Facts
Verified 2026-11-01 (`D:/thesis/data/bench/tasks/pilot/report.md`, `notes.md`).
- 25 trials graded, 2 infra errors run again; 11 passed; $17.18 API-equivalent; 8.5 awake minutes per graded trial with two at once, so 450 trials take about 64 hours with the host awake. 4 trials ran across a host sleep.
- Quota: the 14 trials of 2026-11-01 evening raised account 1's 7-day window by 3 points (at least 1 of them this agent's own session) and account 2's by none. The pacer pauses at 85% of the most used account's window (`evals/pacer.ts`, `readQuota` in `src/frontier/server/quota.ts`).
- The trial branch is `eval/<case id>-<rep>` (`evals/runner.ts`, `makeTrialRepo`), and a benchmark task's id ends in its fix's PR number. The branch reached the agent through git output in 3 of 25 transcripts; jestjs__jest-16439 A0 wrote the PR's link into `CHANGELOG.md`, which the leak scan flagged, while A1, which never saw the branch, wrote `pull/TBD`. No trial fetched the PR.
- tox-dev__tox-4074 A1 reported +214,912 lines; the trial repo went with its trial root, so nothing shows which files.
- A3's review follow-up runs the project's tests; whole suites took 30 minutes (rclone) and about 28 (tox), and tox's A3 trial hit the 60-minute limit and counts as a fail. The limit is the bench's `--timeout` default; 0482 names none.

## Options
| Option | Pros | Cons |
|---|---|---|
| Go, with the fixes below | The run starts on a harness that closes what the pilot found | The harness differs from the pilot's |
| Go exactly as the pilot ran | No change between pilot and run | The PR leak and the 60-minute cut stay |
| Two reps (300 trials) | About 43 hours, less quota | Below 0482's ~20-point detection floor |
| Not yet | | The run waits |

| Branch | Pros | Cons |
|---|---|---|
| `eval/trial-<rep>` | No PR number for any arm | One line changes |
| Keep `eval/<case id>-<rep>` | The harness as the pilot ran | An agent with web search could look the fix up |

| `lines.log` | Pros | Cons |
|---|---|---|
| Add it | Line-count outliers can be explained | A new file per trial |
| Don't | The harness as pre-registered | Outliers stay unexplained |

| Trial limit | Pros | Cons |
|---|---|---|
| 60 minutes | As the pilot | A3 cut short on slow suites |
| 90 minutes, every arm | Fewer cut A3 trials, equal for all arms | Long trials take longer |
| 120 minutes, every arm | Fewest cuts | A stuck trial costs up to 2 hours |

## Recommendation
Go with the fixes; `eval/trial-<rep>`; add `lines.log`; 90 minutes for every arm.

## Choice
- The full run starts: the 30 main tasks × 3 reps × A0–A4 (450 trials), seed 482, two at once. It follows H2, H3, H5, H6 and H8 in the VM, and setup's files are out of tester's omp folder before it starts, so A0 runs on omp's defaults.
- A trial's branch is `eval/trial-<rep>`, without the case id.
- Each trial's logs gain `lines.log`: the per-file added and removed lines behind the row's line counts.
- Every arm's trial limit is 90 minutes (`--timeout 90`).

## Reason
"Go, with the fixes picked below"; "Rename to eval/trial-<rep>"; "Add lines.log"; "90 minutes, every arm".

## Consequences
- Amends 0482's run conditions before the run, as 0482 asks of any change.
- The pilot's trials keep the old branch name and limit; the pilot is not part of the analysis.
- The VM's repo copy is synced before the run and left alone while it runs. The run's script refuses beside a live run, without GitHub, or with setup's files in omp's folder, as the pilot's did.
- `bench/README.md` names the branch, `lines.log` and the limit.
