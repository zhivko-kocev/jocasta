# 0500 The benchmarks run on omp 18.13.1

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
0482 pins omp 18.12.6 for the whole run. His PC and the VM have 18.13.1, and omp ships almost daily. Which omp do the benchmarks run on?

## Facts
Verified 2026-10-27.
- npm dates: 18.12.6 on 2026-10-21, 18.13.0 on 2026-10-26, 18.13.1 on 2026-10-27 01:04 UTC, 18.13.2 on 2026-10-27 18:17 UTC.
- The 0383 re-check of 18.13.1 (52 seams) found no change Jocasta needs. 18.13.0's only breaking change is internal (`snapshotJobs`/`buildJobResult`); new are `--wait-async`, `--no-context-files`, `--max-cost`, `tools.restrictedFolders` and a `policy` object in `omp usage --json`.
- The approvals, diffed between the two npm packages: `tools/shell-tokenize.ts` and the bash rule matching in `tools/bash.ts` are unchanged. `tools/approval.ts` only adds `tools.restrictedFolders` (a listed folder makes every call touching it ask), and with none set it resolves as before; Jocasta sets none. One built-in prompt narrowed: `dd … of=/dev/…` no longer asks for `/dev/null`, `/dev/zero`, stdout, stderr, `tty` or `/dev/fd/N`.
- The campaign (0481) tested v1 on 18.12.6. 0482: a change before the run is a new record.

## Options
| Option | Pros | Cons |
|---|---|---|
| 18.12.6, as registered | No deviation; the version the campaign tested | The VM needs an older omp; A5 on his PC runs on whatever version he has then |
| 18.13.1, with a new record | What he uses every day and what a new user installs today | A change from the pre-registration, named in the thesis |

## Recommendation
18.12.6, as registered.

## Choice
Every benchmark run (H1's live check, H2–H8, the pilot, the task benchmark and A5) uses omp 18.13.1. Claude Code 2.1.296 and herdr 0.9.5 stay pinned.

## Reason
"18.13.1, with a new record" (differs from the recommendation).

## Consequences
- Amends 0482's pins: omp 18.13.1, Claude Code 2.1.296, herdr 0.9.5.
- Differs from the recommendation; the thesis names the change as one made before the run.
- Neither the VM's omp nor his PC's is updated until A5 is done; an update before then is reported with A5.
- Amended by 0535: the harness-OS build and the benchmarks on it pin the omp that is latest when that build starts; his PC's omp is already 18.18.2 (2026-11-08).
- Amended by 0589: the benchmarks run on omp 18.18.6.
