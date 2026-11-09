# 0522 H3 counts a herdr or pane fault recovered when omp is back in every workspace

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
H3 counts the herdr fault recovered when herdr lists exactly the old pane ids, and the pane fault when the same pane id has an agent again. A pane Jocasta relaunches gets a new id. Change the check?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox.
- After a kill of herdr's server (09:43Z), herdr listed the old pane ids 2.3 s later. 0.3 s after that the service began replacing every restored pane with a new one running omp, under new ids; all were replaced within 5.5 s.
- H3 polls every 250 ms for exactly the old set of ids (`samePaneIds`, `bench/h3-recovery.ts`), so it can miss that window and count a recovery as a failure.
- A relaunched pane never has its old id, so the pane fault's check (the same id with an agent) can't pass once 0521's restart replaces the pane.

## Options
| Option | Pros | Cons |
|---|---|---|
| Recovered = omp back | Measures the whole recovery, what he sees; no missed window | Changes 0482's pre-registered check before the run |
| Keep as pre-registered | As pre-registered | Misses count recoveries as failures; it never sees whether omp came back |

## Recommendation
Recovered = omp back.

## Choice
The herdr and pane faults count as recovered when every workspace that had an omp pane before the fault has an omp pane again. For the herdr fault, the time until herdr listed the old pane ids again stays in each row as a second figure. The companion fault keeps its check (a new companion answers and herdr lists the same panes).

## Reason
"Recovered = omp back".

## Consequences
- Amends 0482's H3 before the run, as 0482 asks of any change.
- Amended by 0528: the companion fault, too, counts a run recovered when a new companion answers and omp is back in every workspace.
