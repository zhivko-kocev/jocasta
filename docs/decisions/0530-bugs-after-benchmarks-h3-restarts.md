# 0530 The reruns' three product bugs are fixed after the benchmarks, H3 starts a Jocasta that closed itself again, and H3 and H8 run attended

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
The reruns on 0520's build found three product bugs; H3's and H8's reruns failed on two of them, and the queue started the full run early twice. When are the bugs fixed, what does H3 do when Jocasta closes itself, and in what order do H3, H8's 8-thread run and the full run go?

## Facts
Verified 2026-11-02 in the VM.
- A companion that outlived one of H5's stops stopped answering and kept the single-instance lock: from 19:09:51Z every new companion logged "another companion holds the lock but doesn't answer; quitting", so H5's last 6 starts failed and the next sandbox start timed out, until every Jocasta process was stopped at 19:31Z.
- H3 under 0528 (from 19:46:28Z): the companion started herdr again after each of its 12 herdr kills; 92 ms after the 12th start (19:52:07.904Z) it logged "stopping everything", with no thread busy and so no question. After its client exits, `jocasta` takes a herdr that already answers for him quitting the client (`herdrBack`, `src/shared/cli.ts`), so a restart that wins that race closes Jocasta, against 0518. H3 then found no panes and ended (exit 1, 19:54:14Z) with 12 herdr rows and none for the companion or pane faults. Its threads' omps were at omp's setup wizard (below), which the race doesn't depend on.
- H8 under 0529 (20:00–20:09Z): one of 8 `thread_start`s at once failed with `EPERM` renaming the project's `omp.yml` (`omp.yml.5132.tmp`), so no row. A full run was preparing trials beside it, so a race between the starts and that load aren't told apart.
- That full run began at about 19:55Z, when stopping the `full-run-b` job ended its PowerShell but not the bench it had started; it logged 23 infra errors (`git init` or `git add` failed) and no trial row, and its errors are kept aside. The 2 rows of the full run's first early start (19:31–19:39Z) are valid.
- From 19:31Z to 20:52Z setup's files were out of tester's omp folder: `fullrun-prep.ps1` moves them out whenever the full run starts (arm A0 is omp alone), and the sandbox's `jocasta setup` reads its own markers and found nothing to do. Every omp the sandbox started in that time opened omp's setup wizard ("Setup step 1 of 5", sign-in; a thread pane at 20:49Z): H8's 8-thread run of 19:45Z (0529's), the H3 rerun on its threads, and the attended H8 run of 20:44Z, stopped and kept as `h8-jocasta-n8-wizard`. With the files copied back, every pane opened omp's prompt.
- After `stopServer()`, as Stop everything calls it, herdr's next start restored a session's workspaces and panes under their old ids, as shells in new terminals (a throwaway session).
- On a fresh sandbox with setup's files in place, `POST /api/shutdown` (as bare `jocasta` sends it) stopped Jocasta in 9.4 s, and `jocasta` started as H3 starts it (a hidden, detached shell command) brought omp back at its prompt in all 8 workspaces (the boss, demo and six threads) in 20.1 s, under the same workspace ids.
- H3's companion fault already starts Jocasta with `--restart` (default `jocasta`).

## Options
| The three bugs | Pros | Cons |
|---|---|---|
| Report now, fix after the benchmarks | Every row on one build; the bugs are H3's and H8's findings, fixed before the cutover with their own records | H3 keeps meeting the heal race, so its harness starts Jocasta again when it closes itself |
| Fix now, rerun H3 and H8 only | H3 and H8 measure the fixed product | Three records and a rebuild first; rows on two builds; the lock fix touches the start H5 measures |
| Fix now, rerun every Jocasta row | One build for every row, all fixed | About 5 hours more of reruns before the full run, plus the records and the rebuild |

| The order | Pros | Cons |
|---|---|---|
| Full run now, H3 and H8 after | The VM works the next 64 hours while he is away | H3 and H8 after the full run, not before as 0528 and 0529 decided |
| H3 and H8 now, attended, then the full run | His order (0528, 0529) | The agent watches them for about 1.5–2 hours; the session can't pause until then |

## Recommendation
Report now, fix after the benchmarks; full run now, H3 and H8 after.

## Choice
- **The three bugs** (a hung companion keeping the lock; 0518's heal taking a herdr started again for him quitting; a thread start failing on the project's `omp.yml`) are reported now, in `docs/testing.md` and in H3's and H8's notes, and fixed after the benchmarks, before the cutover, each with its own record. Every Jocasta row stays on build `c154055b…004b`.
- **H3, when Jocasta closes itself:** a run that didn't recover, after which no companion answers and herdr lists no panes, counts not recovered with `stoppedItself`. H3 then starts Jocasta with `--restart`; its client stays attached in place of the terminal's, so one client is attached, as 0528 wants. The next run starts once a companion answers and omp is back in every workspace that had it before, that time kept as `restartMs`; not back within 5 minutes, H3 ends there.
- **The order:** H8's 8-thread run, then H3 on its threads, then those threads closed, each run by the agent watching it, with nothing queued behind it; then the full run, its 2 rows kept, its start checked before the session pauses. The H3 rerun that Jocasta closed is kept as `h3-jocasta-selfstop`, the H8 run that failed at its start as `h8-jocasta-n8-eperm`, and the one whose threads sat at omp's setup wizard as `h8-jocasta-n8-wizard`.

## Reason
"Report now, fix after the benchmarks"; "H3 and H8 now, attended, then the full run" (differs from the recommendation).

## Consequences
- Amends 0482's H3 (a Jocasta that closes itself during a run) and 0528 (the restart's client that stays) before the run, as 0482 asks of any change.
- The fixes for the three bugs are decided after the benchmarks, each in its own record, before the cutover (0487).
- The full run starts after H3.
- The sandbox's runs have setup's files in tester's omp folder, as every earlier Jocasta row had; `fullrun-prep.ps1` moves them out again before the full run, and each attended step checks for them first.
- 0529's 19:45Z run sat at omp's setup wizard rather than before its briefs had landed; its rule stands, since a thread at the wizard has no commit.
- Amended by 0531: H8's attended 8-thread run ran out of the VM's memory; it runs once more on a clean sandbox, and running out again is its result.
- Amended by 0534: the three bugs are fixed in the harness-OS build, before the benchmarks run again.
