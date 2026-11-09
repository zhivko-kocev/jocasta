# 0528 H3 kills each folder's omp at most once in 10 minutes, the companion fault counts omp back everywhere, and each restart's client ends

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
H3's run on 0520's build had misses that measured something other than recovery. How does H3 measure the pane and companion faults, what happens to the clients its restarts leave, and when does it run again?

## Facts
Verified 2026-11-02 in the VM (`D:/thesis/data/bench/h3/h3-jocasta-unspaced/`, 16:00–18:46Z, 20 runs per fault, 5 s between faults).
- Recovered: herdr 18 of 20 (median 9.8 s), companion 14 of 20 (median 1.0 s), pane 12 of 20 (median 8.8 s); folders intact and sessions parse in all 60.
- The pane fault kills "an omp pane at work, else the first" (`pickPane`). It hit the boss's folder and one H8 thread's folder in turn, and each folder's recovery took 6.5, 6.7, 12.2, 32.0 and 72.2 s before passing H3's 120 s limit.
- 0521's wait is per folder (`ompEnded` in `src/frontier/server/service.ts` keys on the pane's real path): none at the first ending, then 10 s doubling to at most 5 minutes for each ending in a row, counted again from zero 10 minutes after the last ending that started omp again.
- The companion fault counts a run recovered when a new companion answers and herdr lists the same pane ids (0522 kept that check). The new companion relaunches the boss pane under a new id with its new key, so whether a poll sees the old ids first is timing: 6 of 20 missed.
- Each companion run's restart (`jocasta`, spawned hidden and detached by the bench) stayed attached to herdr as a client: 33 `jocasta.exe` started 16:17–16:31Z were still running at 18:47Z. Bare `jocasta` starts the companion through a `cmd` that exits at once (`src/entry.ts` `startCompanion`), so the companion isn't in the restart's process tree.
- The herdr fault's 2 misses can't be traced: the next fresh install wiped the sandbox's companion log.

## Options
| The pane fault | Pros | Cons |
|---|---|---|
| One kill per folder per 10 minutes, run again | Every run meets a first ending as 0521 defines it, so the rows measure recovery; about 25–30 minutes | A harness change and a rerun |
| Keep these rows, labelled | No rerun | The rate and median mix recovery with 0521's waits |
| Measure the throttle as well | Shows both behaviours | More harness code; the bench rebuilds Jocasta's rule |

| The companion fault | Pros | Cons |
|---|---|---|
| Back when omp is back everywhere | 0522's rule, as for the other faults; no timing misses | Amends 0522; a rerun |
| Keep the same-panes rule | No change | 30% of runs fail on timing |

| The restarts' clients | Pros | Cons |
|---|---|---|
| End each restart's client before the next fault | One client at a time, as one person restarting once | A small harness change |
| Leave them | No change | 20 hidden clients are not a real setup |

| When | Pros | Cons |
|---|---|---|
| Before the full run | On H8's 8-thread run's threads, kept open for it; every benchmark done before the 64-hour run | The full run starts about 1–1.5 hours later |
| After the full run | The full run starts as queued | H3 waits 64 hours, then needs setup's files back and H8 again |

## Recommendation
One kill per folder per 10 minutes, run again; back when omp is back everywhere; end each restart's client; before the full run.

## Choice
- **The pane fault** kills the omp pane whose folder it killed longest ago (one at work first among equals) and waits, when it has to, until 10 minutes and 30 seconds have passed since that folder's last run ended, so every kill meets a first ending (0521).
- **The companion fault** counts a run recovered when a new companion answers and every workspace that had an omp pane before has one again; `samePanes` stays in each row.
- **Each restart's client** ends after its run's checks, before the next fault: the restart's process tree, root first, without the new companion's tree.
- **H3 runs again** before the full run, on the threads H8's 8-thread run leaves open; the first run's rows stay in the kit as `h3-jocasta-unspaced`, and the companion's logs are kept beside the new rows.

## Reason
"One kill per folder per 10 min, run again"; "Back when omp is back everywhere"; "End each restart's client before the next fault"; "Before the full run".

## Consequences
- Amends 0482's H3 (the pane fault's choice and spacing; the restarts' clients) and 0522 (the companion fault's check) before the run, as 0482 asks of any change.
- The full run starts after H3.
- Amended by 0530: after a run in which Jocasta closed itself, H3 starts it again with `--restart` and that client stays attached, in place of the terminal's, so one client is still attached at a time.
